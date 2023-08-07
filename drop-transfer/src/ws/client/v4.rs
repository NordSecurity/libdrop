use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use drop_analytics::TransferDirection;
use futures::SinkExt;
use slog::{debug, error, warn};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tokio_tungstenite::tungstenite::{self, Message};

use super::{handler, ClientReq, WebSocket};
use crate::{protocol::v4, service::State, ws, FileId};

pub struct HandlerInit<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
}

pub struct HandlerLoop<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
    upload_tx: Sender<Message>,
    tasks: HashMap<FileId, FileTask>,
    done: HashSet<FileId>,
    last_recv: Instant,
    xfer: crate::Transfer,
}

struct FileTask {
    job: JoinHandle<()>,
    events: Arc<ws::events::FileEventTx>,
}

struct Uploader {
    sink: Sender<Message>,
    file_id: FileId,
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
        let req = v4::TransferRequest::from(xfer);
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
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = v4::ClientMsg::Cancel(v4::Cancel {
            file: file_id.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file_id, false).await;

        Ok(())
    }

    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = v4::ClientMsg::Cancel(v4::Cancel {
            file: file_id.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        if let Some(task) = self.tasks.remove(&file_id) {
            if !task.job.is_finished() {
                task.job.abort();

                task.events
                    .stop(crate::Event::FileUploadCancelled(
                        self.xfer.clone(),
                        file_id.clone(),
                        false,
                    ))
                    .await;
            }
        }

        if let Some(file) = self.xfer.files().get(&file_id) {
            self.state.moose.service_quality_transfer_file(
                Err(drop_core::Status::FileRejected as i32),
                self.xfer.id().to_string(),
                0,
                TransferDirection::Upload,
                file.info(),
            );

            self.state
                .event_tx
                .send(crate::Event::FileUploadRejected {
                    transfer_id: self.xfer.id(),
                    file_id,
                    by_peer: false,
                })
                .await
                .expect("Event channel should be open");
        }

        Ok(())
    }

    async fn on_cancel(&mut self, file_id: FileId, by_peer: bool) {
        if let Some(task) = self.tasks.remove(&file_id) {
            if !task.job.is_finished() {
                task.job.abort();

                let file = self
                    .xfer
                    .files()
                    .get(&file_id)
                    .expect("File should exists since we have a transfer task running");

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    self.xfer.id().to_string(),
                    0,
                    TransferDirection::Upload,
                    file.info(),
                );

                task.events
                    .stop(crate::Event::FileUploadCancelled(
                        self.xfer.clone(),
                        file.id().clone(),
                        by_peer,
                    ))
                    .await;
            }
        }
    }

    async fn on_progress(&self, file_id: FileId, transfered: u64) {
        if let Some(task) = self.tasks.get(&file_id) {
            task.events
                .emit(crate::Event::FileUploadProgress(
                    self.xfer.clone(),
                    file_id,
                    transfered,
                ))
                .await;
        }
    }

    async fn on_done(&mut self, file_id: FileId) {
        let event = crate::Event::FileUploadSuccess(self.xfer.clone(), file_id.clone());

        if let Some(task) = self.tasks.remove(&file_id) {
            task.events.stop(event).await;
        } else if !self.done.contains(&file_id) {
            self.state
                .event_tx
                .send(event)
                .await
                .expect("Failed to emit event");
        }

        self.done.insert(file_id);
    }

    async fn on_checksum(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        limit: u64,
    ) -> anyhow::Result<()> {
        let f = async {
            {
                self.state
                    .transfer_manager
                    .lock()
                    .await
                    .ensure_file_not_rejected(self.xfer.id(), &file_id)?;
            }

            let xfile = self.xfer.files().get(&file_id).context("File not found")?;
            let checksum = tokio::task::block_in_place(|| xfile.checksum(limit))?;

            anyhow::Ok(v4::ReportChsum {
                file: file_id.clone(),
                limit,
                checksum,
            })
        };

        match f.await {
            Ok(report) => {
                socket
                    .send(Message::from(&v4::ClientMsg::ReportChsum(report)))
                    .await
                    .context("Failed to send checksum report")?;
            }
            Err(err) => {
                error!(self.logger, "Failed to report checksum: {:?}", err);

                let msg = v4::Error {
                    file: Some(file_id),
                    msg: err.to_string(),
                };
                socket
                    .send(Message::from(&v4::ClientMsg::Error(msg)))
                    .await
                    .context("Failed to report error")?;
            }
        }

        Ok(())
    }

    async fn on_start(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        offset: u64,
    ) -> anyhow::Result<()> {
        let start = async {
            {
                self.state
                    .transfer_manager
                    .lock()
                    .await
                    .ensure_file_not_rejected(self.xfer.id(), &file_id)?;
            }

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

            let msg = v4::Error {
                file: Some(file_id),
                msg: err.to_string(),
            };
            socket
                .send(Message::from(&v4::ClientMsg::Error(msg)))
                .await
                .context("Failed to report error")?;
        }

        Ok(())
    }

    async fn on_error(&mut self, file_id: Option<FileId>, msg: String) {
        error!(
            self.logger,
            "Server reported and error: file: {file_id:?}, message: {msg}",
        );

        if let Some(file_id) = file_id {
            if let Some(task) = self.tasks.remove(&file_id) {
                if !task.job.is_finished() {
                    task.job.abort();
                }

                let file = self
                    .xfer
                    .files()
                    .get(&file_id)
                    .expect("File should exists since we have a transfer task running");

                task.events
                    .stop(crate::Event::FileUploadFailed(
                        self.xfer.clone(),
                        file.id().clone(),
                        crate::Error::BadTransferState(format!(
                            "Receiver reported an error: {msg}"
                        )),
                    ))
                    .await;

                self.done.insert(file_id);
            }
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn on_req(&mut self, socket: &mut WebSocket, req: ClientReq) -> anyhow::Result<()> {
        match req {
            ClientReq::Cancel { file } => self.issue_cancel(socket, file).await,
            ClientReq::Reject { file } => self.issue_reject(socket, file).await,
        }
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .files()
            .values()
            .filter(|file| {
                self.tasks
                    .get(file.id())
                    .map_or(false, |task| !task.job.is_finished())
            })
            .for_each(|file| {
                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    self.xfer.id().to_string(),
                    0,
                    TransferDirection::Upload,
                    file.info(),
                )
            });

        self.on_stop().await;

        self.state
            .event_tx
            .send(crate::Event::TransferCanceled(
                self.xfer.clone(),
                true,
                by_peer,
            ))
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

                let msg: v4::ServerMsg =
                    serde_json::from_str(&json).context("Failed to deserialize server message")?;

                match msg {
                    v4::ServerMsg::Progress(v4::Progress {
                        file,
                        bytes_transfered,
                    }) => self.on_progress(file, bytes_transfered).await,
                    v4::ServerMsg::Done(v4::Done {
                        file,
                        bytes_transfered: _,
                    }) => self.on_done(file).await,
                    v4::ServerMsg::Error(v4::Error { file, msg }) => self.on_error(file, msg).await,
                    v4::ServerMsg::ReqChsum(v4::ReqChsum { file, limit }) => {
                        self.on_checksum(socket, file, limit).await?
                    }
                    v4::ServerMsg::Start(v4::Start { file, offset }) => {
                        self.on_start(socket, file, offset).await?
                    }
                    v4::ServerMsg::Cancel(v4::Cancel { file }) => self.on_cancel(file, true).await,
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
        error!(self.logger, "Client failed on WS loop: {err:?}");

        let err = match err.downcast::<crate::Error>() {
            Ok(err) => err,
            Err(err) => err.downcast::<tungstenite::Error>().map_or_else(
                |err| crate::Error::BadTransferState(err.to_string()),
                Into::into,
            ),
        };

        self.state
            .event_tx
            .send(crate::Event::TransferFailed(self.xfer.clone(), err, false))
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
        let msg = v4::Chunk {
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
        let msg = v4::ClientMsg::Error(v4::Error {
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
        file_id: FileId,
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
