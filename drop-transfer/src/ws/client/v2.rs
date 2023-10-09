use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use drop_core::Status;
use slog::{debug, error, warn};
use tokio::{
    sync::mpsc::Sender,
    task::{AbortHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

use super::{
    handler::{self, MsgToSend},
    WebSocket,
};
use crate::{
    file::FileSubPath,
    protocol::v2,
    service::State,
    tasks::AliveGuard,
    transfer::Transfer,
    ws::{self, events::FileEventTx},
    File, FileId, OutgoingTransfer,
};

pub struct HandlerInit<'a, const PING: bool = true> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
}

pub struct HandlerLoop<'a, const PING: bool> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    upload_tx: Sender<MsgToSend>,
    tasks: HashMap<FileSubPath, FileTask>,
    xfer: Arc<OutgoingTransfer>,
    alive: &'a AliveGuard,
}

struct Uploader {
    sink: Sender<MsgToSend>,
    file_subpath: FileSubPath,
    logger: slog::Logger,
}

struct FileTask {
    job: AbortHandle,
    events: Arc<FileEventTx<OutgoingTransfer>>,
}

impl<'a, const PING: bool> HandlerInit<'a, PING> {
    pub(crate) fn new(
        state: &'a Arc<State>,
        logger: &'a slog::Logger,
        alive: &'a AliveGuard,
    ) -> Self {
        Self {
            state,
            logger,
            alive,
        }
    }
}

#[async_trait::async_trait]
impl<'a, const PING: bool> handler::HandlerInit for HandlerInit<'a, PING> {
    type Pinger = ws::utils::Pinger<PING>;
    type Loop = HandlerLoop<'a, PING>;

    async fn start(
        &mut self,
        socket: &mut WebSocket,
        xfer: &OutgoingTransfer,
    ) -> crate::Result<()> {
        let req = v2::TransferRequest::from(xfer);
        socket.send(Message::from(&req)).await?;
        Ok(())
    }

    fn upgrade(self, upload_tx: Sender<MsgToSend>, xfer: Arc<OutgoingTransfer>) -> Self::Loop {
        let Self {
            state,
            logger,
            alive,
        } = self;

        HandlerLoop {
            state,
            logger,
            upload_tx,
            xfer,
            tasks: HashMap::new(),
            alive,
        }
    }

    fn pinger(&mut self) -> Self::Pinger {
        ws::utils::Pinger::<PING>::new()
    }

    fn recv_timeout(&mut self) -> Duration {
        if PING {
            drop_config::TRANFER_IDLE_LIFETIME
        } else {
            Duration::MAX
        }
    }
}

impl<const PING: bool> HandlerLoop<'_, PING> {
    async fn on_cancel(&mut self, file: FileSubPath) {
        if let Some(task) = self.tasks.remove(&file) {
            if !task.job.is_finished() {
                task.job.abort();
                task.events.pause().await;
            }
        }
    }

    async fn on_progress(&self, file: FileSubPath, transfered: u64) {
        if let Some(task) = self.tasks.get(&file) {
            task.events.progress(transfered).await;
        }
    }

    async fn on_done(&mut self, file: FileSubPath) {
        if let Some(file) = self.xfer.file_by_subpath(&file) {
            super::on_upload_finished(self.state, &self.xfer, file.id(), self.logger).await;
        }

        self.stop_task(&file, Status::FileFinished).await;
    }

    async fn on_download(&mut self, jobs: &mut JoinSet<()>, file_id: FileSubPath) {
        let start = async {
            let file = self
                .xfer
                .file_by_subpath(&file_id)
                .context("Invalid file")?;

            self.state
                .transfer_manager
                .outgoing_ensure_file_not_terminated(self.xfer.id(), file.id())
                .await?;

            match self.tasks.entry(file_id.clone()) {
                Entry::Occupied(o) => {
                    let task = o.into_mut();

                    if task.job.is_finished() {
                        *task = FileTask::new(
                            jobs,
                            self.state,
                            Uploader {
                                sink: self.upload_tx.clone(),
                                file_subpath: file_id.clone(),
                                logger: self.logger.clone(),
                            },
                            self.xfer.clone(),
                            file_id,
                            self.logger,
                            self.alive,
                        )
                        .await?;
                    } else {
                        anyhow::bail!("Transfer already in progress");
                    }
                }
                Entry::Vacant(v) => {
                    let task = FileTask::new(
                        jobs,
                        self.state,
                        Uploader {
                            sink: self.upload_tx.clone(),
                            file_subpath: file_id.clone(),
                            logger: self.logger.clone(),
                        },
                        self.xfer.clone(),
                        file_id,
                        self.logger,
                        self.alive,
                    )
                    .await?;

                    v.insert(task);
                }
            };

            anyhow::Ok(())
        };

        if let Err(err) = start.await {
            error!(self.logger, "Failed to start upload: {:?}", err);
        }
    }

    async fn on_error(&mut self, file: Option<FileSubPath>, msg: String) {
        error!(
            self.logger,
            "Server reported and error: file: {file:?}, message: {msg}",
        );

        if let Some(file) = file {
            if let Some(file) = self.xfer.file_by_subpath(&file) {
                super::on_upload_failure(self.state, &self.xfer, file.id(), msg, self.logger).await;
            }

            self.stop_task(&file, Status::BadTransferState).await;
        }
    }

    async fn stop_task(&mut self, file: &FileSubPath, status: Status) {
        if let Some(FileTask { job: task, events }) = self.tasks.remove(file) {
            if !task.is_finished() {
                debug!(
                    self.logger,
                    "Aborting download job: {}:{file:?}",
                    self.xfer.id()
                );

                task.abort();
                events.stop_silent(status).await;
            }
        }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> handler::HandlerLoop for HandlerLoop<'_, PING> {
    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let file_subpath = if let Some(file) = self.xfer.files().get(&file_id) {
            file.subpath().clone()
        } else {
            warn!(self.logger, "Missing file with ID: {file_id:?}");
            return Ok(());
        };

        let msg = v2::ClientMsg::Cancel(v2::Download {
            file: file_subpath.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.stop_task(&file_subpath, Status::FileRejected).await;

        Ok(())
    }

    async fn issue_failure(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let file_subpath = if let Some(file) = self.xfer.files().get(&file_id) {
            file.subpath().clone()
        } else {
            warn!(self.logger, "Missing file with ID: {file_id:?}");
            return Ok(());
        };

        let msg = v2::ClientMsg::Error(v2::Error {
            file: Some(file_subpath),
            msg: String::from("File failed elsewhere"),
        });
        socket.send(Message::from(&msg)).await?;

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        self.on_stop().await;

        if by_peer {
            self.state
                .emit_event(crate::Event::OutgoingTransferCanceled(
                    self.xfer.clone(),
                    by_peer,
                ));
        }
    }

    async fn on_text_msg(
        &mut self,
        _: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        text: String,
    ) -> anyhow::Result<()> {
        let msg: v2::ServerMsg =
            serde_json::from_str(&text).context("Failed to deserialize server message")?;

        match msg {
            v2::ServerMsg::Progress(v2::Progress {
                file,
                bytes_transfered,
            }) => self.on_progress(file, bytes_transfered).await,
            v2::ServerMsg::Done(v2::Progress {
                file,
                bytes_transfered: _,
            }) => self.on_done(file).await,
            v2::ServerMsg::Error(v2::Error { file, msg }) => self.on_error(file, msg).await,
            v2::ServerMsg::Start(v2::Download { file }) => self.on_download(jobs, file).await,
            v2::ServerMsg::Cancel(v2::Download { file }) => self.on_cancel(file).await,
        }

        Ok(())
    }

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.tasks.drain().map(|(_, task)| async move {
            task.events.stop_silent(Status::Canceled).await;
        });

        futures::future::join_all(tasks).await;
    }
}

impl<const PING: bool> Drop for HandlerLoop<'_, PING> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping client handler");

        let jobs = std::mem::take(&mut self.tasks);
        tokio::spawn(async move {
            let tasks = jobs.into_values().map(|task| async move {
                task.events.pause().await;
            });

            futures::future::join_all(tasks).await;
        });
    }
}

#[async_trait::async_trait]
impl handler::Uploader for Uploader {
    async fn chunk(&mut self, chunk: &[u8]) -> Result<(), crate::Error> {
        let msg = v2::Chunk {
            file: self.file_subpath.clone(),
            data: chunk.to_vec(),
        };

        self.sink
            .send(MsgToSend {
                msg: Message::from(msg),
            })
            .await
            .map_err(|_| crate::Error::Canceled)?;

        Ok(())
    }

    async fn error(&mut self, msg: String) {
        let msg = v2::ClientMsg::Error(v2::Error {
            file: Some(self.file_subpath.clone()),
            msg,
        });

        if let Err(e) = self
            .sink
            .send(MsgToSend {
                msg: Message::from(&msg),
            })
            .await
        {
            warn!(self.logger, "Failed to send error message: {:?}", e);
        };
    }

    fn offset(&self) -> u64 {
        0
    }
}

impl FileTask {
    async fn new(
        jobs: &mut JoinSet<()>,
        state: &Arc<State>,
        uploader: Uploader,
        xfer: Arc<OutgoingTransfer>,
        file: FileSubPath,
        logger: &slog::Logger,
        gaurd: &AliveGuard,
    ) -> anyhow::Result<Self> {
        let file_id = xfer
            .file_by_subpath(&file)
            .context("File not found")?
            .id()
            .clone();

        let (job, events) = super::start_upload(
            jobs,
            state.clone(),
            gaurd.clone(),
            logger.clone(),
            uploader,
            xfer,
            file_id,
        )
        .await?;

        Ok(Self { job, events })
    }
}
