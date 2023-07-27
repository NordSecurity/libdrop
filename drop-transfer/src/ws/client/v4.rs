use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use futures::SinkExt;
use slog::{debug, error};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tokio_tungstenite::tungstenite::Message;

use super::{handler, WebSocket};
use crate::{
    protocol::v4, service::State, transfer::Transfer, ws::events::FileEventTx, FileId,
    OutgoingTransfer,
};

pub struct HandlerInit<'a> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
}

pub struct HandlerLoop<'a> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    upload_tx: Sender<Message>,
    tasks: HashMap<FileId, FileTask>,
    done: HashSet<FileId>,
    xfer: Arc<OutgoingTransfer>,
}

struct FileTask {
    job: JoinHandle<()>,
    events: Arc<FileEventTx<OutgoingTransfer>>,
}

struct Uploader {
    sink: Sender<Message>,
    file_id: FileId,
    offset: u64,
}

impl<'a> HandlerInit<'a> {
    pub(crate) fn new(state: &'a Arc<State>, logger: &'a slog::Logger) -> Self {
        Self { state, logger }
    }
}

#[async_trait::async_trait]
impl<'a> handler::HandlerInit for HandlerInit<'a> {
    type Pinger = tokio::time::Interval;
    type Loop = HandlerLoop<'a>;

    async fn start(
        &mut self,
        socket: &mut WebSocket,
        xfer: &OutgoingTransfer,
    ) -> crate::Result<()> {
        let req = v4::TransferRequest::from(xfer);
        socket.send(Message::from(&req)).await?;
        Ok(())
    }

    fn upgrade(self, upload_tx: Sender<Message>, xfer: Arc<OutgoingTransfer>) -> Self::Loop {
        let Self { state, logger } = self;

        HandlerLoop {
            state,
            logger,
            upload_tx,
            xfer,
            tasks: HashMap::new(),
            done: HashSet::new(),
        }
    }

    fn pinger(&mut self) -> Self::Pinger {
        tokio::time::interval(self.state.config.ping_interval())
    }
}

impl HandlerLoop<'_> {
    async fn on_cancel(&mut self, file_id: FileId, by_peer: bool) {
        if let Some(task) = self.tasks.remove(&file_id) {
            if !task.job.is_finished() {
                task.job.abort();
                task.events.cancelled(by_peer).await;
            }
        }
    }

    async fn on_progress(&self, file_id: FileId, transfered: u64) {
        if let Some(task) = self.tasks.get(&file_id) {
            task.events.progress(transfered).await;
        }
    }

    async fn on_done(&mut self, file_id: FileId) {
        if let Some(task) = self.tasks.remove(&file_id) {
            task.events.success().await;
        } else if !self.done.contains(&file_id) {
            let event = crate::Event::FileUploadSuccess(self.xfer.clone(), file_id.clone());
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
            self.state
                .transfer_manager
                .outgoing_ensure_file_not_rejected(self.xfer.id(), &file_id)
                .await?;

            let xfile = &self.xfer.files()[&file_id];
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
            self.state
                .transfer_manager
                .outgoing_ensure_file_not_rejected(self.xfer.id(), &file_id)
                .await?;

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

                task.events
                    .failed(crate::Error::BadTransferState(format!(
                        "Receiver reported an error: {msg}"
                    )))
                    .await;

                self.done.insert(file_id);
            }
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = v4::ClientMsg::Cancel(v4::Cancel {
            file: file_id.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.state
            .transfer_manager
            .outgoing_rejection_ack(self.xfer.id(), &file_id)
            .await?;

        if let Some(task) = self.tasks.remove(&file_id) {
            if !task.job.is_finished() {
                task.job.abort();
                task.events.cancelled_on_rejection(false).await;
            }
        }

        Ok(())
    }
    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        if by_peer {
            self.state
                .event_tx
                .send(crate::Event::OutgoingTransferCanceled(
                    self.xfer.clone(),
                    by_peer,
                ))
                .await
                .expect("Could not send a transfer cancelled event, channel closed");
        }

        self.on_stop().await;
    }

    async fn on_text_msg(&mut self, socket: &mut WebSocket, text: String) -> anyhow::Result<()> {
        let msg: v4::ServerMsg =
            serde_json::from_str(&text).context("Failed to deserialize server message")?;

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

        Ok(())
    }

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.tasks.drain().map(|(_, task)| {
            task.job.abort();

            async move {
                task.events.cancel_silent().await;
            }
        });

        futures::future::join_all(tasks).await;
    }

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration> {
        Some(
            self.state
                .config
                .transfer_idle_lifetime
                .saturating_sub(last_recv_elapsed),
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
        xfer: Arc<OutgoingTransfer>,
        file_id: FileId,
        offset: u64,
    ) -> anyhow::Result<Self> {
        let events = Arc::new(FileEventTx::new(&state, xfer.clone(), file_id.clone()));

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
