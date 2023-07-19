mod handler;
mod v2;
mod v4;
mod v5;

use std::{
    fs, io,
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use drop_storage::sync;
use futures::{SinkExt, StreamExt};
use hyper::{http::HeaderValue, StatusCode};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{self, UnboundedReceiver},
        Semaphore, SemaphorePermit, TryAcquireError,
    },
    task::JoinHandle,
};
use tokio_tungstenite::{
    tungstenite::{self, client::IntoClientRequest, protocol::Role, Message},
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;

use self::handler::{HandlerInit, HandlerLoop, Uploader};
use super::events::FileEventTx;
use crate::{
    auth,
    error::ResultExt,
    file::{File, FileId, FileSubPath, FileToSend},
    manager::OutgoingState,
    protocol,
    service::State,
    transfer::Transfer,
    ws::Pinger,
    Event, OutgoingTransfer,
};

pub type WebSocket = WebSocketStream<TcpStream>;

pub enum ClientReq {
    Reject { file: FileId },
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: &'a Arc<State>,
    socket: WebSocket,
    xfer: &'a Arc<OutgoingTransfer>,
}

pub(crate) async fn run(state: Arc<State>, xfer: Arc<OutgoingTransfer>, logger: Logger) {
    loop {
        let cf = connect_to_peer(&state, &xfer, &logger).await;
        if cf.is_break() {
            let _ = state
                .transfer_manager
                .outgoing
                .lock()
                .await
                .remove(&xfer.id());

            if let Err(err) = state.storage.trasnfer_sync_clear(xfer.id()) {
                error!(logger, "Failed to clear transfer sync data: {err}");
            }

            return;
        }
    }
}

pub(crate) async fn resume(state: &Arc<State>, stop: &CancellationToken, logger: &Logger) {
    let transfers = match state.storage.outgoing_transfers_to_resume() {
        Ok(transfers) => transfers,
        Err(err) => {
            error!(
                logger,
                "Failed to restore pedning outgoing transfers form DB: {err}"
            );
            return;
        }
    };

    for transfer in transfers {
        let restore_transfer = || {
            let files = transfer
                .files
                .into_iter()
                .map(|dbfile| {
                    let file_id: FileId = dbfile.file_id.into();
                    let subpath: FileSubPath = dbfile.subpath.into();
                    let uri = dbfile.uri;

                    let file = match uri.scheme() {
                        "file" => file_to_resume_from_path_uri(&uri, subpath, file_id)?,
                        #[cfg(unix)]
                        "content" => file_to_resume_from_content_uri(state, uri, subpath, file_id)?,
                        unknown => anyhow::bail!("Unknon URI schema: {unknown}"),
                    };

                    anyhow::Ok(file)
                })
                .collect::<Result<_, _>>()?;

            let xfer = OutgoingTransfer::new_with_uuid(
                transfer.peer.parse().context("Failed to parse peer IP")?,
                files,
                transfer.uuid,
                &state.config,
            )
            .context("Failed to create transfer")?;

            anyhow::Ok(xfer)
        };

        let xfer = match restore_transfer() {
            Ok(xfer) => {
                let xfer = Arc::new(xfer);
                let _ = state.transfer_manager.outgoing.lock().await.insert(
                    xfer.id(),
                    OutgoingState {
                        xfer: xfer.clone(),
                        conn: None,
                    },
                );

                xfer
            }
            Err(err) => {
                error!(
                    logger,
                    "Failed to restore transfer {}: {err}", transfer.uuid
                );

                continue;
            }
        };

        let logger = logger.clone();
        let stop = stop.clone();
        let task = {
            let logger = logger.clone();
            let state = state.clone();
            async move {
                run(state, xfer, logger).await;
            }
        };

        tokio::spawn(async move {
            tokio::select! {
                biased;

                _ = stop.cancelled() => {
                    warn!(logger, "Aborting transfer resume");
                },
                _ = task => (),
            }
        });
    }
}

async fn connect_to_peer(
    state: &Arc<State>,
    xfer: &Arc<OutgoingTransfer>,
    logger: &Logger,
) -> ControlFlow<()> {
    let (socket, ver) = match establish_ws_conn(state, xfer.peer(), logger).await {
        Ok(res) => res,
        Err(err) => {
            error!(logger, "Could not connect to peer {}: {}", xfer.id(), err);

            state
                .event_tx
                .send(Event::OutgoingTransferFailed(xfer.clone(), err, false))
                .await
                .expect("Failed to send TransferFailed event");

            return ControlFlow::Break(());
        }
    };

    info!(logger, "Client connected, using version: {ver}");

    let ctx = RunContext {
        logger,
        state,
        socket,
        xfer,
    };

    use protocol::Version;
    let control = match ver {
        Version::V1 => ctx.run(v2::HandlerInit::<false>::new(state, logger)).await,
        Version::V2 => ctx.run(v2::HandlerInit::<true>::new(state, logger)).await,
        Version::V4 => ctx.run(v4::HandlerInit::new(state, logger)).await,
        Version::V5 => ctx.run(v5::HandlerInit::new(state, logger)).await,
    };

    if let Some(state) = state
        .transfer_manager
        .outgoing
        .lock()
        .await
        .get_mut(&xfer.id())
    {
        let _ = state.conn.take();
    }

    control
}

async fn establish_ws_conn(
    state: &State,
    ip: IpAddr,
    logger: &Logger,
) -> crate::Result<(WebSocket, protocol::Version)> {
    let mut socket = tcp_connect(state, ip, logger).await;

    let mut versions_to_try = [
        protocol::Version::V5,
        protocol::Version::V4,
        protocol::Version::V2,
        protocol::Version::V1,
    ]
    .into_iter();

    let ver = loop {
        let ver = versions_to_try.next().ok_or_else(|| {
            crate::Error::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "Server did not respond for any of known protocol versions",
            ))
        })?;

        match make_request(&mut socket, ip, ver, state.auth.as_ref(), logger).await {
            Ok(_) => break ver,
            Err(tungstenite::Error::Http(resp)) if resp.status().is_client_error() => {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    return Err(crate::Error::AuthenticationFailed);
                } else {
                    debug!(
                        logger,
                        "Failed to connect to version {}, response: {:?}", ver, resp
                    );
                }
            }
            Err(err) => return Err(err.into()),
        }
    };

    let client = WebSocketStream::from_raw_socket(socket, Role::Client, None).await;
    Ok((client, ver))
}

async fn make_request(
    socket: &mut TcpStream,
    ip: IpAddr,
    version: protocol::Version,
    auth: &auth::Context,
    logger: &slog::Logger,
) -> Result<(), tungstenite::Error> {
    let addr = SocketAddr::new(ip, drop_config::PORT);

    let url = format!("ws://{addr}/drop/{version}",);

    debug!(logger, "Making HTTP request: {url}");

    let err = match tokio_tungstenite::client_async(&url, &mut *socket).await {
        Ok(_) => {
            debug!(logger, "Connected to {url} without authorization");
            return Ok(());
        }
        Err(err) => err,
    };

    if let tungstenite::Error::Http(resp) = &err {
        if resp.status() == StatusCode::UNAUTHORIZED {
            debug!(logger, "Creating 'authorization' header");

            let extract_www_auth = || {
                let val = resp
                    .headers()
                    .get(drop_auth::http::WWWAuthenticate::KEY)
                    .context("Missing 'www-authenticate' header")?
                    .to_str()?;

                auth.create_ticket_header_val(ip, val)
            };

            debug!(logger, "Extracting peers ({ip}) public key");
            match extract_www_auth() {
                Ok(auth_header) => {
                    debug!(logger, "Building 'authorization' request");

                    let mut req = url.into_client_request()?;
                    req.headers_mut().insert(
                        drop_auth::http::Authorization::KEY,
                        HeaderValue::from_str(&auth_header.to_string())?,
                    );

                    debug!(logger, "Re-sending request with the 'authorization' header");
                    tokio_tungstenite::client_async(req, &mut *socket).await?;
                    return Ok(());
                }
                Err(err) => warn!(
                    logger,
                    "Failed to extract 'www-authenticate' header: {err:?}"
                ),
            }
        }
    }

    Err(err)
}

async fn tcp_connect(state: &State, ip: IpAddr, logger: &Logger) -> TcpStream {
    let mut sleep_time = Duration::from_millis(200);

    loop {
        match TcpStream::connect((ip, drop_config::PORT)).await {
            Ok(sock) => break sock,
            Err(err) => {
                debug!(
                    logger,
                    "Failed to connect: {:?}, sleeping for {} ms",
                    err,
                    sleep_time.as_millis(),
                );

                tokio::time::sleep(sleep_time).await;

                // Exponential backoff but with upper limit
                sleep_time = state
                    .config
                    .connection_max_retry_interval
                    .min(sleep_time * 2);
            }
        }
    }
}

impl RunContext<'_> {
    async fn start(
        &mut self,
        handler: &mut impl HandlerInit,
    ) -> crate::Result<Option<UnboundedReceiver<ClientReq>>> {
        let state = if let Some(state) = self.state.storage.transfer_sync_state(self.xfer.id())? {
            state
        } else {
            return Ok(None);
        };

        handler.start(&mut self.socket, self.xfer).await?;

        match (state.local_state, state.remote_state) {
            (sync::TransferState::New, _) => self.state.storage.update_transfer_sync_states(
                self.xfer.id(),
                Some(sync::TransferState::Active),
                Some(sync::TransferState::Active),
            )?,
            (_, sync::TransferState::New) => self.state.storage.update_transfer_sync_states(
                self.xfer.id(),
                Some(sync::TransferState::Active),
                None,
            )?,
            _ => (),
        }

        let (tx, rx) = mpsc::unbounded_channel();

        match state.local_state {
            sync::TransferState::Canceled => {
                drop(tx);
            }
            _ => {
                reject_transfer_files(self.state, self.xfer, &tx, self.logger);

                self.state
                    .transfer_manager
                    .outgoing
                    .lock()
                    .await
                    .get_mut(&self.xfer.id())
                    .expect("Missing transfer data")
                    .conn = Some(tx);
            }
        }

        Ok(Some(rx))
    }

    async fn run(mut self, mut handler: impl HandlerInit) -> ControlFlow<()> {
        let mut api_req_rx = match self.start(&mut handler).await {
            Ok(Some(rx)) => rx,
            Ok(None) => return ControlFlow::Break(()),
            Err(err) => {
                error!(
                    self.logger,
                    "Could not send transfer {}: {}",
                    self.xfer.id(),
                    err
                );
                return ControlFlow::Continue(());
            }
        };

        let (upload_tx, mut upload_rx) = mpsc::channel(2);
        let mut ping = handler.pinger();
        let mut handler = handler.upgrade(upload_tx, self.xfer.clone());

        let task = async {
            loop {
                tokio::select! {
                    biased;

                    // API request
                    req = api_req_rx.recv() => {
                        if let Some(req) = req {
                            handler.on_req(&mut self.socket, req).await.context("Handler on API req")?;
                        } else {
                            debug!(self.logger, "Stopping client connection gracefuly");
                            self.socket.close(None).await.context("Failed to close WS")?;
                            handler.on_close(false).await;
                            break;
                        };
                    },
                    // Message received
                    recv = super::utils::recv(&mut self.socket, handler.recv_timeout()) => {
                        match recv? {
                            Some(msg) => {
                                if handler.on_recv(&mut self.socket, msg).await.context("Handler on recv")?.is_break() {
                                    break;
                                }
                            },
                            None => break,
                        }
                    },
                    // Message to send down the wire
                    msg = upload_rx.recv() => {
                        let msg = msg.expect("Handler channel should always be open");
                        self.socket.send(msg).await.context("Socket sending upload msg")?;
                    },
                    _ = ping.tick() => {
                        self.socket.send(Message::Ping(Vec::new())).await.context("Failed to send PING")?;
                    }
                }
            }

            anyhow::Ok(())
        };

        let result = task.await;
        handler.on_stop().await;

        if let Err(err) = result {
            handler.finalize_failure(err).await;
            ControlFlow::Continue(())
        } else {
            if let Err(err) = self.drain_socket().await {
                warn!(
                    self.logger,
                    "Failed to gracefully close the client connection: {err}"
                );
            } else {
                debug!(self.logger, "WS client disconnected");
            }

            ControlFlow::Break(())
        }
    }

    async fn drain_socket(&mut self) -> crate::Result<()> {
        while self.socket.next().await.transpose()?.is_some() {}
        Ok(())
    }
}

async fn start_upload(
    state: Arc<State>,
    logger: slog::Logger,
    events: Arc<FileEventTx>,
    mut uploader: impl Uploader,
    xfer: Arc<OutgoingTransfer>,
    file_id: FileId,
) -> anyhow::Result<JoinHandle<()>> {
    let xfile = xfer
        .files()
        .get(&file_id)
        .context("File not found")?
        .clone();

    events
        .start(Event::FileUploadStarted(xfer.clone(), xfile.id().clone()))
        .await;

    let upload_job = async move {
        let transfer_time = Instant::now();

        state.moose.service_quality_transfer_file(
            Ok(()),
            drop_analytics::Phase::Start,
            xfer.id().to_string(),
            0,
            Some(xfile.info()),
        );

        let send_file = async {
            let _permit = acquire_throttle_permit(&logger, &state.throttle, &file_id)
                .await
                .ok_or(crate::Error::Canceled)?;

            let mut iofile = match xfile.open(uploader.offset()) {
                Ok(f) => f,
                Err(err) => {
                    error!(
                        logger,
                        "Failed at service::download() while opening a file: {}", err
                    );
                    return Err(err);
                }
            };

            loop {
                match iofile.read_chunk()? {
                    Some(chunk) => uploader.chunk(chunk).await?,
                    None => return Ok(()),
                }
            }
        };

        let result = send_file.await;

        state.moose.service_quality_transfer_file(
            result.to_status(),
            drop_analytics::Phase::End,
            xfer.id().to_string(),
            transfer_time.elapsed().as_millis() as i32,
            Some(xfile.info()),
        );

        match result {
            Ok(()) => (),
            Err(crate::Error::Canceled) => (),
            Err(err) => {
                error!(
                    logger,
                    "Failed at service::download() while reading a file: {}", err
                );

                uploader.error(err.to_string()).await;
                events
                    .stop(Event::FileUploadFailed(xfer.clone(), file_id, err))
                    .await;
            }
        };
    };

    Ok(tokio::spawn(upload_job))
}

async fn acquire_throttle_permit<'a>(
    logger: &slog::Logger,
    throttle: &'a Semaphore,
    file_id: &FileId,
) -> Option<SemaphorePermit<'a>> {
    match throttle.try_acquire() {
        Err(TryAcquireError::NoPermits) => info!(logger, "Throttling file: {file_id}"),
        Err(err) => {
            error!(logger, "Throttle semaphore failed: {err}");
            return None;
        }
        Ok(permit) => return Some(permit),
    }

    match throttle.acquire().await {
        Ok(permit) => {
            info!(logger, "Throttle permited file: {file_id}");
            Some(permit)
        }
        Err(err) => {
            error!(logger, "Throttle semaphore failed: {err}");
            None
        }
    }
}

fn file_to_resume_from_path_uri(
    uri: &url::Url,
    subpath: FileSubPath,
    file_id: FileId,
) -> anyhow::Result<FileToSend> {
    let fullpath = uri
        .to_file_path()
        .ok()
        .context("Failed to extract file path")?;

    let meta = fs::symlink_metadata(&fullpath)
        .with_context(|| format!("Failed to load file: {}", file_id))?;

    anyhow::ensure!(!meta.is_dir(), "Invalid file type");

    FileToSend::new_to_send(subpath, fullpath, meta, file_id.clone())
        .with_context(|| format!("Failed to restore file {file_id} from DB"))
}

#[cfg(unix)]
fn file_to_resume_from_content_uri(
    state: &State,
    uri: url::Url,
    subpath: FileSubPath,
    file_id: FileId,
) -> anyhow::Result<FileToSend> {
    let callback = state
        .fdresolv
        .as_ref()
        .context("Encountered content uri but FD resovler callback is missing")?;

    let fd = callback(uri.as_str()).with_context(|| format!("Failed to fetch FD for: {uri}"))?;

    FileToSend::new_from_fd(subpath, uri, fd, file_id.clone())
        .with_context(|| format!("Failed to restore file {file_id} from DB"))
}

fn reject_transfer_files(
    state: &State,
    xfer: &Arc<OutgoingTransfer>,
    req_send: &mpsc::UnboundedSender<ClientReq>,
    logger: &Logger,
) {
    let files = match state.storage.outgoing_files_to_reject(xfer.id()) {
        Ok(files) => files,
        Err(err) => {
            warn!(logger, "Failed to fetch files to reject: {err}");
            return;
        }
    };

    for file_id in files {
        info!(logger, "Rejecting file: {file_id}");

        if xfer.files().get(&file_id).is_some() {
            let _ = req_send.send(ClientReq::Reject {
                file: file_id.into(),
            });
        } else {
            warn!(logger, "Missing file: {file_id}");
        }
    }
}
