use std::{
    collections::HashMap,
    fs, io,
    net::IpAddr,
    ops::ControlFlow,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use drop_config::DropConfig;
use futures::{SinkExt, StreamExt};
use slog::{debug, error, info, warn};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedSender},
    task::JoinHandle,
};
use warp::ws::{Message, WebSocket};

use super::{handler, ServerReq};
use crate::{
    file, file::FileSubPath, protocol::v3, service::State, utils::Hidden, ws::events::FileEventTx,
};

pub struct HandlerInit<'a> {
    peer: IpAddr,
    state: Arc<State>,
    logger: &'a slog::Logger,
}

pub struct HandlerLoop<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
    msg_tx: Sender<Message>,
    xfer: crate::Transfer,
    last_recv: Instant,
    jobs: HashMap<FileSubPath, FileTask>,
}

struct Downloader {
    logger: slog::Logger,
    file_id: FileSubPath,
    msg_tx: Sender<Message>,
    csum_rx: mpsc::Receiver<v3::ReportChsum>,
    offset: u64,
}

struct FileTask {
    job: JoinHandle<()>,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx>,
    csum_tx: mpsc::Sender<v3::ReportChsum>,
}

impl<'a> HandlerInit<'a> {
    pub(crate) fn new(peer: IpAddr, state: Arc<State>, logger: &'a slog::Logger) -> Self {
        Self {
            peer,
            state,
            logger,
        }
    }
}

#[async_trait::async_trait]
impl<'a> handler::HandlerInit for HandlerInit<'a> {
    type Request = (v3::TransferRequest, IpAddr, Arc<DropConfig>);
    type Loop = HandlerLoop<'a>;
    type Pinger = tokio::time::Interval;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request> {
        let msg = ws
            .next()
            .await
            .context("Did not received transfer request")?
            .context("Failed to receive transfer request")?;

        let msg = msg.to_str().ok().context("Expected JOSN message")?;
        debug!(self.logger, "Request received:\n\t{msg}");

        let req = serde_json::from_str(msg).context("Failed to deserialize transfer request")?;

        Ok((req, self.peer, self.state.config.clone()))
    }

    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()> {
        let msg = v3::ServerMsg::Error(v3::Error {
            file: None,
            msg: err.to_string(),
        });

        ws.send(Message::from(&msg))
            .await
            .context("Failed to send error message")?;
        Ok(())
    }

    fn upgrade(self, msg_tx: Sender<Message>, xfer: crate::Transfer) -> Self::Loop {
        let Self {
            peer: _,
            state,
            logger,
        } = self;

        HandlerLoop {
            state,
            msg_tx,
            xfer,
            last_recv: Instant::now(),
            jobs: HashMap::new(),
            logger,
        }
    }

    fn pinger(&mut self) -> Self::Pinger {
        tokio::time::interval(self.state.config.ping_interval())
    }
}

impl HandlerLoop<'_> {
    fn issue_download(
        &mut self,
        _: &mut WebSocket,
        task: super::FileXferTask,
    ) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(task.file.subpath())
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let file_subpath = task.file.subpath().clone();
        let state = FileTask::start(
            self.msg_tx.clone(),
            self.state.clone(),
            task,
            self.logger.clone(),
        );

        self.jobs.insert(file_subpath, state);

        Ok(())
    }

    async fn issue_cancel(
        &mut self,
        socket: &mut WebSocket,
        file: FileSubPath,
    ) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let msg = v3::ServerMsg::Cancel(v3::Cancel { file: file.clone() });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file, false).await;

        Ok(())
    }

    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file: FileSubPath,
        chunk: Vec<u8>,
    ) -> anyhow::Result<()> {
        if let Some(task) = self.jobs.get(&file) {
            if let Err(err) = task.chunks_tx.send(chunk) {
                let msg = v3::Error {
                    msg: format!("Failed to consume chunk for file: {file:?}, msg: {err}",),
                    file: Some(file),
                };

                socket
                    .send(Message::from(&v3::ServerMsg::Error(msg)))
                    .await?;
            }
        }

        Ok(())
    }

    async fn on_cancel(&mut self, file: FileSubPath, by_peer: bool) {
        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
            csum_tx: _,
        }) = self.jobs.remove(&file)
        {
            if !task.is_finished() {
                task.abort();

                let file = self
                    .xfer
                    .file_by_subpath(&file)
                    .expect("File should exists since we have a transfer task running");

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    0,
                    file.info(),
                );

                events
                    .stop(crate::Event::FileDownloadCancelled(
                        self.xfer.clone(),
                        file.id().clone(),
                        by_peer,
                    ))
                    .await;
            }
        }
    }

    async fn on_error(&mut self, file: Option<FileSubPath>, msg: String) {
        error!(
            self.logger,
            "Client reported and error: file: {:?}, message: {}", file, msg
        );

        if let Some(file) = file {
            if let Some(FileTask {
                job: task,
                events,
                chunks_tx: _,
                csum_tx: _,
            }) = self.jobs.remove(&file)
            {
                if !task.is_finished() {
                    task.abort();

                    let file = self
                        .xfer
                        .file_by_subpath(&file)
                        .expect("File should exists since we have a transfer task running");

                    events
                        .stop(crate::Event::FileDownloadFailed(
                            self.xfer.clone(),
                            file.id().clone(),
                            crate::Error::BadTransfer,
                        ))
                        .await;
                }
            }
        }
    }

    async fn on_checksum(&mut self, report: v3::ReportChsum) {
        if let Some(job) = self.jobs.get_mut(&report.file) {
            if job.csum_tx.send(report).await.is_err() {
                warn!(
                    self.logger,
                    "Failed to pass checksum report to receiver task"
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn on_req(&mut self, ws: &mut WebSocket, req: ServerReq) -> anyhow::Result<()> {
        match req {
            ServerReq::Download { task } => self.issue_download(ws, *task)?,
            ServerReq::Cancel { file } => self.issue_cancel(ws, file).await?,
        }

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ServerHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .files()
            .values()
            .filter(|file| {
                self.jobs
                    .get(&file.subpath)
                    .map_or(false, |state| !state.job.is_finished())
            })
            .for_each(|file| {
                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    0,
                    file.info(),
                );
            });

        self.on_stop().await;

        self.state
            .event_tx
            .send(crate::Event::TransferCanceled(self.xfer.clone(), by_peer))
            .await
            .expect("Could not send a file cancelled event, channel closed");
    }

    async fn on_recv(
        &mut self,
        ws: &mut WebSocket,
        msg: Message,
    ) -> anyhow::Result<ControlFlow<()>> {
        self.last_recv = Instant::now();

        if let Ok(json) = msg.to_str() {
            debug!(self.logger, "Received:\n\t{json}");

            let msg: v3::ClientMsg =
                serde_json::from_str(json).context("Failed to deserialize json")?;

            match msg {
                v3::ClientMsg::Error(v3::Error { file, msg }) => self.on_error(file, msg).await,
                v3::ClientMsg::Cancel(v3::Cancel { file }) => self.on_cancel(file, true).await,
                v3::ClientMsg::ReportChsum(report) => self.on_checksum(report).await,
            }
        } else if msg.is_binary() {
            let v3::Chunk { file, data } =
                v3::Chunk::decode(msg.into_bytes()).context("Failed to decode file chunk")?;

            self.on_chunk(ws, file, data).await?;
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

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.jobs.drain().map(|(_, task)| {
            task.job.abort();

            async move {
                task.events.stop_silent().await;
            }
        });

        futures::future::join_all(tasks).await;
    }

    async fn finalize_failure(self, err: anyhow::Error) {
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
            .send(crate::Event::TransferFailed(self.xfer.clone(), err))
            .await
            .expect("Event channel should always be open");
    }

    fn recv_timeout(&mut self) -> Option<Duration> {
        Some(
            self.state
                .config
                .transfer_idle_lifetime
                .saturating_sub(self.last_recv.elapsed()),
        )
    }
}

impl Drop for HandlerLoop<'_> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");
        self.jobs.values().for_each(|task| task.job.abort());
    }
}

impl Downloader {
    async fn send(&mut self, msg: impl Into<Message>) -> crate::Result<()> {
        self.msg_tx
            .send(msg.into())
            .await
            .map_err(|_| crate::Error::Canceled)
    }

    async fn request_csum(&mut self, limit: u64) -> crate::Result<v3::ReportChsum> {
        let msg = v3::ServerMsg::ReqChsum(v3::ReqChsum {
            file: self.file_id.clone(),
            limit,
        });
        self.send(Message::from(&msg)).await?;

        let report = self.csum_rx.recv().await.ok_or(crate::Error::Canceled)?;

        Ok(report)
    }

    async fn check_candidate_file(
        &mut self,
        task: &super::FileXferTask,
    ) -> crate::Result<Option<Hidden<PathBuf>>> {
        let mut csum = None;
        for variant in crate::utils::filepath_variants(&task.location.0)?.map(Hidden) {
            let meta = if let Ok(meta) = variant.symlink_metadata() {
                meta
            } else {
                // If the file does not exists we expect that furter iteration will not yield
                // more file with (i) suffix
                break;
            };

            if !meta.is_file() {
                continue;
            }

            if meta.len() != task.size {
                info!(
                    self.logger,
                    "Fould file {variant:?} but size does not match {}", task.size
                );
                continue;
            }

            // The file exists let's compare checksum
            let target_csum = if let Some(csum) = &csum {
                csum
            } else {
                let report = self.request_csum(task.size).await?;
                if report.limit != task.size {
                    error!(
                        self.logger,
                        "Checksum report missmatch. Most likely protocol error"
                    );
                    return Err(crate::Error::BadTransferState);
                }

                &*csum.insert(report.checksum)
            };

            let candidate_csum = match tokio::task::block_in_place(|| {
                let file = fs::File::open(&*variant)?;
                file::checksum(&mut io::BufReader::new(file))
            }) {
                Ok(csum) => csum,
                Err(err) => {
                    warn!(
                        self.logger,
                        "Failed to compute checksum for candidate file {variant:?}: {err}",
                    );
                    continue;
                }
            };

            if candidate_csum != *target_csum {
                info!(
                    self.logger,
                    "Fould file {variant:?} but checksum does not match"
                );
                continue;
            }

            // We found the file
            return Ok(Some(variant));
        }

        Ok(None)
    }
}

#[async_trait::async_trait]
impl handler::Downloader for Downloader {
    async fn init(&mut self, task: &super::FileXferTask) -> crate::Result<handler::DownloadInit> {
        // Check if the file is already there
        match self.check_candidate_file(task).await? {
            Some(destination) => {
                info!(
                    self.logger,
                    "Found the file here {destination:?}, it's already downloaded"
                );

                let msg = v3::ServerMsg::Done(v3::Done {
                    file: self.file_id.clone(),
                    bytes_transfered: task.size,
                });
                self.send(Message::from(&msg)).await?;

                return Ok(handler::DownloadInit::AlreadyDone { destination });
            }
            None => debug!(self.logger, "Did not find any candidate files"),
        };

        let tmp_location: Hidden<PathBuf> =
            Hidden(format!("{}.dropdl-part", task.location.display(),).into());
        super::validate_tmp_location_path(&tmp_location)?;

        // Check if we can resume the temporary file
        match tokio::task::block_in_place(|| super::TmpFileState::load(&tmp_location.0)) {
            Ok(super::TmpFileState { meta, csum }) => {
                debug!(
                    self.logger,
                    "Found temporary file: {tmp_location:?}, of size: {}",
                    meta.len()
                );

                let report = self.request_csum(meta.len()).await?;

                if report.limit == meta.len() && report.checksum == csum {
                    // All matches, we can continue with temp file
                    self.offset = meta.len();
                } else {
                    info!(
                        self.logger,
                        "Found missmatch in partially downloaded file, overwriting"
                    );
                }
            }
            Err(err) => {
                debug!(self.logger, "Failed to load temporary file info: {err}");
            }
        };

        let msg = v3::ServerMsg::Start(v3::Start {
            file: self.file_id.clone(),
            offset: self.offset,
        });
        self.send(Message::from(&msg)).await?;

        Ok(handler::DownloadInit::Stream {
            offset: self.offset,
            tmp_location,
        })
    }

    async fn open(&mut self, path: &Hidden<PathBuf>) -> crate::Result<fs::File> {
        let file = if self.offset == 0 {
            fs::File::create(&path.0)?
        } else {
            fs::File::options().append(true).open(&path.0)?
        };

        Ok(file)
    }

    async fn progress(&mut self, bytes: u64) -> crate::Result<()> {
        self.send(&v3::ServerMsg::Progress(v3::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn done(&mut self, bytes: u64) -> crate::Result<()> {
        self.send(&v3::ServerMsg::Done(v3::Done {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn error(&mut self, msg: String) -> crate::Result<()> {
        self.send(&v3::ServerMsg::Error(v3::Error {
            file: Some(self.file_id.clone()),
            msg,
        }))
        .await
    }
}

impl FileTask {
    fn start(
        msg_tx: Sender<Message>,
        state: Arc<State>,
        task: super::FileXferTask,
        logger: slog::Logger,
    ) -> Self {
        let events = Arc::new(FileEventTx::new(&state));
        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();
        let (csum_tx, csum_rx) = mpsc::channel(4);

        let downloader = Downloader {
            file_id: task.file.subpath().clone(),
            msg_tx,
            logger: logger.clone(),
            csum_rx,
            offset: 0,
        };
        let job = tokio::spawn(task.run(state, Arc::clone(&events), downloader, chunks_rx, logger));

        Self {
            job,
            chunks_tx,
            events,
            csum_tx,
        }
    }
}

impl handler::Request for (v3::TransferRequest, IpAddr, Arc<DropConfig>) {
    fn parse(self) -> anyhow::Result<crate::Transfer> {
        self.try_into().context("Failed to parse transfer request")
    }
}
