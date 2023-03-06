use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use sha1::{Digest, Sha1};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use super::events::FileEventTx;
use crate::{
    error::ResultExt,
    event::DownloadSuccess,
    manager::{TransferConnection, TransferGuard},
    protocol::{self, v1},
    quarantine::PathExt,
    service::State,
    utils::Hidden,
    Error, Event, Transfer,
};

pub enum ServerReq {
    Download {
        file: PathBuf,
        task: Box<FileXferTask>,
    },
    Cancel {
        file: PathBuf,
    },
}

pub(crate) fn start(
    addr: IpAddr,
    stop: CancellationToken,
    state: Arc<State>,
    logger: Logger,
) -> crate::Result<JoinHandle<()>> {
    let service = {
        let stop = stop.clone();
        let logger = logger.clone();

        warp::path("drop")
            .and(warp::path::param().and_then(|version: String| async move {
                version
                    .parse::<protocol::Version>()
                    .map_err(|_| warp::reject::not_found())
            }))
            .and(warp::ws())
            .and(warp::filters::addr::remote())
            .map(
                move |version: protocol::Version, ws: warp::ws::Ws, peer: Option<SocketAddr>| {
                    let state = Arc::clone(&state);
                    let peer = peer.expect("Transport should use IP addresses");
                    let stop = stop.clone();
                    let logger = logger.clone();

                    ws.on_upgrade(move |socket| async move {
                        info!(logger, "Client requested protocol version: {}", version);

                        match version {
                            protocol::Version::V1 => {
                                on_client_v1(socket, peer.ip(), state, stop, logger).await
                            }
                            protocol::Version::V2 => {
                                on_client_v2(socket, peer.ip(), state, stop, logger).await
                            }
                        }
                    })
                },
            )
    };

    let future = match warp::serve(service)
        .try_bind_with_graceful_shutdown((addr, drop_config::PORT), async move {
            stop.cancelled().await
        }) {
        Ok((_, future)) => future,
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

    let task = tokio::spawn(async move {
        future.await;
        debug!(logger, "WS server stopped");
    });

    Ok(task)
}

async fn on_client_v1(
    socket: WebSocket,
    peer: IpAddr,
    state: Arc<State>,
    stop: CancellationToken,
    logger: Logger,
) {
    on_client_v1_v2::<false>(socket, peer, state, stop, logger).await
}

async fn on_client_v2(
    socket: WebSocket,
    peer: IpAddr,
    state: Arc<State>,
    stop: CancellationToken,
    logger: Logger,
) {
    on_client_v1_v2::<true>(socket, peer, state, stop, logger).await
}

async fn receive_request(socket: &mut WebSocket) -> anyhow::Result<v1::TransferRequest> {
    let msg = socket
        .next()
        .await
        .context("Did not received transfer request")?
        .context("Failed to receive transfer request")?;

    let msg = msg.to_str().ok().context("Expected JOSN message")?;

    serde_json::from_str(msg).context("Failed to deserialize transfer request")
}

async fn on_client_v1_v2<const PING: bool>(
    mut socket: WebSocket,
    peer: IpAddr,
    state: Arc<State>,
    stop: CancellationToken,
    logger: Logger,
) {
    let recv_task = receive_request(&mut socket);

    let xfer = tokio::select! {
        biased;

        _ = stop.cancelled() => {
            debug!(logger, "Stoppint client request on shutdown");
            return;
        },
        r = recv_task => {
            match r {
                Ok(xfer) => xfer,
                Err(err) => {
                    error!(logger, "Failed to initiate transfer: {:?}", err);
                    return;
                }
            }
        },
    };

    let xfer = match crate::Transfer::try_from((xfer, peer, &state.config)) {
        Ok(xfer) => {
            debug!(logger, "on_connect_v1() called with {:?}", xfer);
            xfer
        }
        Err(err) => {
            let close_on_err = async {
                let msg = v1::ServerMsg::Error(v1::Error {
                    file: None,
                    msg: err.to_string(),
                });

                socket
                    .send(Message::from(&msg))
                    .await
                    .context("Failed to send error message")?;
                socket.close().await.context("Failed to close socket")?;

                anyhow::Ok(())
            };

            if let Err(err) = close_on_err.await {
                error!(
                    logger,
                    "Failed to close connection on invalid request: {:?}", err
                );
            }

            return;
        }
    };

    let stop_job = {
        let state = state.clone();
        let xfer = xfer.clone();
        let logger = logger.clone();

        async move {
            // Stop the download job
            info!(logger, "Aborting transfer download");

            state
                .event_tx
                .send(Event::TransferFailed(xfer, Error::Canceled))
                .await
                .expect("Failed to send TransferFailed event");
        }
    };

    let job = handle_client_v1_v2::<PING>(socket, xfer, state, logger);

    tokio::select! {
        biased;

        _ = stop.cancelled() => {
            stop_job.await;
        },
        _ = job => (),
    };
}

async fn handle_client_v1_v2<const PING: bool>(
    mut socket: WebSocket,
    xfer: Transfer,
    state: Arc<State>,
    logger: Logger,
) {
    let _guard = TransferGuard::new(state.clone(), xfer.id());
    let (req_send, mut req_rx) = mpsc::unbounded_channel();

    {
        if let Err(err) = state
            .transfer_manager
            .lock()
            .await
            .insert_transfer(xfer.clone(), TransferConnection::Server(req_send))
        {
            error!(logger, "Failed to insert a new trasfer: {}", err);

            let msg = v1::Error {
                file: None,
                msg: err.to_string(),
            };

            let _ = socket.send(Message::from(&v1::ServerMsg::Error(msg))).await;
            let _ = socket.close().await;

            return;
        } else {
            state
                .event_tx
                .send(Event::RequestReceived(xfer.clone()))
                .await
                .expect("Failed to notify receiving peer!");
        }
    }

    let (send_tx, mut send_rx) = mpsc::channel(2);
    let mut ping = super::utils::Pinger::<PING>::new(&state);

    let mut handler = ServerHandler::new(send_tx, state, xfer, logger.clone());

    let task = async {
        loop {
            tokio::select! {
                biased;

                // API request
                req = req_rx.recv() => {
                    if let Some(req) = req {
                        handler.on_req(&mut socket, req).await?;
                    } else {
                        debug!(logger, "Stoppping server connection gracefuly");
                        handler.on_close(false).await;
                        break;
                    }
                },
                // Message received
                recv = super::utils::recv(&mut socket, handler.timeout::<PING>()) => {
                    match recv? {
                        Some(msg) => {
                            if handler.on_recv(&mut socket, msg).await?.is_break() {
                                break;
                            }
                        },
                        None => break,
                    };
                },
                // Message to send down the wire
                msg = send_rx.recv() => {
                    let msg = msg.expect("Handler channel should always be open");
                    socket.send(Message::from(&msg)).await?;
                },
                _ = ping.tick() => {
                    socket.send(Message::ping(Vec::new())).await.context("Failed to send PING message")?;
                }
            };
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

struct ServerHandler {
    // Used to send feedback from file reading threads
    transfer_sink: Sender<v1::ServerMsg>,
    jobs: HashMap<PathBuf, FileTask>,
    state: Arc<State>,
    xfer: Transfer,
    logger: Logger,
    last_recv: Instant,
}

struct FileTask {
    job: JoinHandle<()>,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx>,
}

impl FileTask {
    fn start(
        sink: Sender<v1::ServerMsg>,
        state: Arc<State>,
        file: PathBuf,
        task: Box<FileXferTask>,
        logger: Logger,
    ) -> Self {
        let events = Arc::new(FileEventTx::new(&state));
        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();

        let job = tokio::spawn(task.run(state, Arc::clone(&events), sink, chunks_rx, file, logger));

        Self {
            job,
            chunks_tx,
            events,
        }
    }
}

impl ServerHandler {
    fn new(sink: Sender<v1::ServerMsg>, state: Arc<State>, xfer: Transfer, logger: Logger) -> Self {
        Self {
            transfer_sink: sink,
            state,
            jobs: HashMap::new(),
            xfer,
            logger,
            last_recv: Instant::now(),
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

    async fn on_recv(
        &mut self,
        socket: &mut WebSocket,
        msg: Message,
    ) -> anyhow::Result<ControlFlow<()>> {
        self.last_recv = Instant::now();

        if let Ok(json) = msg.to_str() {
            let msg: v1::ClientMsg =
                serde_json::from_str(json).context("Failed to deserialize json")?;

            match msg {
                v1::ClientMsg::Error(v1::Error { file, msg }) => {
                    self.on_error(file.as_deref().map(AsRef::as_ref), msg).await
                }

                v1::ClientMsg::Cancel(v1::Download { file }) => self.on_cancel(file.as_ref()).await,
            }
        } else if msg.is_binary() {
            let v1::Chunk { file, data } =
                v1::Chunk::decode(msg.into_bytes()).context("Failed to decode file chunk")?;

            self.on_chunk(socket, file.as_ref(), data).await?;
        } else if msg.is_close() {
            debug!(self.logger, "Got CLOSE frame");
            self.on_close(true).await;

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

    async fn on_finalize_failure(&self, err: anyhow::Error) {
        error!(self.logger, "Server failed to handle WS message: {:?}", err);

        let err = match err.downcast::<crate::Error>() {
            Ok(err) => err,
            Err(err) => match err.downcast::<warp::Error>() {
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

    async fn on_finalize_success(&self, mut socket: WebSocket) {
        let task = async {
            socket.send(Message::close()).await?;

            // Drain messages
            while socket.next().await.transpose()?.is_some() {}
            socket.close().await
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

    async fn on_req(&mut self, socket: &mut WebSocket, req: ServerReq) -> anyhow::Result<()> {
        match req {
            ServerReq::Download { file, task } => self.issue_download(socket, file, task).await,
            ServerReq::Cancel { file } => self.issue_cancel(socket, &file).await,
        }
    }

    async fn on_error(&mut self, file: Option<&Path>, msg: String) {
        error!(
            self.logger,
            "Client reported and error: file: {:?}, message: {}",
            Hidden(&file),
            msg
        );

        if let Some(file) = file {
            if let Some(FileTask {
                job: task,
                events,
                chunks_tx: _,
            }) = self.jobs.remove(file)
            {
                if !task.is_finished() {
                    task.abort();

                    events
                        .stop(Event::FileDownloadFailed(
                            self.xfer.clone(),
                            Hidden(file.into()),
                            Error::BadTransfer,
                        ))
                        .await;
                }
            }
        }
    }

    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file: &Path,
        chunk: Vec<u8>,
    ) -> anyhow::Result<()> {
        if let Some(task) = self.jobs.get(file) {
            if let Err(err) = task.chunks_tx.send(chunk) {
                let msg = v1::Error {
                    file: Some(file.to_string_lossy().to_string()),
                    msg: format!(
                        "Failed to consue chunk for file: {}, msg: {err}",
                        file.display(),
                    ),
                };

                socket
                    .send(Message::from(&v1::ServerMsg::Error(msg)))
                    .await?;
            }
        }

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ServerHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .flat_file_list()
            .iter()
            .filter(|file| {
                self.jobs
                    .get(file.path())
                    .map_or(false, |state| !state.job.is_finished())
            })
            .for_each(|file| {
                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    file.size_kb(),
                    0,
                );
            });

        self.stop_jobs().await;

        self.state
            .event_tx
            .send(Event::TransferCanceled(self.xfer.clone(), by_peer))
            .await
            .expect("Could not send a file cancelled event, channel closed");
    }

    async fn issue_cancel(&mut self, socket: &mut WebSocket, file: &Path) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let msg = v1::ServerMsg::Cancel(v1::Download {
            file: file.to_string_lossy().to_string(),
        });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file).await;

        Ok(())
    }

    async fn on_cancel(&mut self, file: &Path) {
        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
        }) = self.jobs.remove(file)
        {
            if !task.is_finished() {
                task.abort();

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

                events
                    .stop(Event::FileDownloadCancelled(
                        self.xfer.clone(),
                        Hidden(file.into()),
                    ))
                    .await;
            }
        }
    }

    async fn issue_download(
        &mut self,
        socket: &mut WebSocket,
        file: PathBuf,
        task: Box<FileXferTask>,
    ) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(&file)
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let msg = v1::ServerMsg::Start(v1::Download {
            file: file.to_string_lossy().to_string(),
        });
        socket.send(Message::from(&msg)).await?;

        let state = FileTask::start(
            self.transfer_sink.clone(),
            self.state.clone(),
            file.clone(),
            task,
            self.logger.clone(),
        );

        self.jobs.insert(file, state);

        Ok(())
    }

    async fn stop_jobs(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.jobs.drain().map(|(_, task)| {
            task.job.abort();

            async move {
                task.events.stop_silent().await;
            }
        });

        futures::future::join_all(tasks).await;
    }
}

impl Drop for ServerHandler {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");
        self.jobs.values().for_each(|task| task.job.abort());
    }
}

pub struct FileXferTask {
    pub file: crate::File,
    pub location: Hidden<PathBuf>,
    pub filename: String,
    pub tmp_location: Hidden<PathBuf>,
    pub size: u64,
    pub xfer: crate::Transfer,
}

impl FileXferTask {
    pub fn new(file: crate::File, xfer: crate::Transfer, location: PathBuf) -> crate::Result<Self> {
        let filename = location
            .file_stem()
            .ok_or(crate::Error::BadFile)?
            .to_string_lossy()
            .to_string();
        let mut suffix = Sha1::new();

        suffix.update(xfer.id().as_bytes());
        if let Ok(time) = SystemTime::now().elapsed() {
            suffix.update(time.as_nanos().to_ne_bytes());
        }

        let suffix: String = suffix
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let tmp_location = format!(
            "{}.dropdl-{}",
            location.display(),
            suffix.get(..8).unwrap_or(&suffix),
        )
        .into();
        let size = file.size().ok_or(crate::Error::DirectoryNotExpected)?;

        Ok(Self {
            file,
            xfer,
            location: Hidden(location),
            filename,
            tmp_location: Hidden(tmp_location),
            size,
        })
    }

    fn move_tmp_to_dst(&self, logger: &Logger) -> crate::Result<PathBuf> {
        let dst_location = crate::utils::map_path_if_exists(&self.location.0)?;

        fs::rename(&self.tmp_location.0, &dst_location)?;

        if let Err(err) = dst_location.quarantine() {
            error!(logger, "Failed to quarantine downloaded file: {}", err);
        }

        Ok(dst_location)
    }

    async fn run(
        self,
        state: Arc<State>,
        events: Arc<FileEventTx>,
        sink: Sender<v1::ServerMsg>,
        mut stream: UnboundedReceiver<Vec<u8>>,
        file: PathBuf,
        logger: Logger,
    ) {
        let send_feedback =
            |msg| async { sink.send(msg).await.map_err(|_| crate::Error::Canceled) };

        let transfer_time = Instant::now();

        state.moose.service_quality_transfer_file(
            Ok(()),
            drop_analytics::Phase::Start,
            self.xfer.id().to_string(),
            self.file.size_kb(),
            0,
        );

        events
            .start(crate::Event::FileDownloadStarted(
                self.xfer.clone(),
                Hidden(self.file.path().into()),
            ))
            .await;

        let receive_file = async {
            let mut out_file = match fs::File::create(&self.tmp_location.0) {
                Ok(out_file) => out_file,
                Err(err) => {
                    error!(
                        logger,
                        "Could not create {:?} for downloading: {}", self.tmp_location, err
                    );

                    return Err(crate::Error::from(err));
                }
            };

            let consume_file_chunks = async {
                let mut bytes_received = 0;
                let mut last_progress = 0;

                while bytes_received < self.size {
                    let chunk = stream.recv().await.ok_or(crate::Error::Canceled)?;

                    let chunk_size = chunk.len();
                    if chunk_size as u64 + bytes_received > self.size {
                        return Err(crate::Error::MismatchedSize);
                    }

                    out_file.write_all(&chunk)?;

                    bytes_received += chunk_size as u64;

                    let current_progress =
                        ((bytes_received as f64 / self.size as f64) * 100.0) as u8;

                    // send progress to the caller
                    if current_progress != last_progress {
                        send_feedback(v1::ServerMsg::Progress(v1::Progress {
                            file: file.to_string_lossy().to_string(),
                            bytes_transfered: bytes_received,
                        }))
                        .await?;

                        events
                            .emit(crate::Event::FileDownloadProgress(
                                self.xfer.clone(),
                                Hidden(self.file.path().into()),
                                bytes_received,
                            ))
                            .await;
                    }

                    last_progress = current_progress;
                }

                if bytes_received > self.size {
                    Err(crate::Error::UnexpectedData)
                } else {
                    Ok(bytes_received)
                }
            };

            let bytes_received = match consume_file_chunks.await {
                Ok(br) => br,
                Err(err) => {
                    if let Err(ioerr) = fs::remove_file(&self.tmp_location.0) {
                        error!(
                            logger,
                            "Could not remove temporary file {:?} after failed download: {}",
                            self.tmp_location,
                            ioerr
                        );
                    }

                    return Err(err);
                }
            };

            send_feedback(v1::ServerMsg::Done(v1::Progress {
                file: file.to_string_lossy().to_string(),
                bytes_transfered: bytes_received,
            }))
            .await?;

            let dst = match self.move_tmp_to_dst(&logger) {
                Ok(dst) => dst,
                Err(err) => {
                    error!(
                        logger,
                        "Could not rename temporary file {:?} after downloading: {}",
                        self.location,
                        err
                    );
                    return Err(err);
                }
            };

            Ok(dst)
        };

        let result = receive_file.await;

        state.moose.service_quality_transfer_file(
            result.to_status(),
            drop_analytics::Phase::End,
            self.xfer.id().to_string(),
            self.file.size_kb(),
            transfer_time.elapsed().as_millis() as i32,
        );

        let event = match result {
            Ok(dst_location) => Some(Event::FileDownloadSuccess(
                self.xfer.clone(),
                DownloadSuccess {
                    id: Hidden(self.file.path().into()),
                    final_path: Hidden(
                        PathBuf::from(
                            dst_location
                                .as_path()
                                .file_name()
                                .expect("Invalid output path"),
                        )
                        .into_boxed_path(),
                    ),
                },
            )),
            Err(crate::Error::Canceled) => None,
            Err(err) => {
                let _ = send_feedback(v1::ServerMsg::Error(v1::Error {
                    file: Some(file.to_string_lossy().to_string()),
                    msg: err.to_string(),
                }))
                .await;

                Some(Event::FileDownloadFailed(
                    self.xfer.clone(),
                    Hidden(self.file.path().into()),
                    err,
                ))
            }
        };

        if let Some(event) = event {
            events.stop(event).await;
        }
    }
}

impl Drop for FileXferTask {
    fn drop(&mut self) {
        let _ = fs::remove_file(&*self.tmp_location);
    }
}
