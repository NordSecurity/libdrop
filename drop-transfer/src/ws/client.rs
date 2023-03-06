use std::{
    collections::{hash_map::Entry, HashMap},
    io,
    net::IpAddr,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    net::TcpStream,
    sync::mpsc::{self, Sender, UnboundedReceiver},
    task::JoinHandle,
};
use tokio_tungstenite::{
    tungstenite::{self, protocol::Role, Message},
    WebSocketStream,
};

use super::events::FileEventTx;
use crate::{
    error::ResultExt,
    manager::{TransferConnection, TransferGuard},
    protocol::{self, v1},
    service::State,
    utils::Hidden,
    Event,
};

pub enum ClientReq {
    Cancel { file: PathBuf },
}

pub(crate) async fn run(state: Arc<State>, xfer: crate::Transfer, logger: Logger) {
    let start_session_v1 = async {
        let mut socket = tokio::time::timeout(
            state.config.req_connection_timeout,
            tcp_connect(&state, xfer.peer(), &logger),
        )
        .await
        .map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))?;

        let mut versions_to_try = [protocol::Version::V2, protocol::Version::V1].into_iter();

        let ver = loop {
            let ver = versions_to_try.next().ok_or_else(|| {
                crate::Error::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Server did not respond for any of known protocol versions",
                ))
            })?;

            let url = format!("ws://{}:{}/drop/{ver}", xfer.peer(), drop_config::PORT);

            match tokio_tungstenite::client_async(url, &mut socket).await {
                Ok(_) => break ver,
                Err(tungstenite::Error::Http(resp)) if resp.status().is_client_error() => {
                    debug!(
                        logger,
                        "Failed to connect to version {}, response: {:?}", ver, resp
                    );
                }
                Err(err) => return Err(err.into()),
            }
        };

        info!(logger, "Client connected, using version: {}", ver);
        let mut client = WebSocketStream::from_raw_socket(socket, Role::Client, None).await;

        let req = v1::TransferRequest::try_from(&xfer)?;

        client.send(Message::from(&req)).await?;

        let (tx, rx) = mpsc::unbounded_channel();

        let mut lock = state.transfer_manager.lock().await;
        lock.insert_transfer(xfer.clone(), TransferConnection::Client(tx))
            .map_err(|_| crate::Error::BadTransfer)?;

        state
            .event_tx
            .send(Event::RequestQueued(xfer.clone()))
            .await
            .expect("Could not send a Request Queued event, channel closed");

        crate::Result::Ok((client, rx, ver))
    };

    let (client, rx, ver) = match start_session_v1.await {
        Ok(client) => client,
        Err(err) => {
            error!(logger, "Could not send transfer {}: {}", xfer.id(), err);

            state
                .event_tx
                .send(Event::TransferFailed(xfer, err))
                .await
                .expect("Failed to send TransferFailed event");

            return;
        }
    };

    match ver {
        protocol::Version::V1 => client_task_v1(client, state, xfer, rx, logger).await,
        protocol::Version::V2 => client_task_v2(client, state, xfer, rx, logger).await,
    }
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

async fn client_task_v1(
    socket: WebSocketStream<TcpStream>,
    state: Arc<State>,
    xfer: crate::Transfer,
    api_req_rx: UnboundedReceiver<ClientReq>,
    logger: Logger,
) {
    client_task_v1_v2::<false>(socket, state, xfer, api_req_rx, logger).await
}

async fn client_task_v2(
    socket: WebSocketStream<TcpStream>,
    state: Arc<State>,
    xfer: crate::Transfer,
    api_req_rx: UnboundedReceiver<ClientReq>,
    logger: Logger,
) {
    client_task_v1_v2::<true>(socket, state, xfer, api_req_rx, logger).await
}

async fn client_task_v1_v2<const PING: bool>(
    mut socket: WebSocketStream<TcpStream>,
    state: Arc<State>,
    xfer: crate::Transfer,
    mut api_req_rx: UnboundedReceiver<ClientReq>,
    logger: Logger,
) {
    let _guard = TransferGuard::new(state.clone(), xfer.id());

    let (upload_tx, mut upload_rx) = mpsc::channel(2);
    let mut ping = super::utils::Pinger::<PING>::new(&state);
    let mut handler = ClientHandler::new(state, upload_tx, xfer, logger);

    let task = async {
        loop {
            tokio::select! {
                biased;

                // API request
                req = api_req_rx.recv() => {
                    if let Some(req) = req {
                        handler.on_req(&mut socket, req).await.context("Handler on API req")?;
                    } else {
                        handler.issue_close(&mut socket).await.context("Handler issuing close")?;
                        break;
                    };
                },
                // Message received
                recv = super::utils::recv(&mut socket, handler.timeout::<PING>()) => {
                    match recv? {
                        Some(msg) => {
                            if handler.on_recv(msg).await.context("Handler on recv")?.is_break() {
                                break;
                            }
                        },
                        None => break,
                    }
                },
                // Message to send down the wire
                msg = upload_rx.recv() => {
                    let msg = msg.expect("Handler channel should always be open");
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
    handler.stop_jobs().await;

    if let Err(err) = result {
        handler.on_finalize_failure(err).await;
    } else {
        handler.on_finalize_success(socket).await;
    }
}

type WsSink = WebSocketStream<TcpStream>;

struct FileTask {
    job: JoinHandle<()>,
    events: Arc<FileEventTx>,
}

impl FileTask {
    fn new(
        state: Arc<State>,
        sink: Sender<Message>,
        xfer: crate::Transfer,
        file: String,
        logger: Logger,
    ) -> anyhow::Result<Self> {
        let events = Arc::new(FileEventTx::new(&state));
        let job = start_upload(state, Arc::clone(&events), sink, xfer, file, logger)?;

        Ok(Self { job, events })
    }
}

struct ClientHandler {
    state: Arc<State>,
    upload_tx: Sender<Message>,
    xfer: crate::Transfer,
    tasks: HashMap<PathBuf, FileTask>,
    logger: Logger,
    last_recv: Instant,
}

impl ClientHandler {
    fn new(
        state: Arc<State>,
        upload_tx: Sender<Message>,
        xfer: crate::Transfer,
        logger: Logger,
    ) -> Self {
        Self {
            state,
            upload_tx,
            xfer,
            tasks: HashMap::new(),
            logger,
            last_recv: Instant::now(),
        }
    }

    async fn issue_close(&mut self, socket: &mut WsSink) -> anyhow::Result<()> {
        debug!(self.logger, "Stoppping client connection gracefuly");

        socket.close(None).await?;
        self.on_close(false).await;
        Ok(())
    }

    async fn on_req(&mut self, socket: &mut WsSink, msg: ClientReq) -> anyhow::Result<()> {
        match msg {
            ClientReq::Cancel { file } => self.issue_cancel(socket, file.as_ref()).await,
        }
    }

    fn timeout<const PING: bool>(&self) -> Option<Duration> {
        if PING {
            Some(
                self.state
                    .config
                    .transfer_idle_lifetime
                    .saturating_sub(self.last_recv.elapsed()),
            )
        } else {
            None
        }
    }

    async fn on_recv(&mut self, msg: Message) -> anyhow::Result<ControlFlow<()>> {
        self.last_recv = Instant::now();

        match msg {
            Message::Text(json) => {
                let msg: v1::ServerMsg =
                    serde_json::from_str(&json).context("Failed to deserialize server message")?;

                match msg {
                    v1::ServerMsg::Progress(v1::Progress {
                        file,
                        bytes_transfered,
                    }) => self.on_progress(file.into(), bytes_transfered).await,
                    v1::ServerMsg::Done(v1::Progress {
                        file,
                        bytes_transfered: _,
                    }) => self.on_done(file.into()).await,
                    v1::ServerMsg::Error(v1::Error { file, msg }) => {
                        self.on_error(file.as_deref().map(AsRef::as_ref), msg).await
                    }
                    v1::ServerMsg::Start(v1::Download { file }) => self.on_download(file.into()),
                    v1::ServerMsg::Cancel(v1::Download { file }) => {
                        self.on_cancel(file.as_ref()).await
                    }
                }
            }
            Message::Close(_) => {
                debug!(self.logger, "Got CLOSE frame");
                self.on_close(true).await;
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

    async fn on_finalize_failure(&self, err: anyhow::Error) {
        error!(self.logger, "Client failed on WS loop: {:?}", err);

        let err = match err.downcast::<crate::Error>() {
            Ok(err) => err,
            Err(err) => match err.downcast::<tungstenite::Error>() {
                Ok(err) => err.into(),
                Err(_) => crate::Error::BadTransferState,
            },
        };

        self.state
            .event_tx
            .send(Event::TransferFailed(self.xfer.clone(), err))
            .await
            .expect("Event channel should always be open");
    }

    async fn on_finalize_success(&self, mut socket: WsSink) {
        let task = async {
            // Drain messages
            while socket.next().await.transpose()?.is_some() {}
            anyhow::Ok(())
        };

        if let Err(err) = task.await {
            warn!(
                self.logger,
                "Failed to gracefully close the client connection: {}", err
            );
        } else {
            debug!(self.logger, "WS client disconnected");
        }
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .flat_file_list()
            .iter()
            .filter(|file| {
                self.tasks
                    .get(file.path())
                    .map_or(false, |task| !task.job.is_finished())
            })
            .for_each(|file| {
                let size = file.size_kb().unwrap_or_default();

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    Some(size),
                    0,
                )
            });

        self.stop_jobs().await;

        self.state
            .event_tx
            .send(Event::TransferCanceled(self.xfer.clone(), by_peer))
            .await
            .expect("Could not send a transfer cancelled event, channel closed");
    }

    async fn on_progress(&self, file: PathBuf, transfered: u64) {
        if let Some(task) = self.tasks.get(&file) {
            task.events
                .emit(Event::FileUploadProgress(
                    self.xfer.clone(),
                    Hidden(file.into_boxed_path()),
                    transfered,
                ))
                .await;
        }
    }

    async fn on_done(&mut self, file: PathBuf) {
        if let Some(task) = self.tasks.remove(&file) {
            task.events
                .stop(Event::FileUploadSuccess(
                    self.xfer.clone(),
                    Hidden(file.into_boxed_path()),
                ))
                .await;
        }
    }

    async fn on_error(&mut self, file: Option<&Path>, msg: String) {
        error!(
            self.logger,
            "Server reported and error: file: {:?}, message: {}",
            Hidden(&file),
            msg
        );

        if let Some(file) = file {
            if let Some(task) = self.tasks.remove(file) {
                if !task.job.is_finished() {
                    task.job.abort();

                    task.events
                        .stop(Event::FileUploadFailed(
                            self.xfer.clone(),
                            Hidden(file.into()),
                            crate::Error::BadTransfer,
                        ))
                        .await;
                }
            }
        }
    }

    fn on_download(&mut self, file: PathBuf) {
        let file_str = file.to_string_lossy().to_string();

        let f = || {
            match self.tasks.entry(file) {
                Entry::Occupied(o) => {
                    let task = o.into_mut();

                    if task.job.is_finished() {
                        *task = FileTask::new(
                            self.state.clone(),
                            self.upload_tx.clone(),
                            self.xfer.clone(),
                            file_str,
                            self.logger.clone(),
                        )?;
                    } else {
                        anyhow::bail!("Transfer already in progress");
                    }
                }
                Entry::Vacant(v) => {
                    let task = FileTask::new(
                        self.state.clone(),
                        self.upload_tx.clone(),
                        self.xfer.clone(),
                        file_str,
                        self.logger.clone(),
                    )?;

                    v.insert(task);
                }
            };

            anyhow::Ok(())
        };

        if let Err(err) = f() {
            error!(self.logger, "Failed to start upload: {:?}", err);
        }
    }

    async fn issue_cancel(&mut self, socket: &mut WsSink, file: &Path) -> anyhow::Result<()> {
        let msg = v1::ClientMsg::Cancel(v1::Download {
            file: file.to_string_lossy().to_string(),
        });

        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file).await;

        Ok(())
    }

    async fn on_cancel(&mut self, file: &Path) {
        if let Some(task) = self.tasks.remove(file) {
            if !task.job.is_finished() {
                task.job.abort();

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    self.xfer
                        .file(file)
                        .expect("File should exists since we have a transfer task running")
                        .size_kb(),
                    0,
                );

                task.events
                    .stop(Event::FileUploadCancelled(
                        self.xfer.clone(),
                        Hidden(file.into()),
                    ))
                    .await;
            }
        }
    }

    async fn stop_jobs(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.tasks.drain().map(|(_, task)| {
            task.job.abort();

            async move {
                task.events.stop_silent().await;
            }
        });

        futures::future::join_all(tasks).await;
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping client handler");
        self.tasks.values().for_each(|task| task.job.abort());
    }
}

fn start_upload(
    state: Arc<State>,
    events: Arc<FileEventTx>,
    sink: Sender<Message>,
    xfer: crate::Transfer,
    file: String,
    logger: Logger,
) -> anyhow::Result<JoinHandle<()>> {
    let xfile = xfer.file(file.as_ref()).context("File not found")?.clone();

    let transfer_time = Instant::now();

    state.moose.service_quality_transfer_file(
        Ok(()),
        drop_analytics::Phase::Start,
        xfer.id().to_string(),
        xfile.size_kb(),
        0,
    );

    let upload_job = async move {
        let send_file = async {
            events
                .start(Event::FileUploadStarted(
                    xfer.clone(),
                    Hidden(PathBuf::from(&file).into()),
                ))
                .await;

            let mut iofile = match xfile.open() {
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
                let chunk = match iofile.read_chunk()? {
                    Some(chunk) => v1::Chunk {
                        file: file.clone(),
                        data: chunk.to_vec(),
                    },
                    None => return Ok(()),
                };

                sink.send(Message::from(chunk))
                    .await
                    .map_err(|_| crate::Error::Canceled)?;
            }
        };

        let result = send_file.await;

        state.moose.service_quality_transfer_file(
            result.to_status(),
            drop_analytics::Phase::End,
            xfer.id().to_string(),
            xfile.size_kb(),
            transfer_time.elapsed().as_millis() as i32,
        );

        match result {
            Ok(()) => (),
            Err(crate::Error::Canceled) => (),
            Err(err) => {
                error!(
                    logger,
                    "Failed at service::download() while reading a file: {}", err
                );

                let _ = sink
                    .send(Message::from(&v1::ClientMsg::Error(v1::Error {
                        file: Some(file.clone()),
                        msg: err.to_string(),
                    })))
                    .await;

                events
                    .stop(Event::FileUploadFailed(
                        xfer.clone(),
                        Hidden(PathBuf::from(file).into_boxed_path()),
                        err,
                    ))
                    .await;
            }
        };
    };

    Ok(tokio::spawn(upload_job))
}
