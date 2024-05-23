mod auth;
mod handler;
mod socket;
mod v2;
mod v4;
mod v6;

use std::{
    borrow::Borrow,
    collections::HashMap,
    fs,
    future::Future,
    io::{self, Write},
    net::SocketAddr,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use drop_auth::Nonce;
use handler::{Downloader, HandlerInit, HandlerLoop};
use hyper::StatusCode;
use slog::{debug, error, info, warn, Logger};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver},
        Mutex,
    },
    task::{AbortHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use warp::{ws::Message, Filter};

use self::socket::{WebSocket, WsStream};
use super::{events::FileEventTx, IncomingFileEventTx};
use crate::{
    check,
    file::{self, FileSubPath, FileToRecv},
    protocol,
    quarantine::PathExt,
    service::State,
    tasks::AliveGuard,
    transfer::{IncomingTransfer, Transfer},
    utils::Hidden,
    ws::{
        server::handler::{MsgToSend, Request},
        Pinger,
    },
    Error, File, FileId,
};

const MAX_FILENAME_LENGTH: usize = 255;
const MAX_FILE_SUFFIX_LEN: usize = 5; // Assume that the suffix will fit into 5 characters e.g.
                                      // `<filename>(999).<ext>`
const REPORT_PROGRESS_THRESHOLD: u64 = 1024 * 64;

pub enum ServerReq {
    Download { task: Box<FileXferTask> },
    Start { file: FileId, offset: u64 },
    Reject { file: FileId },
    Done { file: FileId },
    Fail { file: FileId, msg: String },
    Close,
}

pub struct FileXferTask {
    pub file: FileToRecv,
    pub xfer: Arc<IncomingTransfer>,
    pub base_dir: Hidden<PathBuf>,
}

pub struct FileStreamCtx<'a> {
    jobs: &'a mut JoinSet<()>,
    guard: AliveGuard,
    state: Arc<State>,
    logger: Logger,
    req_send: mpsc::UnboundedSender<ServerReq>,
    task: FileXferTask,
}

pub struct TmpFileState {
    meta: fs::Metadata,
    csum: [u8; 32],
}

struct StreamCtx<'a> {
    logger: &'a Logger,
    state: &'a State,
    tmp_loc: &'a Hidden<PathBuf>,
    stream: &'a mut UnboundedReceiver<Vec<u8>>,
    events: &'a FileEventTx<IncomingTransfer>,
}

#[derive(Debug)]
struct MissingAuth {
    peer: SocketAddr,
    authorization: auth::Authorization,
}
impl warp::reject::Reject for MissingAuth {}

#[derive(Debug)]
struct Unauthorized;
impl warp::reject::Reject for Unauthorized {}

#[derive(Debug)]
struct ToManyReqs;
impl warp::reject::Reject for ToManyReqs {}

#[derive(Debug)]
struct BadRequest;
impl warp::reject::Reject for BadRequest {}

pub(crate) fn spawn(
    refresh_trigger: tokio::sync::watch::Receiver<()>,
    state: Arc<State>,
    logger: Logger,
    stop: CancellationToken,
    alive: AliveGuard,
) -> crate::Result<()> {
    let addr = SocketAddr::new(state.addr, drop_config::PORT);

    let nonce_store = Arc::new(Mutex::new(HashMap::new()));

    let service = {
        let rate_limiter = Arc::new(governor::RateLimiter::dashmap(governor::Quota::per_second(
            drop_config::MAX_REQUESTS_PER_SEC
                .try_into()
                .map_err(|_| crate::Error::InvalidArgument)?,
        )));

        let remote = warp::filters::addr::remote()
            .map(move |peer: Option<SocketAddr>| peer.expect("Transport should use IP addresses"));

        let ddos = remote
            .and_then(move |peer: SocketAddr| {
                let check = rate_limiter.check_key(&peer.ip());
                async move {
                    match check {
                        Ok(_) => Ok(()),
                        Err(_) => Err(warp::reject::custom(ToManyReqs)),
                    }
                }
            })
            .untuple_one();

        let route =
            warp::path("drop").and(warp::path::param().and_then(|version: String| async move {
                version
                    .parse::<protocol::Version>()
                    .map_err(|_| warp::reject::not_found())
            }));

        let base = remote
            .and(route)
            .and(warp::filters::header::optional(
                drop_auth::http::Authorization::KEY,
            ))
            .and(
                warp::filters::header::optional(drop_auth::http::WWWAuthenticate::KEY)
                    .map(auth::WWWAuthenticate::new),
            );

        let ws_route = {
            let logger = logger.clone();
            let nonces = nonce_store.clone();
            let alive = alive.clone();
            let stop = stop.clone();
            let state = state.clone();

            base.and(warp::ws()).and_then(
                move |peer: SocketAddr,
                      version: protocol::Version,
                      auth_header: Option<String>,
                      www_auth: auth::WWWAuthenticate,
                      ws: warp::ws::Ws| {
                    let state = Arc::clone(&state);
                    let alive = alive.clone();
                    let stop = stop.clone();
                    let logger = logger.clone();
                    let nonces = nonces.clone();
                    let refresh_trigger = refresh_trigger.clone();

                    async move {
                        let authorization = process_authentication(
                            &state.auth,
                            &nonces,
                            peer,
                            version,
                            auth_header,
                            www_auth,
                            &logger,
                        )
                        .await?;

                        let reply = ws.on_upgrade(move |socket| async move {
                            info!(logger, "Client requested protocol version: {}", version);
                            websocket_start(
                                socket,
                                state,
                                alive,
                                stop,
                                version,
                                peer,
                                logger,
                                refresh_trigger,
                            )
                            .await;
                        });

                        Ok::<_, warp::Rejection>(authorization.insert(reply))
                    }
                },
            )
        };

        let check_route = {
            let nonces = nonce_store.clone();
            let logger = logger.clone();

            base.and(warp::path!("check" / String))
                .and(warp::get())
                .and_then(move |peer, version, auth_header, www_auth, uuid: String| {
                    let state = Arc::clone(&state);
                    let nonces = nonces.clone();
                    let logger = logger.clone();

                    async move {
                        let authorization = process_authentication(
                            &state.auth,
                            &nonces,
                            peer,
                            version,
                            auth_header,
                            www_auth,
                            &logger,
                        )
                        .await?;

                        let uuid = uuid.parse().map_err(|_| warp::reject::custom(BadRequest))?;
                        let status = if state.transfer_manager.is_outgoing_alive(uuid).await {
                            StatusCode::OK
                        } else {
                            StatusCode::GONE
                        };

                        Ok::<_, warp::Rejection>(authorization.insert(status))
                    }
                })
        };

        ddos.and(ws_route.or(check_route)).recover(move |err| {
            let nonces = Arc::clone(&nonce_store);
            async move { handle_rejection(&nonces, err).await }
        })
    };

    let future =
        match warp::serve(service).try_bind_with_graceful_shutdown(addr, stop.cancelled_owned()) {
            Ok((socket, future)) => {
                debug!(logger, "WS server is bound to: {socket}");
                future
            }
            Err(err) => {
                // Check if this is IO error about address already in use
                if let Some(ioerr) = std::error::Error::source(&err)
                    .and_then(|src| src.downcast_ref::<hyper::Error>())
                    .and_then(std::error::Error::source)
                    .and_then(|src| src.downcast_ref::<io::Error>())
                {
                    if ioerr.kind() == io::ErrorKind::AddrInUse {
                        error!(
                            logger,
                            "Found that the address {addr} is already used, while trying to bind \
                             the WS server: {ioerr}",
                        );
                        return Err(Error::AddrInUse);
                    }
                }

                return Err(err.into());
            }
        };

    tokio::spawn(async move {
        let _guard = alive;
        future.await;
        debug!(logger, "WS server stopped");
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn websocket_start(
    socket: warp::ws::WebSocket,
    state: Arc<State>,
    alive: AliveGuard,
    stop: CancellationToken,
    version: protocol::Version,
    peer: SocketAddr,
    logger: Logger,
    refresh_trigger: tokio::sync::watch::Receiver<()>,
) {
    let ctx = RunContext {
        logger: &logger,
        state: state.clone(),
        stop: &stop,
        alive: &alive,
        refresh_trigger: &refresh_trigger,
    };

    match version {
        protocol::Version::V1 => {
            ctx.run(
                socket,
                v2::HandlerInit::<false>::new(peer.ip(), &state, &logger),
            )
            .await
        }
        protocol::Version::V2 => {
            ctx.run(
                socket,
                v2::HandlerInit::<true>::new(peer.ip(), &state, &logger),
            )
            .await
        }
        protocol::Version::V4 => {
            ctx.run(
                socket,
                v4::HandlerInit::new(peer.ip(), state, &logger, &alive),
            )
            .await
        }
        protocol::Version::V5 => {
            ctx.run(
                socket,
                v6::HandlerInit::new(peer.ip(), state, &logger, &alive),
            )
            .await
        }
        protocol::Version::V6 => {
            ctx.run(
                socket,
                v6::HandlerInit::new(peer.ip(), state, &logger, &alive),
            )
            .await
        }
    }
}

async fn process_authentication(
    auth: &crate::auth::Context,
    nonces: &Mutex<HashMap<SocketAddr, Nonce>>,
    peer: SocketAddr,
    version: protocol::Version,
    clients_authorization_header: Option<String>,
    www_auth: auth::WWWAuthenticate,
    logger: &Logger,
) -> Result<auth::Authorization, warp::Rejection> {
    // Uncache the peer nonce first
    let nonce = nonces.lock().await.remove(&peer);

    match version {
        protocol::Version::V1 | protocol::Version::V2 => (),
        _ => {
            let Some(auth_header) = clients_authorization_header else {
                return Err(warp::reject::custom(MissingAuth {
                    peer,
                    authorization: www_auth.authorize(auth, peer, logger),
                }));
            };

            let nonce = nonce.ok_or_else(|| warp::reject::custom(Unauthorized))?;

            if !auth.authorize(peer.ip(), &auth_header, &nonce) {
                return Err(warp::reject::custom(Unauthorized));
            }
        }
    };

    Ok(www_auth.authorize(auth, peer, logger))
}

async fn handle_rejection(
    nonces: &Mutex<HashMap<SocketAddr, Nonce>>,
    err: warp::Rejection,
) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
    if let Some(MissingAuth {
        peer,
        authorization,
    }) = err.find()
    {
        let nonce = Nonce::generate_as_server();
        let (header_key, header_val) = crate::auth::create_www_authentication_header(&nonce);

        nonces.lock().await.insert(*peer, nonce);

        let reply = authorization.insert(warp::reply::with_header(
            StatusCode::UNAUTHORIZED,
            header_key,
            header_val,
        ));

        Ok(reply)
    } else if let Some(Unauthorized) = err.find() {
        Ok(Box::new(StatusCode::UNAUTHORIZED))
    } else if let Some(ToManyReqs) = err.find() {
        Ok(Box::new(StatusCode::TOO_MANY_REQUESTS))
    } else if let Some(BadRequest) = err.find() {
        Ok(Box::new(StatusCode::BAD_REQUEST))
    } else {
        Err(err)
    }
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: Arc<State>,
    refresh_trigger: &'a tokio::sync::watch::Receiver<()>,
    stop: &'a CancellationToken,
    alive: &'a AliveGuard,
}

impl RunContext<'_> {
    async fn run(self, socket: WsStream, mut handler: impl HandlerInit) {
        let mut socket =
            WebSocket::new(socket, handler.recv_timeout(), drop_config::WS_SEND_TIMEOUT);

        let recv_task = handler.recv_req(&mut socket);

        let xfer = tokio::select! {
            biased;

            _ = self.stop.cancelled() => {
                debug!(self.logger, "Stopping client request on shutdown");
                return;
            },
            r = recv_task => {
                match r {
                    Ok(xfer) => xfer,
                    Err(err) => {
                        error!(self.logger, "Failed to initiate transfer: {:?}", err);
                        return;
                    }
                }
            },
        };

        let xfer = match xfer.parse() {
            Ok(xfer) => {
                debug!(self.logger, "RunContext::run() called with {:?}", xfer);
                xfer
            }
            Err(err) => {
                if let Err(err) = handler.on_error(&mut socket, err).await {
                    error!(
                        self.logger,
                        "Failed to close connection on invalid request: {:?}", err
                    );
                }

                return;
            }
        };

        let xfer = Arc::new(xfer);
        let xfer_id = xfer.id();

        let job = async {
            self.client_loop(socket, handler, xfer).await;

            // The error indicates the transfer is already finished. That's fine
            let _ = self
                .state
                .transfer_manager
                .incoming_disconnect(xfer_id)
                .await;
        };

        tokio::select! {
            biased;

            _ = self.stop.cancelled() => {
                debug!(self.logger, "Server job stop: {xfer_id}");
            }
            _ = job => (),
        }
    }

    async fn client_loop(
        &self,
        mut socket: WebSocket,
        mut handler: impl handler::HandlerInit,
        xfer: Arc<IncomingTransfer>,
    ) {
        let (req_send, mut req_rx) = mpsc::unbounded_channel();

        if let Err(err) = self.init_manager(req_send.clone(), &xfer).await {
            error!(self.logger, "Failed to init trasfer: {err:?}");
            if let Err(e) = handler.on_error(&mut socket, err).await {
                warn!(
                    self.logger,
                    "Failed to close connection on invalid request: {:?}", e
                );
            }
            return;
        }

        let mut ping = handler.pinger();

        let (send_tx, mut send_rx) = mpsc::channel(2);
        let mut jobs = JoinSet::new();

        let mut handler = if let Some(handler) = handler
            .upgrade(&mut socket, &mut jobs, send_tx, xfer.clone())
            .await
        {
            handler
        } else {
            return;
        };

        let task = async {
            loop {
                tokio::select! {
                    biased;

                    // API request
                    req = req_rx.recv() => {
                        if self.on_req(&mut socket, &mut jobs, &mut handler, &xfer, &req_send, req).await?.is_break() {
                            break;
                        }
                    },
                    // Message received
                    recv = socket.recv() => {
                        let msg =  recv.context("Failed to receive WS message")?;

                        if self.on_recv(&mut socket, &mut handler, &xfer, msg).await?.is_break() {
                            break;
                        }
                    },
                    // Message to send down the wire
                    msg = send_rx.recv() => {
                        let MsgToSend { msg } = msg.expect("Handler channel should always be open");
                        socket.send(msg).await?;
                    },
                    _ = ping.tick() => {
                        socket.send(Message::ping(Vec::new())).await.context("Failed to send PING message")?;
                    }
                };
            }
            anyhow::Ok(())
        };

        let result = task.await;
        info!(self.logger, "Connection loop finished");

        jobs.shutdown().await;

        if let Err(err) = result {
            info!(
                self.logger,
                "WS connection broke for {}: {err:?}",
                xfer.id()
            );
            handler.finalize_failure().await;
        } else {
            info!(self.logger, "Sucesfully finalizing transfer loop");
            handler.finalize_success().await;
        }
    }

    async fn init_manager(
        &self,
        req_send: mpsc::UnboundedSender<ServerReq>,
        xfer: &Arc<IncomingTransfer>,
    ) -> anyhow::Result<()> {
        let is_new = self
            .state
            .transfer_manager
            .register_incoming(xfer.clone(), req_send)
            .await?;

        if let Some(xfer_tx) = is_new {
            xfer_tx.received().await;

            check::spawn(
                self.refresh_trigger.clone(),
                self.state.clone(),
                xfer.clone(),
                self.logger.clone(),
                self.alive.clone(),
                self.stop.clone(),
            );
        }

        Ok(())
    }

    async fn on_recv(
        &self,
        socket: &mut WebSocket,
        handler: &mut impl HandlerLoop,
        xfer: &Arc<IncomingTransfer>,
        msg: Message,
    ) -> anyhow::Result<ControlFlow<()>> {
        if let Ok(text) = msg.to_str() {
            debug!(self.logger, "Received:\n\t{text}");
            handler.on_text_msg(socket, text).await?;
        } else if msg.is_binary() {
            handler.on_bin_msg(socket, msg.into_bytes()).await?;
        } else if msg.is_close() {
            debug!(self.logger, "Got CLOSE frame");

            handler.on_close().await;

            if let Some(state) = self.state.transfer_manager.incoming_remove(xfer.id()).await {
                state.xfer_events.cancel(true).await
            }

            return Ok(ControlFlow::Break(()));
        } else if msg.is_ping() {
            debug!(self.logger, "PING");
        } else if msg.is_pong() {
            debug!(self.logger, "PONG");
        } else {
            warn!(self.logger, "Server received invalid WS message type");
        }

        anyhow::Ok(ControlFlow::Continue(()))
    }

    async fn on_req(
        &self,
        socket: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        handler: &mut impl HandlerLoop,
        xfer: &Arc<IncomingTransfer>,
        req_send: &mpsc::UnboundedSender<ServerReq>,
        req: Option<ServerReq>,
    ) -> anyhow::Result<ControlFlow<()>> {
        match req.context("API channel broken")? {
            ServerReq::Download { task } => {
                let ctx = FileStreamCtx {
                    jobs,
                    guard: self.alive.clone(),
                    state: self.state.clone(),
                    logger: self.logger.clone(),
                    req_send: req_send.clone(),
                    task: *task,
                };

                handler.start_download(ctx).await?
            }
            ServerReq::Start { file, offset } => handler.issue_start(socket, file, offset).await?,
            ServerReq::Reject { file } => handler.issue_reject(socket, file).await?,
            ServerReq::Done { file } => handler.issue_done(socket, file).await?,
            ServerReq::Fail { file, msg } => handler.issue_failure(socket, file, msg).await?,

            ServerReq::Close => {
                debug!(self.logger, "Stoppping server connection gracefuly");
                socket.send(Message::close()).await?;
                handler.on_close().await;
                socket.drain().await.context("Failed to drain the socket")?;

                self.state.transfer_manager.incoming_remove(xfer.id()).await;
                return Ok(ControlFlow::Break(()));
            }
        }

        Ok(ControlFlow::Continue(()))
    }
}

impl FileXferTask {
    pub fn new(file: FileToRecv, xfer: Arc<IncomingTransfer>, base_dir: PathBuf) -> Self {
        Self {
            file,
            xfer,
            base_dir: Hidden(base_dir),
        }
    }

    async fn stream_file(
        &mut self,
        StreamCtx {
            logger,
            state,
            tmp_loc,
            stream,
            events,
        }: StreamCtx<'_>,
        downloader: &mut impl Downloader,
        offset: u64,
        emit_checksum_events: bool,
        checksum_events_granularity: u64,
    ) -> crate::Result<PathBuf> {
        let mut out_file = match downloader.open(tmp_loc).await {
            Ok(out_file) => out_file,
            Err(err) => {
                error!(
                    logger,
                    "Could not create {tmp_loc:?} for downloading: {err}"
                );

                return Err(err);
            }
        };

        let consume_file_chunks = async {
            let mut bytes_received = offset;
            let mut last_progress = bytes_received;

            // Announce initial state of the transfer
            downloader.progress(bytes_received).await?;
            events.progress(bytes_received).await;

            while bytes_received < self.file.size() {
                let chunk = stream.recv().await.ok_or(crate::Error::Canceled)?;

                let chunk_size = chunk.len();
                if chunk_size as u64 + bytes_received > self.file.size() {
                    return Err(crate::Error::MismatchedSize);
                }

                out_file.write_all(&chunk)?;

                bytes_received += chunk_size as u64;

                if last_progress + REPORT_PROGRESS_THRESHOLD <= bytes_received {
                    // send progress to the caller
                    downloader.progress(bytes_received).await?;
                    events.progress(bytes_received).await;

                    last_progress = bytes_received;
                }
            }

            // Close the file handle
            drop(out_file);

            if bytes_received > self.file.size() {
                return Err(crate::Error::UnexpectedData);
            }

            if emit_checksum_events {
                events.finalize_checksum_start(self.file.size()).await;
                let progress_cb = {
                    move |progress_bytes: u64| async move {
                        events.finalize_checksum_progress(progress_bytes).await;
                    }
                };

                downloader
                    .validate(
                        tmp_loc,
                        Some(progress_cb),
                        Some(checksum_events_granularity),
                    )
                    .await?;

                events.finalize_checksum_finish().await;
            } else {
                downloader
                    .validate::<_, futures::future::Ready<()>>(
                        tmp_loc,
                        None::<fn(u64) -> futures::future::Ready<()>>,
                        None,
                    )
                    .await?;
            }

            Ok(())
        };

        match consume_file_chunks.await {
            Err(err @ crate::Error::Canceled) => return Err(err), // Do not remove temp file
            // when cancelled. We might
            // resume
            Err(err) => {
                if let Err(ioerr) = fs::remove_file(&tmp_loc.0) {
                    error!(
                        logger,
                        "Could not remove temporary file {tmp_loc:?} after failed download: {}",
                        ioerr
                    );
                }

                return Err(err);
            }
            _ => (),
        };

        let dst = match self.place_file_into_dest(state, logger, tmp_loc).await {
            Ok(dst) => {
                info!(
                    logger,
                    "Sucesfully placed file for id {} into destination: {tmp_loc:?} -> {:?}",
                    self.file.id(),
                    Hidden(&dst)
                );

                dst
            }
            Err(err) => {
                error!(
                    logger,
                    "Could not rename temporary file of {} after downloading: {err}",
                    self.file.id(),
                );
                return Err(err);
            }
        };

        Ok(dst)
    }

    async fn prepare_abs_path(&self, state: &State) -> crate::Result<PathBuf> {
        let mut lock = state.transfer_manager.incoming.lock().await;

        let state = lock
            .get_mut(&self.xfer.id())
            .ok_or(crate::Error::Canceled)?;

        let mapping = state
            .dir_mappings
            .compose_final_path(&self.base_dir, self.file.subpath())?;

        drop(lock);

        Ok(self.base_dir.join(mapping))
    }

    async fn place_file_into_dest(
        &self,
        state: &State,
        logger: &Logger,
        tmp_location: &Hidden<PathBuf>,
    ) -> crate::Result<PathBuf> {
        let abs_path = self.prepare_abs_path(state).await?;
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let dst = move_tmp_to_dst(tmp_location, Hidden(&abs_path), logger)?;

        Ok(dst)
    }

    async fn handle_tmp_file(
        &mut self,
        logger: &Logger,
        events: &FileEventTx<IncomingTransfer>,
        tmp_location: &Hidden<PathBuf>,
        emit_checksum_events: bool,
        checksum_events_granularity: u64,
    ) -> Option<TmpFileState> {
        // TODO: we load the file's metadata to check if we should emit checksum events
        // based on size threshold. However TmpFileState::load also does the
        // same thing. We should refactor this to avoid double loading.
        let tmp_size = fs::File::open(&tmp_location.0)
            .and_then(|file| file.metadata())
            .map(|metadata| metadata.len())
            .ok();

        let will_emit_checksum_events = emit_checksum_events && tmp_size.is_some();

        let cb = if will_emit_checksum_events {
            let size = tmp_size.unwrap_or(0);
            events.verify_checksum_start(size).await;

            Some(|progress_bytes| events.verify_checksum_progress(progress_bytes))
        } else {
            None
        };

        // Check if we can resume the temporary file
        let tmp_file_state = match TmpFileState::load(
            &tmp_location.0,
            cb,
            Some(checksum_events_granularity),
        )
        .await
        {
            Ok(tmp_file_state) => {
                debug!(
                    logger,
                    "Found temporary file: {:?}, of size: {}",
                    tmp_location.0,
                    tmp_file_state.meta.len()
                );
                Some(tmp_file_state)
            }
            Err(err) => {
                debug!(logger, "Failed to load temporary file info: {err}");
                return None;
            }
        };

        if will_emit_checksum_events {
            events.verify_checksum_finish().await;
        }

        tmp_file_state
    }

    #[allow(clippy::too_many_arguments)]
    async fn run(
        mut self,
        state: Arc<State>,
        events: Arc<FileEventTx<IncomingTransfer>>,
        mut downloader: impl Downloader,
        mut stream: UnboundedReceiver<Vec<u8>>,
        req_send: mpsc::UnboundedSender<ServerReq>,
        logger: Logger,
        guard: AliveGuard,
    ) {
        let task = async {
            validate_subpath_for_download(self.file.subpath())?;

            let emit_checksum_events = {
                if let Some(threshold) = state.config.checksum_events_size_threshold {
                    self.file.size() >= threshold as u64
                } else {
                    false
                }
            };
            let checksum_events_granularity = state.config.checksum_events_granularity;

            events.preflight().await;

            let tmp_location: Hidden<PathBuf> = Hidden(
                self.base_dir
                    .join(temp_file_name(self.xfer.id(), self.file.id())),
            );

            let tmp_file_state = self
                .handle_tmp_file(
                    &logger,
                    &events,
                    &tmp_location,
                    emit_checksum_events,
                    checksum_events_granularity,
                )
                .await;

            let init_res = downloader.init(&self, tmp_file_state).await?;

            match init_res {
                handler::DownloadInit::Stream { offset } => {
                    if req_send
                        .send(ServerReq::Start {
                            file: self.file.id().clone(),
                            offset,
                        })
                        .is_err()
                    {
                        debug!(logger, "Client is disconnected. Stopping file stream");
                        return Err(Error::Canceled);
                    }

                    events.start(self.base_dir.to_string_lossy(), offset).await;

                    self.stream_file(
                        StreamCtx {
                            logger: &logger,
                            state: &state,
                            tmp_loc: &tmp_location,
                            stream: &mut stream,
                            events: &events,
                        },
                        &mut downloader,
                        offset,
                        emit_checksum_events,
                        checksum_events_granularity,
                    )
                    .await
                }
            }
        };

        let result = task.await;

        // This is a critical part that we need to execute atomically.
        // Since the outter task can be aborted, let's move it to a separate task
        // so that it's never interrupted.
        let error_logger = logger.clone();
        if let Err(e) = tokio::spawn(async move {
            let _guard = guard;

            match result {
                Err(crate::Error::Canceled) => {
                    info!(logger, "File {} stopped", self.file.id())
                }
                Ok(dst_location) => {
                    info!(logger, "File {} downloaded succesfully", self.file.id());

                    if let Err(err) = state
                        .transfer_manager
                        .incoming_finish_post(self.xfer.id(), self.file.id(), true)
                        .await
                    {
                        warn!(logger, "Failed to post finish: {err}");
                    }

                    if let Err(e) = req_send.send(ServerReq::Done {
                        file: self.file.id().clone(),
                    }) {
                        warn!(logger, "Failed to send DONE message: {}", e);
                    };

                    events.success(dst_location).await;
                }
                Err(err) => {
                    info!(
                        logger,
                        "File {} finished with error: {err:?}",
                        self.file.id()
                    );

                    if let Err(err) = state
                        .transfer_manager
                        .incoming_finish_post(self.xfer.id(), self.file.id(), false)
                        .await
                    {
                        warn!(logger, "Failed to post finish: {err}");
                    }

                    if let Err(e) = req_send.send(ServerReq::Fail {
                        file: self.file.id().clone(),
                        msg: err.to_string(),
                    }) {
                        warn!(logger, "Failed to send FAIL message: {}", e);
                    };

                    events.failed(err).await;
                }
            }
        })
        .await
        {
            error!(error_logger, "Failed to spawn file xfer task: {:?}", e);
        };
    }
}

impl TmpFileState {
    // Blocking operation
    async fn load<F, Fut>(
        path: &Path,
        progress_cb: Option<F>,
        event_granularity: Option<u64>,
    ) -> io::Result<Self>
    where
        F: Fn(u64) -> Fut + Sync + Send,
        Fut: Future<Output = ()>,
    {
        let file = fs::File::open(path)?;

        let meta = file.metadata()?;

        let csum = file::checksum(file, progress_cb, event_granularity).await?;
        Ok(TmpFileState { meta, csum })
    }
}

fn validate_tmp_location_path(tmp_location: &Hidden<PathBuf>) -> crate::Result<()> {
    let char_count = tmp_location
        .file_name()
        .expect("Cannot extract filename")
        .len();

    if char_count > MAX_FILENAME_LENGTH {
        return Err(crate::Error::FilenameTooLong);
    }

    Ok(())
}

fn move_tmp_to_dst(
    tmp_location: &Hidden<PathBuf>,
    absolute_path: Hidden<&Path>,
    logger: &Logger,
) -> crate::Result<PathBuf> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);

    let mut iter = crate::utils::filepath_variants(absolute_path.0)?;
    let dst_location = loop {
        let path = iter.next().expect("File paths iterator should never end");

        match opts.open(&path) {
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                continue;
            }
            Err(err) => {
                // On Win the permissions error is returned in case there's a
                // directory with the same name. Let's do it for all OSes since
                // there should be no harm.
                if path.exists() {
                    continue;
                }

                error!(logger, "Failed to crate destination file: {err}");
                return Err(err.into());
            }
            Ok(file) => {
                drop(file); // Close the file
                break path;
            }
        }
    };

    if let Err(err) = fs::rename(&tmp_location.0, &dst_location) {
        if let Err(err) = fs::remove_file(&dst_location) {
            warn!(
                logger,
                "Failed to remove touched destination file on move error: {err}"
            );
        }
        return Err(err.into());
    }

    if let Err(err) = dst_location.quarantine() {
        error!(logger, "Failed to quarantine downloaded file: {err}");
    }

    Ok(dst_location)
}

impl<'a> FileStreamCtx<'a> {
    async fn start(
        self,
        downloader: impl Downloader + Send + 'static,
        stream: UnboundedReceiver<Vec<u8>>,
    ) -> anyhow::Result<(AbortHandle, Arc<IncomingFileEventTx>)> {
        let events = self
            .state
            .transfer_manager
            .incoming_file_events(self.task.xfer.id(), self.task.file.id())
            .await?;

        let job = {
            let events = events.clone();

            let Self {
                jobs,
                guard,
                state,
                logger,
                req_send,
                task,
            } = self;

            jobs.spawn(async move {
                let _guard = guard.clone();

                task.run(state, events, downloader, stream, req_send, logger, guard)
                    .await;
            })
        };

        Ok((job, events))
    }
}

pub fn remove_temp_files<P, I>(
    logger: &Logger,
    transfer_id: uuid::Uuid,
    iter: impl IntoIterator<Item = (P, I)>,
) where
    P: Into<PathBuf>,
    I: Borrow<FileId>,
{
    for (base, file_id) in iter.into_iter() {
        let file_id = file_id.borrow();
        let location = base.into().join(temp_file_name(transfer_id, file_id));
        let location = Hidden(location);

        debug!(logger, "Removing temporary file: {location:?}");
        match std::fs::remove_file(&*location) {
            Ok(()) => (),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => {
                error!(
                    logger,
                    "Failed to delete temporary file, id: {file_id}, path {location:?}, {err:?}",
                );
            }
        }
    }
}

fn temp_file_name(transfer_id: uuid::Uuid, file_id: &FileId) -> String {
    format!("{}-{file_id}.dropdl-part", transfer_id.as_simple(),)
}

/// Check file and dir names are shorter then MAX and contain illegal values
fn validate_subpath_for_download(subpath: &FileSubPath) -> crate::Result<()> {
    const DISALLOWED: &[&str] = &[".."];

    for name in subpath.iter() {
        if name.len() + MAX_FILE_SUFFIX_LEN > MAX_FILENAME_LENGTH {
            return Err(Error::FilenameTooLong);
        }

        if DISALLOWED.contains(&name.as_str()) {
            return Err(Error::BadPath(
                "File subpath contains disallowed element".into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::file::FileSubPath;

    #[test]
    fn validate_subpath() {
        let sp = FileSubPath::from_path("abc/dfg/hjk.txt").unwrap();
        assert!(super::validate_subpath_for_download(&sp).is_ok());

        let sp = FileSubPath::from_path("abc/../hjk.txt").unwrap();
        assert!(matches!(
            super::validate_subpath_for_download(&sp),
            Err(crate::Error::BadPath(..))
        ));

        let mut path = String::from("abc/");
        path.extend(std::iter::repeat('x').take(251));
        path.push_str("/hjk.txt");
        let sp = FileSubPath::from_path(&path).unwrap();
        assert!(matches!(
            super::validate_subpath_for_download(&sp),
            Err(crate::Error::FilenameTooLong)
        ));
    }
}
