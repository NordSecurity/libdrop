use std::{
    cmp::Ordering, collections::HashMap, fs, future::Future, net::IpAddr, path::PathBuf, sync::Arc,
};

use anyhow::Context;
use async_cell::sync::AsyncCell;
use drop_config::DropConfig;
use drop_core::Status;
use slog::{debug, error, info, warn};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedSender},
    task::{AbortHandle, JoinSet},
};
use warp::ws::Message;

use super::{
    handler::{self, MsgToSend},
    socket::WebSocket,
    TmpFileState,
};
use crate::{
    file::{self},
    manager::FileTerminalState,
    protocol::v4,
    service::State,
    tasks::AliveGuard,
    transfer::{IncomingTransfer, Transfer},
    utils::Hidden,
    ws::events::FileEventTx,
    File, FileId,
};

pub struct HandlerInit<'a> {
    peer: IpAddr,
    state: Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
}

pub struct HandlerLoop<'a> {
    state: Arc<State>,
    logger: &'a slog::Logger,
    msg_tx: Sender<MsgToSend>,
    xfer: Arc<IncomingTransfer>,
    jobs: HashMap<FileId, FileTask>,
    checksums: HashMap<FileId, Arc<AsyncCell<[u8; 32]>>>,
}

struct Downloader {
    logger: slog::Logger,
    file_id: FileId,
    msg_tx: Sender<MsgToSend>,
    csum_rx: mpsc::Receiver<v4::ReportChsum>,
    full_csum: Arc<AsyncCell<[u8; 32]>>,
    offset: u64,
}

struct FileTask {
    job: AbortHandle,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx<IncomingTransfer>>,
    csum_tx: mpsc::Sender<v4::ReportChsum>,
}

impl<'a> HandlerInit<'a> {
    pub(crate) fn new(
        peer: IpAddr,
        state: Arc<State>,
        logger: &'a slog::Logger,
        alive: &'a AliveGuard,
    ) -> Self {
        Self {
            peer,
            state,
            logger,
            alive,
        }
    }
}

#[async_trait::async_trait]
impl<'a> handler::HandlerInit for HandlerInit<'a> {
    type Request = (v4::TransferRequest, IpAddr, Arc<DropConfig>);
    type Loop = HandlerLoop<'a>;
    type Pinger = tokio::time::Interval;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request> {
        let msg = ws
            .recv()
            .await
            .context("Failed to receive transfer request")?;

        let msg = msg.to_str().ok().context("Expected JSON message")?;
        debug!(self.logger, "Request received:\n\t{msg}");

        let req = serde_json::from_str(msg).context("Failed to deserialize transfer request")?;

        Ok((req, self.peer, self.state.config.clone()))
    }

    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()> {
        let msg = v4::ServerMsg::Error(v4::Error {
            file: None,
            msg: err.to_string(),
        });

        ws.send(Message::from(&msg))
            .await
            .context("Failed to send error message")?;
        Ok(())
    }

    async fn upgrade(
        mut self,
        ws: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        msg_tx: Sender<MsgToSend>,
        xfer: Arc<IncomingTransfer>,
    ) -> Option<Self::Loop> {
        let task = async {
            let checksums = self.state.storage.fetch_checksums(xfer.id()).await;

            let mut checksum_map = HashMap::new();
            let mut to_fetch = Vec::new();

            for (xfile, csum_bytes) in checksums.into_iter().filter_map(|csum| {
                let xfile = xfer.files().get(&csum.file_id)?;
                Some((xfile, csum.checksum))
            }) {
                let acell = checksum_map
                    .entry(xfile.id().clone())
                    .or_insert_with(AsyncCell::shared);

                match csum_bytes {
                    Some(csbytes) => acell.set(
                        csbytes
                            .try_into()
                            .ok()
                            .context("Invalid length checksum stored in the DB")?,
                    ),
                    None => to_fetch.push(xfile.id().clone()),
                }
            }

            Ok((to_fetch, checksum_map))
        };

        let (to_fetch, checksums) = match task.await {
            Ok(res) => res,
            Err(err) => {
                error!(self.logger, "Failed to prepare checksum info: {err}");

                if let Err(e) = self.on_error(ws, err).await {
                    warn!(self.logger, "Failed to send error message: {e}");
                }
                return None;
            }
        };

        let Self {
            peer: _,
            state,
            logger,
            alive,
        } = self;

        // task responsible for requesting the checksum
        let req_file_checksums = {
            let msg_tx = msg_tx.clone();
            let logger = logger.clone();
            let xfer = xfer.clone();
            let guard = alive.clone();

            async move {
                let _guard = guard;

                for xfile in to_fetch.into_iter().filter_map(|id| xfer.files().get(&id)) {
                    let msg = v4::ReqChsum {
                        file: xfile.id().clone(),
                        limit: xfile.size(),
                    };
                    let msg = v4::ServerMsg::ReqChsum(msg);
                    if let Err(err) = msg_tx.send((&msg).into()).await {
                        warn!(logger, "Failed to request checksum: {err}");
                    }
                }
            }
        };

        jobs.spawn(req_file_checksums);

        Some(HandlerLoop {
            state,
            msg_tx,
            xfer,
            jobs: HashMap::new(),
            logger,
            checksums,
        })
    }

    fn pinger(&mut self) -> Self::Pinger {
        tokio::time::interval(drop_config::PING_INTERVAL)
    }
}

impl HandlerLoop<'_> {
    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        chunk: Vec<u8>,
    ) -> anyhow::Result<()> {
        if let Some(task) = self.jobs.get(&file_id) {
            if let Err(err) = task.chunks_tx.send(chunk) {
                let msg = v4::Error {
                    msg: format!("Failed to consume chunk for file: {file_id:?}, msg: {err}",),
                    file: Some(file_id),
                };

                socket
                    .send(Message::from(&v4::ServerMsg::Error(msg)))
                    .await?;
            }
        }

        Ok(())
    }

    async fn on_cancel(&mut self, file_id: FileId) {
        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
            csum_tx: _,
        }) = self.jobs.remove(&file_id)
        {
            if !task.is_finished() {
                task.abort();
                events.pause().await;
            }
        }
    }

    async fn stop_task(&mut self, file_id: &FileId, status: Status) {
        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
            csum_tx: _,
        }) = self.jobs.remove(file_id)
        {
            if !task.is_finished() {
                debug!(
                    self.logger,
                    "Aborting download job: {}:{file_id}",
                    self.xfer.id()
                );

                task.abort();
                events.stop_silent(status).await;
            }
        }
    }

    async fn on_error(&mut self, file_id: Option<FileId>, msg: String) {
        error!(
            self.logger,
            "Client reported and error: file: {:?}, message: {}", file_id, msg
        );

        if let Some(file_id) = file_id {
            match self
                .state
                .transfer_manager
                .incoming_terminal_recv(self.xfer.id(), &file_id, FileTerminalState::Failed)
                .await
            {
                Err(err) => {
                    warn!(self.logger, "Failed to accept failure: {err}");
                }
                Ok(Some(res)) => {
                    res.events
                        .failed(crate::Error::BadTransferState(format!(
                            "Sender reported an error: {msg}"
                        )))
                        .await;
                }
                Ok(None) => (),
            }

            self.stop_task(&file_id, Status::BadTransferState).await;
        }
    }

    async fn on_checksum(&mut self, report: v4::ReportChsum) {
        let xfile = match self.xfer.files().get(&report.file) {
            Some(file) => file,
            None => return,
        };

        // Full checksum requsted at the begining of the transfer
        if report.limit == xfile.size() {
            self.checksums
                .get(&report.file)
                .expect("Missing file")
                .or_set(report.checksum);

            let storage = self.state.storage.clone();
            let transfer_id = self.xfer.id();
            let file_id = report.file.clone();

            tokio::spawn(async move {
                storage
                    .save_checksum(transfer_id, file_id.as_ref(), &report.checksum)
                    .await;
            });
        // Requests made by the download task
        } else if let Some(job) = self.jobs.get_mut(&report.file) {
            if job.csum_tx.send(report).await.is_err() {
                warn!(
                    self.logger,
                    "Failed to pass checksum report to receiver task"
                );
            }
        }
    }

    fn take_pause_futures(&mut self) -> impl Future<Output = ()> {
        let jobs = std::mem::take(&mut self.jobs);

        async move {
            let tasks = jobs.into_values().map(|task| async move {
                task.events.pause().await;
            });

            futures::future::join_all(tasks).await;
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn start_download(&mut self, ctx: super::FileStreamCtx<'_>) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(ctx.task.file.id())
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let full_csum_cell = self
            .checksums
            .get(ctx.task.file.id())
            .context("Missing file checksum cell")?
            .clone();

        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();
        let (csum_tx, csum_rx) = mpsc::channel(4);

        let downloader = Downloader {
            file_id: ctx.task.file.id().clone(),
            msg_tx: self.msg_tx.clone(),
            logger: self.logger.clone(),
            csum_rx,
            full_csum: full_csum_cell,
            offset: 0,
        };

        let file_id = ctx.task.file.id().clone();
        let (job, events) = ctx.start(downloader, chunks_rx).await?;

        self.jobs.insert(
            file_id,
            FileTask {
                job,
                chunks_tx,
                events,
                csum_tx,
            },
        );

        Ok(())
    }

    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = v4::ServerMsg::Cancel(v4::Cancel {
            file: file_id.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.stop_task(&file_id, Status::FileRejected).await;
        Ok(())
    }

    async fn issue_failure(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        msg: String,
    ) -> anyhow::Result<()> {
        let msg = v4::ServerMsg::Error(v4::Error {
            file: Some(file_id),
            msg,
        });
        socket.send(Message::from(&msg)).await?;

        Ok(())
    }

    async fn issue_done(&mut self, socket: &mut WebSocket, file_id: FileId) -> anyhow::Result<()> {
        let file = self.xfer.files().get(&file_id).context("Invalid file")?;

        let msg = v4::ServerMsg::Done(v4::Done {
            file: file_id,
            bytes_transfered: file.size(),
        });
        socket.send(Message::from(&msg)).await?;
        Ok(())
    }

    async fn issue_start(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        offset: u64,
    ) -> anyhow::Result<()> {
        let msg = v4::ServerMsg::Start(v4::Start {
            file: file_id.clone(),
            offset,
        });
        socket.send(Message::from(&msg)).await?;
        Ok(())
    }

    async fn on_close(&mut self) {
        debug!(self.logger, "ServerHandler::on_close(), stopping silently",);

        let tasks = self.jobs.drain().map(|(_, task)| async move {
            task.events.stop_silent(Status::Finalized).await;
        });

        futures::future::join_all(tasks).await;
    }

    async fn on_text_msg(&mut self, _: &mut WebSocket, text: &str) -> anyhow::Result<()> {
        let msg: v4::ClientMsg =
            serde_json::from_str(text).context("Failed to deserialize json")?;

        match msg {
            v4::ClientMsg::Error(v4::Error { file, msg }) => self.on_error(file, msg).await,
            v4::ClientMsg::Cancel(v4::Cancel { file }) => self.on_cancel(file).await,
            v4::ClientMsg::ReportChsum(report) => self.on_checksum(report).await,
        }

        Ok(())
    }

    async fn on_bin_msg(&mut self, ws: &mut WebSocket, bytes: Vec<u8>) -> anyhow::Result<()> {
        let v4::Chunk { file, data } =
            v4::Chunk::decode(bytes).context("Failed to decode file chunk")?;

        self.on_chunk(ws, file, data).await?;

        Ok(())
    }

    async fn finalize_success(mut self) {
        debug!(self.logger, "Finalizing");

        let files = self
            .state
            .storage
            .fetch_temp_locations(self.xfer.id())
            .await;

        super::remove_temp_files(
            self.logger,
            self.xfer.id(),
            files
                .into_iter()
                .map(|tmp| (tmp.base_path, FileId::from(tmp.file_id))),
        );
    }

    // While the destructor ensures the events are paused this function waits for
    // the execution to be finished
    async fn finalize_failure(mut self) {
        self.take_pause_futures().await;
    }
}

impl Drop for HandlerLoop<'_> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");
        tokio::spawn(self.take_pause_futures());
    }
}

impl Downloader {
    async fn send(&mut self, msg: impl Into<Message>) -> crate::Result<()> {
        self.msg_tx
            .send(msg.into().into())
            .await
            .map_err(|_| crate::Error::Canceled)
    }

    async fn request_csum(&mut self, limit: u64) -> crate::Result<v4::ReportChsum> {
        let msg = v4::ServerMsg::ReqChsum(v4::ReqChsum {
            file: self.file_id.clone(),
            limit,
        });
        self.send(Message::from(&msg)).await?;

        let report = self.csum_rx.recv().await.ok_or(crate::Error::Canceled)?;

        Ok(report)
    }
}

#[async_trait::async_trait]
impl handler::Downloader for Downloader {
    async fn init(
        &mut self,
        task: &super::FileXferTask,
        tmpstate: Option<TmpFileState>,
    ) -> crate::Result<handler::DownloadInit> {
        match tmpstate {
            Some(TmpFileState { meta, csum }) => {
                self.offset = match meta.len().cmp(&task.file.size()) {
                    Ordering::Less => {
                        let report = self.request_csum(meta.len()).await?;

                        if report.limit == meta.len() && report.checksum == csum {
                            // All matches, we can continue with temp file
                            meta.len()
                        } else {
                            info!(
                                self.logger,
                                "Found missmatch in partially downloaded file, overwriting"
                            );

                            0
                        }
                    }
                    Ordering::Equal => {
                        if self.full_csum.get().await == csum {
                            // All matches the temp file is actually the full file
                            meta.len()
                        } else {
                            info!(
                                self.logger,
                                "The partially downloaded file has the same size as the target \
                                 file but the checksum does not match, overwriting"
                            );

                            0
                        }
                    }
                    Ordering::Greater => {
                        info!(
                            self.logger,
                            "The partially downloaded file is bigger then the target file, \
                             overwriting"
                        );

                        0
                    }
                };

                Ok(handler::DownloadInit::Stream {
                    offset: self.offset,
                })
            }
            None => Ok(handler::DownloadInit::Stream { offset: 0 }),
        }
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
        self.send(&v4::ServerMsg::Progress(v4::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn validate<F, Fut>(
        &mut self,
        path: &Hidden<PathBuf>,
        progress_cb: Option<F>,
        event_granularity: Option<u64>,
    ) -> crate::Result<()>
    where
        F: FnMut(u64) -> Fut + Send + Sync,
        Fut: Future<Output = ()> + Send + Sync,
    {
        let file = std::fs::File::open(&path.0)?;
        let csum = file::checksum(file, progress_cb, event_granularity).await?;

        if self.full_csum.get().await != csum {
            return Err(crate::Error::ChecksumMismatch);
        }

        Ok(())
    }
}
