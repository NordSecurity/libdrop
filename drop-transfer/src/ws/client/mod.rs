mod handler;
mod socket;
mod v2;
mod v4;
mod v6;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use drop_analytics::{TransferStateEventData, MOOSE_STATUS_SUCCESS};
use hyper::{Request, Response, StatusCode};
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

use self::{
    handler::{HandlerInit, HandlerLoop, Uploader},
    socket::{WebSocket, WsStream},
};
use super::OutgoingFileEventTx;
use crate::{
    auth,
    file::FileId,
    manager::FileTerminalState,
    protocol,
    service::State,
    tasks::AliveGuard,
    transfer::Transfer,
    utils,
    ws::{client::handler::MsgToSend, Pinger},
    Event, OutgoingTransfer,
};

pub enum ClientReq {
    Reject { file: FileId },
    Fail { file: FileId },
    Close,
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: &'a Arc<State>,
    xfer: &'a Arc<OutgoingTransfer>,
}

enum WsConnection {
    Recoverable,
    Unrecoverable(crate::Error),
    Connected(WsStream, protocol::Version),
}

#[derive(thiserror::Error, Debug)]
enum RequestError {
    #[error("{0}")]
    General(#[from] anyhow::Error),
    #[error("Unexpected HTTP response: {0}")]
    UnexpectedResponse(StatusCode),
}

pub(crate) fn spawn(
    mut refresh_trigger: tokio::sync::watch::Receiver<()>,
    state: Arc<State>,
    xfer: Arc<OutgoingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();

    tokio::spawn(async move {
        let task = async {
            loop {
                let cf = run(state.clone(), xfer.clone(), &logger, &guard).await;
                if cf.is_break() {
                    debug!(logger, "connection status is irrecoverable");
                    if let Err(err) = state.transfer_manager.outgoing_remove(xfer.id()).await {
                        warn!(
                            logger,
                            "Failed to clear sync state for {}: {err}",
                            xfer.id()
                        );
                    }

                    break;
                }

                let _ = refresh_trigger.changed().await;
            }
        };

        tokio::select! {
            biased;

            _ = stop.cancelled() => {
                debug!(logger, "stop client job for: {}", id);
            },
            _ = task => ()
        }
    });
}

async fn run(
    state: Arc<State>,
    xfer: Arc<OutgoingTransfer>,
    logger: &Logger,
    alive: &AliveGuard,
) -> ControlFlow<()> {
    for delay in std::iter::once(Duration::from_secs(0)).chain(drop_config::RETRY_INTERVALS) {
        debug!(
            logger,
            "Outcoming transfer job started for {}, will sleep for {:?}",
            xfer.id(),
            delay
        );

        tokio::time::sleep(delay).await;
        connect_to_peer(&state, &xfer, logger, alive).await?;
    }

    debug!(logger, "no more retries left, considering failure");
    ControlFlow::Continue(())
}

async fn connect_to_peer(
    state: &Arc<State>,
    xfer: &Arc<OutgoingTransfer>,
    logger: &Logger,
    alive: &AliveGuard,
) -> ControlFlow<()> {
    let (socket, ver) = match establish_ws_conn(state, xfer, logger).await {
        WsConnection::Connected(sock, ver) => (sock, ver),
        WsConnection::Recoverable => return ControlFlow::Continue(()),
        WsConnection::Unrecoverable(err) => {
            error!(logger, "Could not connect to peer {}: {}", xfer.id(), err);

            state.moose.event_transfer_state(TransferStateEventData {
                transfer_id: xfer.id().to_string(),
                result: i32::from(&err),
                protocol_version: 0,
            });

            state.emit_event(Event::OutgoingTransferFailed(xfer.clone(), err, false));

            return ControlFlow::Break(());
        }
    };

    state.moose.event_transfer_state(TransferStateEventData {
        protocol_version: ver.into(),
        transfer_id: xfer.id().to_string(),
        result: MOOSE_STATUS_SUCCESS,
    });

    info!(logger, "Client connected, using version: {ver}");

    let ctx = RunContext {
        logger,
        state,
        xfer,
    };

    use protocol::Version;
    let control = match ver {
        Version::V1 => {
            ctx.run(socket, v2::HandlerInit::<false>::new(state, logger, alive))
                .await
        }
        Version::V2 => {
            ctx.run(socket, v2::HandlerInit::<true>::new(state, logger, alive))
                .await
        }
        Version::V4 => {
            ctx.run(socket, v4::HandlerInit::new(state, logger, alive))
                .await
        }
        Version::V5 => {
            ctx.run(socket, v6::HandlerInit::new(state, logger, alive))
                .await
        }
        Version::V6 => {
            ctx.run(socket, v6::HandlerInit::new(state, logger, alive))
                .await
        }
    };

    if let Err(e) = state.transfer_manager.outgoing_disconnect(xfer.id()).await {
        warn!(logger, "Transfer manager outoing_disconnect() failed: {e}");
    }
    control
}

async fn establish_ws_conn(
    state: &State,
    xfer: &OutgoingTransfer,
    logger: &Logger,
) -> WsConnection {
    let remote = SocketAddr::new(xfer.peer(), drop_config::PORT);
    let local = SocketAddr::new(state.addr, 0);

    let mut socket = match utils::connect(local, remote).await {
        Ok(sock) => sock,
        Err(err) => {
            debug!(logger, "Failed to connect: {:?}", err,);
            return WsConnection::Recoverable;
        }
    };

    let mut versions_to_try = [
        protocol::Version::V6,
        protocol::Version::V5,
        protocol::Version::V4,
        protocol::Version::V2,
        protocol::Version::V1,
    ]
    .into_iter();

    let ver = loop {
        let ver = if let Some(ver) = versions_to_try.next() {
            ver
        } else {
            return WsConnection::Unrecoverable(crate::Error::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "Server did not respond for any of known protocol versions",
            )));
        };

        match make_request(&mut socket, xfer.peer(), ver, state.auth.as_ref(), logger).await {
            Ok(_) => break ver,
            Err(RequestError::General(err)) => {
                info!(logger, "Error while making the HTTP request: {err:?}");
                return WsConnection::Recoverable;
            }
            Err(RequestError::UnexpectedResponse(status)) => {
                match status {
                    StatusCode::UNAUTHORIZED => {
                        return WsConnection::Unrecoverable(crate::Error::AuthenticationFailed)
                    }
                    StatusCode::TOO_MANY_REQUESTS => {
                        warn!(logger, "The response triggered DoS protection mechanism");
                        return WsConnection::Recoverable;
                    }
                    StatusCode::NOT_FOUND => (), // Server doesn't support
                    status => debug!(
                        logger,
                        "Failed to connect to version {ver}, status: {status}"
                    ),
                }
            }
        }
    };

    let client = WebSocketStream::from_raw_socket(socket, Role::Client, None).await;
    WsConnection::Connected(client, ver)
}

async fn make_request(
    socket: &mut TcpStream,
    ip: IpAddr,
    version: protocol::Version,
    auth: &auth::Context,
    logger: &slog::Logger,
) -> Result<(), RequestError> {
    let addr = SocketAddr::new(ip, drop_config::PORT);

    let url = format!("ws://{addr}/drop/{version}",);

    debug!(logger, "Making HTTP request: {url}");

    let mut req = url.as_str().into_client_request().context("Invalid URL")?;

    use protocol::Version as Ver;
    let server_auth_scheme = match version {
        Ver::V1 | Ver::V2 | Ver::V4 | Ver::V5 => None,
        _ => {
            let nonce = drop_auth::Nonce::generate_as_client();

            let (key, value) = auth::create_www_authentication_header(&nonce);
            req.headers_mut().insert(key, value);

            Some(nonce)
        }
    };

    let resp = send_request_and_wait_for_respnse(socket, req).await?;

    let authorize = || {
        if let Some(nonce) = &server_auth_scheme {
            // Validate the server response
            authorize_server(auth, &resp, ip, nonce)
                .context("Failed to authorize server. Closing connection")?;
        }
        anyhow::Ok(())
    };

    match resp.status() {
        status if status.is_success() || status.is_informational() => {
            authorize()?;

            debug!(logger, "Connected to {url} without authorization");
            Ok(())
        }
        StatusCode::UNAUTHORIZED => {
            authorize()?;

            debug!(logger, "Creating 'authorization' header");

            debug!(logger, "Extracting peers ({ip}) public key");
            let (key, value) =
                auth.create_clients_auth_header(&resp, ip, server_auth_scheme.is_some())?;

            debug!(logger, "Building 'authorization' request");
            let mut req = url.as_str().into_client_request().context("Invalid URL")?;
            req.headers_mut().insert(key, value);

            debug!(logger, "Re-sending request with the 'authorization' header");
            let resp = send_request_and_wait_for_respnse(socket, req).await?;

            match resp.status() {
                status if status.is_success() || status.is_informational() => Ok(()),
                status => Err(RequestError::UnexpectedResponse(status)),
            }
        }
        status => Err(RequestError::UnexpectedResponse(status)),
    }
}

async fn send_request_and_wait_for_respnse(
    socket: &mut TcpStream,
    req: Request<()>,
) -> anyhow::Result<Response<Option<Vec<u8>>>> {
    let resp = match tokio_tungstenite::client_async(req, &mut *socket).await {
        Ok((_, resp)) => resp,
        Err(tungstenite::Error::Http(resp)) => resp,
        Err(err) => return Err(err.into()),
    };

    Ok(resp)
}

impl RunContext<'_> {
    async fn start(
        &mut self,
        socket: &mut WebSocket,
        handler: &mut impl HandlerInit,
    ) -> crate::Result<Option<UnboundedReceiver<ClientReq>>> {
        let (tx, rx) = mpsc::unbounded_channel();
        match self
            .state
            .transfer_manager
            .outgoing_connected(self.xfer.id(), tx)
            .await
        {
            Ok(()) => handler.start(socket, self.xfer).await?,
            Err(crate::Error::BadTransfer) => return Ok(None),
            Err(err) => return Err(err),
        }

        Ok(Some(rx))
    }

    async fn run(mut self, socket: WsStream, mut handler: impl HandlerInit) -> ControlFlow<()> {
        let mut socket =
            WebSocket::new(socket, handler.recv_timeout(), drop_config::WS_SEND_TIMEOUT);

        let mut api_req_rx = match self.start(&mut socket, &mut handler).await {
            Ok(Some(rx)) => rx,
            Ok(None) => {
                let task = async {
                    socket.close().await?;
                    socket.drain().await?;
                    anyhow::Ok(())
                };
                if let Err(err) = task.await {
                    // It means that the close() call returned an IO error for some reason. It
                    // shouldn't happen probably, and even if it does, it's probably best to just
                    // ignore it
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
        let mut jobs = JoinSet::new();

        let task = async {
            loop {
                tokio::select! {
                    biased;

                    // API request
                    req = api_req_rx.recv() => {
                        if on_req(&mut socket, &mut handler, self.logger, req).await?.is_break() {
                            break;
                        }
                    },
                    // Message received
                    recv = socket.recv() => {
                        let msg =  recv.context("Failed to receive WS message")?;

                        if on_recv(&mut socket, &mut handler, msg, self.logger, &mut jobs).await.context("Handler on recv")?.is_break() {
                            break;
                        }
                    },
                    // Message to send down the wire
                    msg = upload_rx.recv() => {
                        let MsgToSend { msg } = msg.expect("Handler channel should always be open");
                        socket.send(msg).await.context("Socket sending upload msg")?;
                    },
                    _ = ping.tick() => {
                        socket.send(Message::Ping(Vec::new())).await.context("Failed to send PING")?;
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
                if let Err(err) = socket.drain().await {
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

    events.start(uploader.offset()).await;

    let upload_job = {
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

                    let msg = err.to_string();

                    match state
                        .transfer_manager
                        .outgoing_failure_post(xfer.id(), &file_id)
                        .await
                    {
                        Err(err) => {
                            warn!(logger, "Failed to post failure {err:?}");
                        }
                        Ok(res) => res.events.failed(err).await,
                    }
                    uploader.error(msg).await;
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
    logger: &Logger,
    req: Option<ClientReq>,
) -> anyhow::Result<ControlFlow<()>> {
    match req.context("API channel broken")? {
        ClientReq::Reject { file } => {
            handler.issue_reject(socket, file.clone()).await?;
        }
        ClientReq::Fail { file } => {
            handler.issue_failure(socket, file.clone()).await?;
        }
        ClientReq::Close => {
            debug!(logger, "Stopping client connection gracefuly");
            socket.close().await?;
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

async fn on_upload_finished(
    state: &State,
    xfer: &OutgoingTransfer,
    file_id: &FileId,
    logger: &slog::Logger,
) {
    match state
        .transfer_manager
        .outgoing_terminal_recv(xfer.id(), file_id, FileTerminalState::Completed)
        .await
    {
        Err(err) => warn!(logger, "Failed to accept file as done: {err}"),
        Ok(Some(res)) => res.events.success().await,
        Ok(None) => (),
    }
}

async fn on_upload_failure(
    state: &State,
    xfer: &OutgoingTransfer,
    file_id: &FileId,
    msg: String,
    logger: &slog::Logger,
) {
    match state
        .transfer_manager
        .outgoing_terminal_recv(xfer.id(), file_id, FileTerminalState::Failed)
        .await
    {
        Err(err) => warn!(logger, "Failed to accept failure: {err}"),
        Ok(Some(res)) => {
            res.events
                .failed(crate::Error::BadTransferState(format!(
                    "Receiver reported an error: {msg}"
                )))
                .await;
        }
        Ok(None) => (),
    }
}

fn authorize_server<T>(
    auth: &auth::Context,
    response: &hyper::Response<T>,
    ip: IpAddr,
    nonce: &drop_auth::Nonce,
) -> anyhow::Result<()> {
    let ticket = response
        .headers()
        .get(drop_auth::http::Authorization::KEY)
        .context("Missing 'authorization' header")?
        .to_str()
        .context("Invalid 'authorization' header value")?;

    anyhow::ensure!(
        auth.authorize(ip, ticket, nonce),
        "Cannot authrozie server with ticket provided"
    );
    Ok(())
}
