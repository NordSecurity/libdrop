mod handler;
mod socket;
mod throttle;
mod v6;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
};

use anyhow::Context;
use hyper::{Request, Response, StatusCode};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    net::TcpStream,
    sync::mpsc::{self, UnboundedReceiver},
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
    manager::{FileTerminalState, FinishTransferState, OutgoingConnected},
    protocol,
    service::State,
    tasks::AliveGuard,
    transfer::Transfer,
    utils,
    ws::{client::handler::MsgToSend, Pinger},
    OutgoingTransfer,
};

pub enum ClientReq {
    Reject { file: FileId },
    Fail { file: FileId, msg: String },
    Close,
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: &'a Arc<State>,
    xfer: &'a Arc<OutgoingTransfer>,
}

enum WsConnection {
    Recoverable(crate::Error),
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
    refresh_trigger: tokio::sync::watch::Receiver<()>,
    state: Arc<State>,
    xfer: Arc<OutgoingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();

    tokio::spawn(async move {
        let mut backoff =
            utils::RetryTrigger::new(refresh_trigger, state.config.connection_retries);

        let task = async {
            loop {
                let cf = connect_to_peer(&state, &xfer, &logger, &guard).await;
                if cf.is_break() {
                    debug!(logger, "connection status is irrecoverable");
                    break;
                }

                backoff.backoff().await;
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

async fn connect_to_peer(
    state: &Arc<State>,
    xfer: &Arc<OutgoingTransfer>,
    logger: &Logger,
    alive: &AliveGuard,
) -> ControlFlow<()> {
    debug!(logger, "Outgoing transfer job started for {}", xfer.id(),);

    let (socket, ver) = match establish_ws_conn(state, xfer, logger).await {
        WsConnection::Connected(sock, ver) => (sock, ver),
        WsConnection::Recoverable(error) => {
            info!(logger, "Transfer deferred {}: {error}", xfer.id());

            if let Some(tx) = state.transfer_manager.outgoing_event_tx(xfer.id()).await {
                tx.deferred(error).await;
            }
            return ControlFlow::Continue(());
        }
        WsConnection::Unrecoverable(err) => {
            error!(logger, "Could not connect to peer {}: {}", xfer.id(), err);

            if let Some(state) = state.transfer_manager.outgoing_remove(xfer.id()).await {
                state.xfer_events.failed(err, false).await
            }

            return ControlFlow::Break(());
        }
    };

    if let Some(tx) = state.transfer_manager.outgoing_event_tx(xfer.id()).await {
        tx.connected(ver.into()).await;
    }
    info!(logger, "Client connected, using version: {ver}");

    let ctx = RunContext {
        logger,
        state,
        xfer,
    };

    use protocol::Version;
    let control = match ver {
        Version::V6 => {
            ctx.run(socket, v6::HandlerInit::new(state, logger, alive))
                .await
        }
    };

    // The error indicates the transfer is already finished. That's fine
    let _ = state.transfer_manager.outgoing_disconnect(xfer.id()).await;
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
            return WsConnection::Recoverable(crate::Error::Io(err));
        }
    };

    let mut versions_to_try = [protocol::Version::V6].into_iter();

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
                return WsConnection::Recoverable(crate::Error::ConnectionClosedByPeer);
            }
            Err(RequestError::UnexpectedResponse(status)) => {
                match status {
                    StatusCode::UNAUTHORIZED => {
                        return WsConnection::Unrecoverable(crate::Error::AuthenticationFailed)
                    }
                    StatusCode::TOO_MANY_REQUESTS => {
                        warn!(logger, "The response triggered DoS protection mechanism");
                        return WsConnection::Recoverable(crate::Error::TooManyRequests);
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

    let nonce = drop_auth::Nonce::generate_as_client();

    let (key, value) = auth::create_www_authentication_header(&nonce);
    req.headers_mut().insert(key, value);

    let resp = send_request_and_wait_for_respnse(socket, req).await?;

    let authorize = || {
        // Validate the server response
        auth.authorize_server(&resp, ip, &nonce)
            .context("Failed to authorize server. Closing connection")
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
            let (key, value) = auth.create_clients_auth_header(&resp, ip, true)?;

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
            Ok(OutgoingConnected::Continue) => (),
            Ok(OutgoingConnected::JustCancelled { events }) => events.cancel(false).await,
            Err(crate::Error::BadTransfer) => return Ok(None),
            Err(err) => return Err(err),
        }

        handler.start(socket, self.xfer).await?;

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
                        if self.on_req(&mut socket, &mut handler, req).await?.is_break() {
                            break;
                        }
                    },
                    // Message received
                    recv = socket.recv() => {
                        let msg =  recv.context("Failed to receive WS message")?;

                        if self.on_recv(&mut socket, &mut handler, msg, &mut jobs).await.context("Handler on recv")?.is_break() {
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

    async fn on_recv(
        &mut self,
        socket: &mut WebSocket,
        handler: &mut impl HandlerLoop,
        msg: Message,
        jobs: &mut JoinSet<()>,
    ) -> anyhow::Result<ControlFlow<()>> {
        match msg {
            Message::Text(text) => {
                debug!(self.logger, "Received:\n\t{text}");
                handler.on_text_msg(socket, jobs, text).await?;
            }
            Message::Close(_) => {
                debug!(self.logger, "Got CLOSE frame");
                handler.on_close().await;

                if let Some(state) = self
                    .state
                    .transfer_manager
                    .outgoing_remove(self.xfer.id())
                    .await
                {
                    state.xfer_events.cancel(true).await
                }

                return Ok(ControlFlow::Break(()));
            }
            Message::Ping(_) => {
                debug!(self.logger, "PING");
            }
            Message::Pong(_) => {
                debug!(self.logger, "PONG");
            }
            _ => warn!(self.logger, "Client received invalid WS message type"),
        }

        Ok(ControlFlow::Continue(()))
    }

    async fn on_req(
        &mut self,
        socket: &mut WebSocket,
        handler: &mut impl HandlerLoop,
        req: Option<ClientReq>,
    ) -> anyhow::Result<ControlFlow<()>> {
        match req.context("API channel broken")? {
            ClientReq::Reject { file } => {
                handler.issue_reject(socket, file).await?;
            }
            ClientReq::Fail { file, msg } => {
                handler.issue_failure(socket, file, msg).await?;
            }
            ClientReq::Close => {
                debug!(self.logger, "Stopping client connection gracefuly");
                socket.close().await?;
                handler.on_close().await;

                self.state
                    .transfer_manager
                    .outgoing_remove(self.xfer.id())
                    .await;

                return Ok(ControlFlow::Break(()));
            }
        }

        Ok(ControlFlow::Continue(()))
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

    let offset = uploader.offset();

    let permit = throttle::init(&logger, &state, &events, offset)
        .await
        .context("Failed to acquire upload permit")?;

    let upload_job = async move {
        let _guard = guard;
        let xfile = &xfer.files()[&file_id];

        let send_file = async {
            let _permit = permit.acquire().await.ok_or(crate::Error::Canceled)?;

            let mut iofile = match xfile.open(offset) {
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

                match state
                    .transfer_manager
                    .outgoing_failure_post(xfer.id(), &file_id, err.to_string())
                    .await
                {
                    Err(err) => {
                        warn!(logger, "Failed to post failure {err:?}");
                    }
                    Ok(res) => {
                        res.file_events.failed(err).await;
                        handle_finish_xfer_state(res.xfer_state, false).await;
                    }
                }
            }
        };
    };

    Ok((jobs.spawn(upload_job), events))
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
        Ok(Some(res)) => {
            res.file_events.success().await;
            handle_finish_xfer_state(res.xfer_state, true).await;
        }
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
            res.file_events
                .failed(crate::Error::BadTransferState(format!(
                    "Receiver reported an error: {msg}"
                )))
                .await;
            handle_finish_xfer_state(res.xfer_state, true).await;
        }
        Ok(None) => (),
    }
}

pub async fn handle_finish_xfer_state(state: FinishTransferState<OutgoingTransfer>, by_peer: bool) {
    match state {
        FinishTransferState::Canceled { events } => events.cancel(by_peer).await,
        FinishTransferState::Alive => (),
    }
}
