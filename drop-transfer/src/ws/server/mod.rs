mod handler;
mod v2;
mod v4;
mod v5;

use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use drop_auth::Nonce;
use futures::{SinkExt, StreamExt};
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
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use super::{events::FileEventTx, IncomingFileEventTx};
use crate::{
    auth,
    file::{self, FileToRecv},
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
    Error, Event, File, FileId,
};

const MAX_FILENAME_LENGTH: usize = 255;
const MAX_FILE_SUFFIX_LEN: usize = 5; // Assume that the suffix will fit into 5 characters e.g.
                                      // `<filename>(999).<ext>`
const REPORT_PROGRESS_THRESHOLD: u64 = 1024 * 64;

pub enum ServerReq {
    Download { task: Box<FileXferTask> },
    Reject { file: FileId },
    Done { file: FileId },
    Fail { file: FileId },
    Close,
}

pub struct FileXferTask {
    pub file: FileToRecv,
    pub xfer: Arc<IncomingTransfer>,
    pub base_dir: Hidden<PathBuf>,
}

struct TmpFileState {
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

pub(crate) fn spawn(
    addr: IpAddr,
    state: Arc<State>,
    auth: Arc<auth::Context>,
    logger: Logger,
    stop: CancellationToken,
    alive: AliveGuard,
) -> crate::Result<()> {
    let nonce_store = Arc::new(Mutex::new(HashMap::new()));

    #[derive(Debug)]
    struct MissingAuth(SocketAddr);
    impl warp::reject::Reject for MissingAuth {}

    #[derive(Debug)]
    struct Unauthrorized;
    impl warp::reject::Reject for Unauthrorized {}

    #[derive(Debug)]
    struct ToManyReqs;
    impl warp::reject::Reject for ToManyReqs {}

    async fn handle_rejection(
        nonces: &Mutex<HashMap<SocketAddr, Nonce>>,
        err: warp::Rejection,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        if let Some(MissingAuth(peer)) = err.find() {
            let nonce = Nonce::generate();
            let value = drop_auth::http::WWWAuthenticate::new(nonce);

            nonces.lock().await.insert(*peer, nonce);

            Ok(Box::new(warp::reply::with_header(
                StatusCode::UNAUTHORIZED,
                drop_auth::http::WWWAuthenticate::KEY,
                value.to_string(),
            )))
        } else if let Some(Unauthrorized) = err.find() {
            Ok(Box::new(StatusCode::UNAUTHORIZED))
        } else if let Some(ToManyReqs) = err.find() {
            Ok(Box::new(StatusCode::TOO_MANY_REQUESTS))
        } else {
            Err(err)
        }
    }

    let service = {
        let logger = logger.clone();
        let nonces = nonce_store.clone();
        let alive = alive.clone();
        let stop = stop.clone();

        let rate_limiter = Arc::new(governor::RateLimiter::dashmap(governor::Quota::per_second(
            drop_config::MAX_REQUESTS_PER_SEC
                .try_into()
                .map_err(|_| crate::Error::InvalidArgument)?,
        )));

        warp::path("drop")
            .and(warp::path::param().and_then(|version: String| async move {
                version
                    .parse::<protocol::Version>()
                    .map_err(|_| warp::reject::not_found())
            }))
            .and(
                warp::filters::addr::remote().and_then(move |peer: Option<SocketAddr>| {
                    let peer = peer.expect("Transport should use IP addresses");
                    let check = rate_limiter.check_key(&peer.ip());

                    async move {
                        match check {
                            Ok(_) => Ok(peer),
                            Err(_) => Err(warp::reject::custom(ToManyReqs)),
                        }
                    }
                }),
            )
            .and(warp::filters::header::optional("authorization"))
            .and_then(
                move |version: protocol::Version, peer: SocketAddr, auth_header: Option<String>| {
                    let nonces = nonces.clone();
                    let auth = auth.clone();

                    async move {
                        // Uncache the peer nonce first
                        let nonce = nonces.lock().await.remove(&peer);

                        match version {
                            protocol::Version::V1 | protocol::Version::V2 => (),
                            _ => {
                                let auth_header = auth_header
                                    .ok_or_else(|| warp::reject::custom(MissingAuth(peer)))?;

                                let nonce =
                                    nonce.ok_or_else(|| warp::reject::custom(Unauthrorized))?;

                                if !auth.authorize(peer.ip(), &auth_header, &nonce) {
                                    return Err(warp::reject::custom(Unauthrorized));
                                }
                            }
                        };

                        Ok((version, peer))
                    }
                },
            )
            .untuple_one()
            .and(warp::ws())
            .map(
                move |version: protocol::Version, peer: SocketAddr, ws: warp::ws::Ws| {
                    let state = Arc::clone(&state);
                    let alive = alive.clone();
                    let stop = stop.clone();
                    let logger = logger.clone();

                    ws.on_upgrade(move |socket| async move {
                        info!(logger, "Client requested protocol version: {}", version);

                        let ctx = RunContext {
                            logger: &logger,
                            state: state.clone(),
                            socket,
                            stop: &stop,
                        };

                        match version {
                            protocol::Version::V1 => {
                                ctx.run(v2::HandlerInit::<false>::new(
                                    peer.ip(),
                                    &state,
                                    &logger,
                                    &alive,
                                ))
                                .await
                            }
                            protocol::Version::V2 => {
                                ctx.run(v2::HandlerInit::<true>::new(
                                    peer.ip(),
                                    &state,
                                    &logger,
                                    &alive,
                                ))
                                .await
                            }
                            protocol::Version::V4 => {
                                ctx.run(v4::HandlerInit::new(peer.ip(), state, &logger, &alive))
                                    .await
                            }
                            protocol::Version::V5 => {
                                ctx.run(v5::HandlerInit::new(peer.ip(), state, &logger, &alive))
                                    .await
                            }
                        }
                    })
                },
            )
            .recover(move |err| {
                let nonces = Arc::clone(&nonce_store);
                async move { handle_rejection(&nonces, err).await }
            })
    };

    let future = match warp::serve(service)
        .try_bind_with_graceful_shutdown((addr, drop_config::PORT), stop.cancelled_owned())
    {
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
                        "Found that the address {}:{} is already used, while trying to bind the \
                         WS server: {}",
                        addr,
                        drop_config::PORT,
                        ioerr
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

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: Arc<State>,
    socket: WebSocket,
    stop: &'a CancellationToken,
}

impl RunContext<'_> {
    async fn run(mut self, mut handler: impl HandlerInit) {
        let recv_task = handler.recv_req(&mut self.socket);

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
                if let Err(err) = handler.on_error(&mut self.socket, err).await {
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
            handle_client(&self.state, self.logger, self.socket, handler, xfer).await;

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
}

async fn handle_client(
    state: &Arc<State>,
    logger: &slog::Logger,
    mut socket: WebSocket,
    mut handler: impl handler::HandlerInit,
    xfer: Arc<IncomingTransfer>,
) {
    let (req_send, mut req_rx) = mpsc::unbounded_channel();

    if let Err(err) = init_client_handler(state, &xfer, req_send).await {
        error!(logger, "Failed to init trasfer: {err:?}");
        let _ = handler.on_error(&mut socket, err).await;
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

    let mut last_recv = Instant::now();

    let task = async {
        loop {
            tokio::select! {
                biased;

                // API request
                req = req_rx.recv() => {
                    if on_req(&mut socket, &mut jobs, &mut handler, logger, req).await?.is_break() {
                        break;
                    }
                },
                // Message received
                recv = super::utils::recv(&mut socket, handler.recv_timeout(last_recv.elapsed())) => {
                    let msg =  recv?.context("Failed to receive WS message")?;
                    last_recv = Instant::now();

                    if on_recv(&mut socket, &mut handler, msg, logger).await?.is_break() {
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

    if let Err(err) = result {
        info!(logger, "WS connection broke for {}: {err:?}", xfer.id());
    } else {
        let drain_sock = async {
            let task = async {
                // Drain messages
                while socket.next().await.is_some() {}
                socket.close().await
            };

            if let Err(err) = task.await {
                warn!(
                    logger,
                    "Failed to gracefully close the client connection: {}", err
                );
            } else {
                debug!(logger, "WS client disconnected");
            }
        };

        let remove_xfer = async {
            if let Err(err) = state.transfer_manager.incoming_remove(xfer.id()).await {
                warn!(
                    logger,
                    "Failed to clear sync state for {}: {err}",
                    xfer.id()
                );
            }
        };

        tokio::join!(handler.finalize_success(), drain_sock, remove_xfer);
    }

    jobs.shutdown().await;
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

            if bytes_received > self.file.size() {
                return Err(crate::Error::UnexpectedData);
            }

            downloader.validate(tmp_loc).await?;
            Ok(bytes_received)
        };

        let bytes_received = match consume_file_chunks.await {
            Ok(br) => br,
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
        };

        let dst = match self.place_file_into_dest(state, logger, tmp_loc).await {
            Ok(dst) => dst,
            Err(err) => {
                error!(
                    logger,
                    "Could not rename temporary file of {} after downloading: {err}",
                    self.file.id(),
                );
                return Err(err);
            }
        };

        downloader.done(bytes_received).await?;

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

    async fn run(
        mut self,
        state: Arc<State>,
        events: Arc<FileEventTx<IncomingTransfer>>,
        mut downloader: impl Downloader,
        mut stream: UnboundedReceiver<Vec<u8>>,
        logger: Logger,
    ) {
        let init_res = match downloader.init(&self).await {
            Ok(init) => init,
            Err(crate::Error::Canceled) => {
                warn!(logger, "File cancelled on download init stage");
                return;
            }
            Err(err) => {
                events.failed(err).await;
                return;
            }
        };

        match init_res {
            handler::DownloadInit::Stream {
                offset,
                tmp_location,
            } => {
                let result = self
                    .stream_file(
                        StreamCtx {
                            logger: &logger,
                            state: &state,
                            tmp_loc: &tmp_location,
                            stream: &mut stream,
                            events: &events,
                        },
                        &mut downloader,
                        offset,
                    )
                    .await;

                match result {
                    Err(crate::Error::Canceled) => {
                        if let Err(err) = state
                            .transfer_manager
                            .incoming_download_cancel(self.xfer.id(), self.file.id())
                            .await
                        {
                            warn!(logger, "Failed to store download finish: {err}");
                        }
                    }
                    Ok(dst_location) => {
                        if let Err(err) = state
                            .transfer_manager
                            .incoming_finish_post(self.xfer.id(), self.file.id(), true)
                            .await
                        {
                            warn!(logger, "Failed to post finish: {err}");
                        }

                        events.success(dst_location).await;
                    }
                    Err(err) => {
                        if let Err(err) = state
                            .transfer_manager
                            .incoming_finish_post(self.xfer.id(), self.file.id(), false)
                            .await
                        {
                            warn!(logger, "Failed to post finish: {err}");
                        }

                        let _ = downloader.error(err.to_string()).await;
                        events.failed(err).await;
                    }
                }
            }
        }
    }
}

impl TmpFileState {
    // Blocking operation
    async fn load(path: &Path) -> io::Result<Self> {
        let file = fs::File::open(path)?;

        let meta = file.metadata()?;
        let csum = file::checksum(&mut io::BufReader::new(file)).await?;
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

async fn init_client_handler(
    state: &State,
    xfer: &Arc<IncomingTransfer>,
    req_send: mpsc::UnboundedSender<ServerReq>,
) -> anyhow::Result<()> {
    let is_new = state
        .transfer_manager
        .register_incoming(xfer.clone(), req_send)
        .await?;

    if is_new {
        state
            .event_tx
            .send(Event::RequestReceived(xfer.clone()))
            .await
            .expect("Failed to notify receiving peer!");
    }

    Ok(())
}

async fn on_req(
    socket: &mut WebSocket,
    jobs: &mut JoinSet<()>,
    handler: &mut impl HandlerLoop,
    logger: &Logger,
    req: Option<ServerReq>,
) -> anyhow::Result<ControlFlow<()>> {
    match req.context("API channel broken")? {
        ServerReq::Download { task } => handler.issue_download(socket, jobs, *task).await?,
        ServerReq::Reject { file } => handler.issue_reject(socket, file.clone()).await?,
        ServerReq::Done { file } => handler.issue_done(socket, file.clone()).await?,
        ServerReq::Fail { file } => handler.issue_failure(socket, file.clone()).await?,

        ServerReq::Close => {
            debug!(logger, "Stoppping server connection gracefuly");
            socket.send(Message::close()).await?;
            handler.on_close(false).await;
            return Ok(ControlFlow::Break(()));
        }
    }

    Ok(ControlFlow::Continue(()))
}

async fn on_recv(
    socket: &mut WebSocket,
    handler: &mut impl HandlerLoop,
    msg: Message,
    logger: &slog::Logger,
) -> anyhow::Result<ControlFlow<()>> {
    if let Ok(text) = msg.to_str() {
        debug!(logger, "Received:\n\t{text}");
        handler.on_text_msg(socket, text).await?;
    } else if msg.is_binary() {
        handler.on_bin_msg(socket, msg.into_bytes()).await?;
    } else if msg.is_close() {
        debug!(logger, "Got CLOSE frame");
        handler.on_close(true).await;

        return Ok(ControlFlow::Break(()));
    } else if msg.is_ping() {
        debug!(logger, "PING");
    } else if msg.is_pong() {
        debug!(logger, "PONG");
    } else {
        warn!(logger, "Server received invalid WS message type");
    }

    anyhow::Ok(ControlFlow::Continue(()))
}

async fn start_download(
    jobs: &mut JoinSet<()>,
    guard: AliveGuard,
    state: Arc<State>,
    job: FileXferTask,
    downloader: impl Downloader + Send + 'static,
    stream: UnboundedReceiver<Vec<u8>>,
    logger: Logger,
) -> anyhow::Result<(AbortHandle, Arc<IncomingFileEventTx>)> {
    let events = state
        .transfer_manager
        .incoming_file_events(job.xfer.id(), job.file.id())
        .await?;

    events.start(job.base_dir.to_string_lossy()).await;

    let job = {
        let events = events.clone();

        jobs.spawn(async move {
            let _guard = guard;
            job.run(state, events, downloader, stream, logger).await;
        })
    };

    Ok((job, events))
}
