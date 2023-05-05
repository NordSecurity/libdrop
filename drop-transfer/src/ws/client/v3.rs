use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use futures::SinkExt;
use slog::{debug, error, warn};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tokio_tungstenite::tungstenite::{self, Message};

use super::{handler, ClientReq, WebSocket};
use crate::{protocol::v3, service::State, utils::Hidden, ws, FileSubPath};

pub struct HandlerInit<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
}

pub struct HandlerLoop<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
    upload_tx: Sender<Message>,
    tasks: HashMap<FileSubPath, FileTask>,
    done: HashSet<FileSubPath>,
    last_recv: Instant,
    xfer: crate::Transfer,
}

struct FileTask {
    job: JoinHandle<()>,
    events: Arc<ws::events::FileEventTx>,
}

struct Uploader {
    sink: Sender<Message>,
    file_id: FileSubPath,
    offset: u64,
}

impl<'a> HandlerInit<'a> {
    pub(crate) fn new(state: Arc<State>, logger: &'a slog::Logger) -> Self {
        Self { state, logger }
    }
}

#[async_trait::async_trait]
impl<'a> handler::HandlerInit for HandlerInit<'a> {
    type Pinger = tokio::time::Interval;
    type Loop = HandlerLoop<'a>;

    async fn start(&mut self, socket: &mut WebSocket, xfer: &crate::Transfer) -> crate::Result<()> {
        let req = v3::TransferRequest::try_from(xfer)?;
        socket.send(Message::from(&req)).await?;
        Ok(())
    }

    fn upgrade(self, upload_tx: Sender<Message>, xfer: crate::Transfer) -> Self::Loop {
        let Self { state, logger } = self;

        HandlerLoop {
            state,
            logger,
            upload_tx,
            xfer,
            tasks: HashMap::new(),
            done: HashSet::new(),
            last_recv: Instant::now(),
        }
    }

    fn pinger(&mut self) -> Self::Pinger {
        tokio::time::interval(self.state.config.ping_interval())
    }
}

impl HandlerLoop<'_> {
    async fn issue_cancel(
        &mut self,
        socket: &mut WebSocket,
        file: FileSubPath,
    ) -> anyhow::Result<()> {
        let msg = v3::ClientMsg::Cancel(v3::Cancel { file: file.clone() });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file, false).await;

        Ok(())
    }

    async fn on_cancel(&mut self, file: FileSubPath, by_peer: bool) {
        if let Some(task) = self.tasks.remove(&file) {
            if !task.job.is_finished() {
                task.job.abort();

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    0,
                    self.xfer
                        .file_by_subpath(&file)
                        .expect("File should exists since we have a transfer task running")
                        .info(),
                );

                task.events
                    .stop(crate::Event::FileUploadCancelled(
                        self.xfer.clone(),
                        file,
                        by_peer,
                    ))
                    .await;
            }
        }
    }

    async fn on_progress(&self, file: FileSubPath, transfered: u64) {
        if let Some(task) = self.tasks.get(&file) {
            task.events
                .emit(crate::Event::FileUploadProgress(
                    self.xfer.clone(),
                    file,
                    transfered,
                ))
                .await;
        }
    }

    async fn on_done(&mut self, file: FileSubPath) {
        let event = crate::Event::FileUploadSuccess(self.xfer.clone(), file.clone());

        if let Some(task) = self.tasks.remove(&file) {
            task.events.stop(event).await;
        } else if !self.done.contains(&file) {
            self.state
                .event_tx
                .send(event)
                .await
                .expect("Failed to emit event");
        }

        self.done.insert(file);
    }

    async fn on_checksum(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileSubPath,
        limit: u64,
    ) -> anyhow::Result<()> {
        let f = || {
            let xfile = self
                .xfer
                .file_by_subpath(&file_id)
                .context("File not found")?;
            let checksum = tokio::task::block_in_place(|| xfile.checksum(limit))?;

            anyhow::Ok(v3::ReportChsum {
                file: file_id.clone(),
                limit,
                checksum,
            })
        };

        match f() {
            Ok(report) => {
                socket
                    .send(Message::from(&v3::ClientMsg::ReportChsum(report)))
                    .await
                    .context("Failed to send checksum report")?;
            }
            Err(err) => {
                error!(self.logger, "Failed to report checksum: {:?}", err);

                let msg = v3::Error {
                    file: Some(file_id),
                    msg: err.to_string(),
                };
                socket
                    .send(Message::from(&v3::ClientMsg::Error(msg)))
                    .await
                    .context("Failed to report error")?;
            }
        }

        Ok(())
    }

    async fn on_start(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileSubPath,
        offset: u64,
    ) -> anyhow::Result<()> {
        let start = async {
            match self.tasks.entry(file_id.clone()) {
                Entry::Occupied(o) => {
                    let task = o.into_mut();

                    if task.job.is_finished() {
                        *task = FileTask::start(
                            self.state.clone(),
                            self.logger,
                            self.upload_tx.clone(),
                            self.xfer.clone(),
                            file_id.clone(),
                            offset,
                        )
                        .await?;
                    } else {
                        anyhow::bail!("Transfer already in progress");
                    }
                }
                Entry::Vacant(v) => {
                    let task = FileTask::start(
                        self.state.clone(),
                        self.logger,
                        self.upload_tx.clone(),
                        self.xfer.clone(),
                        file_id.clone(),
                        offset,
                    )
                    .await?;

                    v.insert(task);
                }
            };

            self.done.remove(&file_id);
            anyhow::Ok(())
        };

        if let Err(err) = start.await {
            error!(self.logger, "Failed to start upload: {:?}", err);

            let msg = v3::Error {
                file: Some(file_id),
                msg: err.to_string(),
            };
            socket
                .send(Message::from(&v3::ClientMsg::Error(msg)))
                .await
                .context("Failed to report error")?;
        }

        Ok(())
    }

    async fn on_error(&mut self, file: Option<FileSubPath>, msg: String) {
        error!(
            self.logger,
            "Server reported and error: file: {:?}, message: {}",
            Hidden(&file),
            msg
        );

        if let Some(file) = file {
            if let Some(task) = self.tasks.remove(&file) {
                if !task.job.is_finished() {
                    task.job.abort();

                    task.events
                        .stop(crate::Event::FileUploadFailed(
                            self.xfer.clone(),
                            file,
                            crate::Error::BadTransfer,
                        ))
                        .await;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn on_req(&mut self, socket: &mut WebSocket, req: ClientReq) -> anyhow::Result<()> {
        match req {
            ClientReq::Cancel { file } => self.issue_cancel(socket, file).await,
        }
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .files()
            .values()
            .filter(|file| {
                self.tasks
                    .get(&file.subpath)
                    .map_or(false, |task| !task.job.is_finished())
            })
            .for_each(|file| {
                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    0,
                    file.info(),
                )
            });

        self.on_stop().await;

        self.state
            .event_tx
            .send(crate::Event::TransferCanceled(self.xfer.clone(), by_peer))
            .await
            .expect("Could not send a transfer cancelled event, channel closed");
    }

    async fn on_recv(
        &mut self,
        socket: &mut WebSocket,
        msg: Message,
    ) -> anyhow::Result<ControlFlow<()>> {
        self.last_recv = Instant::now();

        match msg {
            Message::Text(json) => {
                debug!(self.logger, "Received:\n\t{json}");

                let msg: v3::ServerMsg =
                    serde_json::from_str(&json).context("Failed to deserialize server message")?;

                match msg {
                    v3::ServerMsg::Progress(v3::Progress {
                        file,
                        bytes_transfered,
                    }) => self.on_progress(file, bytes_transfered).await,
                    v3::ServerMsg::Done(v3::Done {
                        file,
                        bytes_transfered: _,
                    }) => self.on_done(file).await,
                    v3::ServerMsg::Error(v3::Error { file, msg }) => self.on_error(file, msg).await,
                    v3::ServerMsg::ReqChsum(v3::ReqChsum { file, limit }) => {
                        self.on_checksum(socket, file, limit).await?
                    }
                    v3::ServerMsg::Start(v3::Start { file, offset }) => {
                        self.on_start(socket, file, offset).await?
                    }
                    v3::ServerMsg::Cancel(v3::Cancel { file }) => self.on_cancel(file, true).await,
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

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.tasks.drain().map(|(_, task)| {
            task.job.abort();

            async move {
                task.events.stop_silent().await;
            }
        });

        futures::future::join_all(tasks).await;
    }

    async fn finalize_failure(self, err: anyhow::Error) {
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
        debug!(self.logger, "Stopping client handler");
        self.tasks.values().for_each(|task| task.job.abort());
    }
}

#[async_trait::async_trait]
impl handler::Uploader for Uploader {
    async fn chunk(&mut self, chunk: &[u8]) -> Result<(), crate::Error> {
        let msg = v3::Chunk {
            file: self.file_id.clone(),
            data: chunk.to_vec(),
        };

        self.sink
            .send(Message::from(msg))
            .await
            .map_err(|_| crate::Error::Canceled)?;

        Ok(())
    }

    async fn error(&mut self, msg: String) {
        let msg = v3::ClientMsg::Error(v3::Error {
            file: Some(self.file_id.clone()),
            msg,
        });

        let _ = self.sink.send(Message::from(&msg)).await;
    }

    fn offset(&self) -> u64 {
        self.offset
    }
}

impl FileTask {
    async fn start(
        state: Arc<State>,
        logger: &slog::Logger,
        sink: Sender<Message>,
        xfer: crate::Transfer,
        file_id: FileSubPath,
        offset: u64,
    ) -> anyhow::Result<Self> {
        let events = Arc::new(ws::events::FileEventTx::new(&state));

        let uploader = Uploader {
            sink,
            file_id: file_id.clone(),
            offset,
        };

        let job = super::start_upload(
            state,
            logger.clone(),
            Arc::clone(&events),
            uploader,
            xfer,
            file_id,
        )
        .await?;

        Ok(Self { job, events })
    }
}
