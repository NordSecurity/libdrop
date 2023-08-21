mod handler;
mod v2;
mod v4;
mod v5;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use hyper::{http::HeaderValue, StatusCode};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{self, UnboundedReceiver},
        Semaphore, SemaphorePermit, TryAcquireError,
    },
    task::{AbortHandle, JoinSet},
};
use tokio_tungstenite::{
    tungstenite::{self, client::IntoClientRequest, protocol::Role, Message},
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;

use self::handler::{HandlerInit, HandlerLoop, Uploader};
use super::OutgoingFileEventTx;
use crate::{
    auth, file::FileId, protocol, service::State, tasks::AliveGuard, transfer::Transfer,
    ws::Pinger, Event, OutgoingTransfer,
};

pub type WebSocket = WebSocketStream<TcpStream>;

pub enum ClientReq {
    Reject { file: FileId },
    Close,
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: &'a Arc<State>,
    socket: WebSocket,
    xfer: &'a Arc<OutgoingTransfer>,
}

pub(crate) fn spawn(
    state: Arc<State>,
    xfer: Arc<OutgoingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();
    let job = run(state, xfer, logger.clone(), guard.clone());

    tokio::spawn(async move {
        let _guard = guard;

        tokio::select! {
            biased;

            _ = stop.cancelled() => {
                debug!(logger, "Client job stop: {id}");
            },
            _ = job => (),
        }
    });
}

async fn run(state: Arc<State>, xfer: Arc<OutgoingTransfer>, logger: Logger, alive: AliveGuard) {
    loop {
        let cf = connect_to_peer(&state, &xfer, &logger, &alive).await;
        if cf.is_break() {
            if let Err(err) = state.transfer_manager.outgoing_remove(xfer.id()).await {
                warn!(
                    logger,
                    "Failed to clear sync state for {}: {err}",
                    xfer.id()
                );
            }

            return;
        }
    }
}

async fn connect_to_peer(
    state: &Arc<State>,
    xfer: &Arc<OutgoingTransfer>,
    logger: &Logger,
    alive: &AliveGuard,
) -> ControlFlow<()> {
    let (socket, ver) = match establish_ws_conn(state, xfer.peer(), logger).await {
        Ok(Some(res)) => res,
        Ok(None) => return ControlFlow::Continue(()),
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

    state
        .moose
        .service_quality_transfer_batch(xfer.id().to_string(), xfer.info(), ver.into());

    info!(logger, "Client connected, using version: {ver}");

    let ctx = RunContext {
        logger,
        state,
        socket,
        xfer,
    };

    use protocol::Version;
    let control = match ver {
        Version::V1 => {
            ctx.run(v2::HandlerInit::<false>::new(state, logger, alive))
                .await
        }
        Version::V2 => {
            ctx.run(v2::HandlerInit::<true>::new(state, logger, alive))
                .await
        }
        Version::V4 => ctx.run(v4::HandlerInit::new(state, logger, alive)).await,
        Version::V5 => ctx.run(v5::HandlerInit::new(state, logger, alive)).await,
    };

    let _ = state.transfer_manager.outgoing_disconnect(xfer.id()).await;
    control
}

async fn establish_ws_conn(
    state: &State,
    ip: IpAddr,
    logger: &Logger,
) -> crate::Result<Option<(WebSocket, protocol::Version)>> {
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
            Err(err) => {
                info!(logger, "Error while making the HTTP request: {err:?}");
                return Ok(None);
            }
        }
    };

    let client = WebSocketStream::from_raw_socket(socket, Role::Client, None).await;
    Ok(Some((client, ver)))
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
        handler.start(&mut self.socket, self.xfer).await?;

        let (tx, rx) = mpsc::unbounded_channel();
        match self
            .state
            .transfer_manager
            .outgoing_connected(self.xfer.id(), tx)
            .await
        {
            Ok(()) => (),
            Err(crate::Error::BadTransfer) => return Ok(None),
            Err(err) => return Err(err),
        }

        Ok(Some(rx))
    }

    async fn run(mut self, mut handler: impl HandlerInit) -> ControlFlow<()> {
        let mut api_req_rx = match self.start(&mut handler).await {
            Ok(Some(rx)) => rx,
            Ok(None) => {
                let task = async {
                    self.socket
                        .close(None)
                        .await
                        .context("Failed to close WS")?;
                    self.drain_socket()
                        .await
                        .context("Failed to drain socket")?;
                    anyhow::Ok(())
                };
                if let Err(err) = task.await {
                    error!(self.logger, "Failed to close socket on start: {err:?}");
                } else {
                    info!(self.logger, "Socket closed on start");
                }

                return ControlFlow::Break(());
            }
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
        let mut last_recv = Instant::now();
        let mut jobs = JoinSet::new();

        let task = async {
            loop {
                tokio::select! {
                    biased;

                    // API request
                    req = api_req_rx.recv() => {
                        if on_req(&mut self.socket, &mut handler, req, self.logger).await?.is_break() {
                            break;
                        }
                    },
                    // Message received
                    recv = super::utils::recv(&mut self.socket, handler.recv_timeout(last_recv.elapsed())) => {
                        let msg =  recv?.context("Failed to receive WS message")?;
                        last_recv = Instant::now();

                        if on_recv(&mut self.socket, &mut handler, msg, self.logger, &mut jobs).await.context("Handler on recv")?.is_break() {
                            break;
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

        let cf = if let Err(err) = result {
            info!(
                self.logger,
                "WS connection broke for {}: {err:?}",
                self.xfer.id()
            );

            ControlFlow::Continue(())
        } else {
            let drain_sock = async {
                if let Err(err) = self.drain_socket().await {
                    warn!(
                        self.logger,
                        "Failed to gracefully close the client connection: {err}"
                    );
                } else {
                    debug!(self.logger, "WS client disconnected");
                }
            };

            tokio::join!(handler.on_stop(), drain_sock);

            ControlFlow::Break(())
        };

        jobs.shutdown().await;

        cf
    }

    async fn drain_socket(&mut self) -> crate::Result<()> {
        while self.socket.next().await.transpose()?.is_some() {}
        Ok(())
    }
}

async fn start_upload(
    jobs: &mut JoinSet<()>,
    state: Arc<State>,
    guard: AliveGuard,
    logger: slog::Logger,
    mut uploader: impl Uploader,
    xfer: Arc<OutgoingTransfer>,
    file_id: FileId,
) -> anyhow::Result<(AbortHandle, Arc<OutgoingFileEventTx>)> {
    let events = state
        .transfer_manager
        .outgoing_file_events(xfer.id(), &file_id)
        .await?;

    events.start().await;

    let upload_job = {
        let events = events.clone();
        async move {
            let _guard = guard;
            let xfile = &xfer.files()[&file_id];

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

            match send_file.await {
                Ok(()) => (),
                Err(crate::Error::Canceled) => (),
                Err(err) => {
                    error!(
                        logger,
                        "Failed at service::download() while reading a file: {}", err
                    );

                    uploader.error(err.to_string()).await;
                    events.failed(err).await;
                }
            };
        }
    };

    Ok((jobs.spawn(upload_job), events))
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

async fn on_req(
    socket: &mut WebSocket,
    handler: &mut impl HandlerLoop,
    req: Option<ClientReq>,
    logger: &slog::Logger,
) -> anyhow::Result<ControlFlow<()>> {
    match req.context("API channel broken")? {
        ClientReq::Reject { file } => handler.issue_reject(socket, file).await?,
        ClientReq::Close => {
            debug!(logger, "Stopping client connection gracefuly");
            socket.close(None).await.context("Failed to close WS")?;
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
    jobs: &mut JoinSet<()>,
) -> anyhow::Result<ControlFlow<()>> {
    match msg {
        Message::Text(text) => {
            debug!(logger, "Received:\n\t{text}");
            handler.on_text_msg(socket, jobs, text).await?;
        }
        Message::Close(_) => {
            debug!(logger, "Got CLOSE frame");
            handler.on_close(true).await;
            return Ok(ControlFlow::Break(()));
        }
        Message::Ping(_) => {
            debug!(logger, "PING");
        }
        Message::Pong(_) => {
            debug!(logger, "PONG");
        }
        _ => warn!(logger, "Client received invalid WS message type"),
    }

    Ok(ControlFlow::Continue(()))
}
