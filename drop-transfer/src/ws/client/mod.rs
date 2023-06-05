mod handler;
mod v2;
mod v4;

use std::{
    io,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use hyper::{http::HeaderValue, StatusCode};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    net::TcpStream,
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
};
use tokio_tungstenite::{
    tungstenite::{self, client::IntoClientRequest, protocol::Role, Message},
    WebSocketStream,
};

use self::handler::{HandlerInit, HandlerLoop, Uploader};
use super::events::FileEventTx;
use crate::{
    auth,
    error::ResultExt,
    file::FileId,
    manager::{TransferConnection, TransferGuard},
    protocol,
    service::State,
    ws::Pinger,
    Event,
};

pub type WebSocket = WebSocketStream<TcpStream>;

pub enum ClientReq {
    Cancel { file: FileId },
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: Arc<State>,
    socket: WebSocket,
    xfer: crate::Transfer,
}

pub(crate) async fn run(state: Arc<State>, xfer: crate::Transfer, logger: Logger) {
    let (socket, ver) = match establish_ws_conn(&state, xfer.peer(), &logger).await {
        Ok(res) => res,
        Err(err) => {
            error!(logger, "Could not connect to peer {}: {}", xfer.id(), err);

            state
                .event_tx
                .send(Event::TransferFailed(xfer, err))
                .await
                .expect("Failed to send TransferFailed event");

            return;
        }
    };

    info!(logger, "Client connected, using version: {ver}");

    let ctx = RunContext {
        logger: &logger,
        state: state.clone(),
        socket,
        xfer,
    };

    match ver {
        protocol::Version::V1 => {
            ctx.run(v2::HandlerInit::<false>::new(&state, &logger))
                .await
        }
        protocol::Version::V2 => ctx.run(v2::HandlerInit::<true>::new(&state, &logger)).await,
        protocol::Version::V4 => ctx.run(v4::HandlerInit::new(state, &logger)).await,
    }
}

async fn establish_ws_conn(
    state: &State,
    ip: IpAddr,
    logger: &Logger,
) -> crate::Result<(WebSocket, protocol::Version)> {
    let mut socket = tokio::time::timeout(
        state.config.req_connection_timeout,
        tcp_connect(state, ip, logger),
    )
    .await
    .map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))?;

    let mut versions_to_try = [
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
    let url = format!("ws://{ip}:{}/drop/{version}", drop_config::PORT);

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
    ) -> crate::Result<UnboundedReceiver<ClientReq>> {
        handler.start(&mut self.socket, &self.xfer).await?;

        let (tx, rx) = mpsc::unbounded_channel();

        let mut lock = self.state.transfer_manager.lock().await;
        lock.insert_transfer(self.xfer.clone(), TransferConnection::Client(tx))?;

        self.state
            .event_tx
            .send(Event::RequestQueued(self.xfer.clone()))
            .await
            .expect("Could not send a RequestQueued event, channel closed");

        Ok(rx)
    }

    async fn run(mut self, mut handler: impl HandlerInit) {
        let _guard = TransferGuard::new(self.state.clone(), self.xfer.id());

        let mut api_req_rx = match self.start(&mut handler).await {
            Ok(rx) => rx,
            Err(err) => {
                error!(
                    self.logger,
                    "Could not send transfer {}: {}",
                    self.xfer.id(),
                    err
                );

                self.state
                    .event_tx
                    .send(Event::TransferFailed(self.xfer.clone(), err))
                    .await
                    .expect("Failed to send TransferFailed event");

                return;
            }
        };

        let (upload_tx, mut upload_rx) = mpsc::channel(2);
        let mut ping = handler.pinger();
        let mut handler = handler.upgrade(upload_tx, self.xfer);

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
        } else {
            let task = async {
                // Drain messages
                while self.socket.next().await.transpose()?.is_some() {}
                anyhow::Ok(())
            };

            if let Err(err) = task.await {
                warn!(
                    self.logger,
                    "Failed to gracefully close the client connection: {err}"
                );
            } else {
                debug!(self.logger, "WS client disconnected");
            }
        }
    }
}

async fn start_upload(
    state: Arc<State>,
    logger: slog::Logger,
    events: Arc<FileEventTx>,
    mut uploader: impl Uploader,
    xfer: crate::Transfer,
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
            xfile.info(),
        );

        let send_file = async {
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
            xfile.info(),
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
