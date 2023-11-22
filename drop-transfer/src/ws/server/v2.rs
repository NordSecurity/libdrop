use std::{
    collections::HashMap,
    fmt::Write,
    fs,
    future::Future,
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Context;
use drop_config::DropConfig;
use drop_core::Status;
use sha1::Digest;
use slog::{debug, error, warn};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedSender},
    task::{AbortHandle, JoinSet},
};
use warp::ws::Message;

use super::{
    handler::{self, MsgToSend},
    socket::WebSocket,
};
use crate::{
    manager::FileTerminalState,
    protocol::v2,
    service::State,
    transfer::{IncomingTransfer, Transfer},
    utils::Hidden,
    ws::{self, events::FileEventTx},
    File, FileId,
};

pub struct HandlerInit<'a, const PING: bool = true> {
    peer: IpAddr,
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
}

pub struct HandlerLoop<'a, const PING: bool> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    msg_tx: Sender<MsgToSend>,
    xfer: Arc<IncomingTransfer>,
    jobs: HashMap<FileId, FileTask>,
}

struct Downloader {
    state: Arc<State>,
    file_id: FileId,
    msg_tx: Sender<MsgToSend>,
    tmp_loc: Option<Hidden<PathBuf>>,
    logger: slog::Logger,
}

struct FileTask {
    job: AbortHandle,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx<IncomingTransfer>>,
}

impl<'a, const PING: bool> HandlerInit<'a, PING> {
    pub(crate) fn new(peer: IpAddr, state: &'a Arc<State>, logger: &'a slog::Logger) -> Self {
        Self {
            peer,
            state,
            logger,
        }
    }
}

#[async_trait::async_trait]
impl<'a, const PING: bool> handler::HandlerInit for HandlerInit<'a, PING> {
    type Request = (v2::TransferRequest, IpAddr, Arc<DropConfig>);
    type Loop = HandlerLoop<'a, PING>;
    type Pinger = ws::utils::Pinger<PING>;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request> {
        let msg = ws
            .recv()
            .await
            .context("Failed to receive transfer request")?;

        let msg = msg.to_str().ok().context("Expected JSON message")?;

        let req = serde_json::from_str(msg).context("Failed to deserialize transfer request")?;

        Ok((req, self.peer, self.state.config.clone()))
    }

    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()> {
        let msg = v2::ServerMsgOnServer::Error(v2::Error {
            file: None,
            msg: err.to_string(),
        });

        ws.send(Message::from(&msg))
            .await
            .context("Failed to send error message")?;
        Ok(())
    }

    async fn upgrade(
        self,
        _: &mut WebSocket,
        _: &mut JoinSet<()>,
        msg_tx: Sender<MsgToSend>,
        xfer: Arc<IncomingTransfer>,
    ) -> Option<Self::Loop> {
        let Self {
            peer: _,
            state,
            logger,
        } = self;

        Some(HandlerLoop {
            state,
            msg_tx,
            xfer,
            jobs: HashMap::new(),
            logger,
        })
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
    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
        chunk: Vec<u8>,
    ) -> anyhow::Result<()> {
        if let Some(task) = self.jobs.get(&file_id) {
            if let Err(err) = task.chunks_tx.send(chunk) {
                let msg = v2::Error {
                    msg: format!("Failed to consue chunk for file: {file_id:?}, msg: {err}",),
                    file: Some(file_id),
                };

                socket
                    .send(Message::from(&v2::ServerMsgOnServer::Error(msg)))
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
        }) = self.jobs.remove(file_id)
        {
            if !task.is_finished() {
                debug!(
                    self.logger,
                    "Aborting download job: {}:{file_id:?}",
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

            self.stop_task(&file_id, Status::FileRejected).await;
        }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> handler::HandlerLoop for HandlerLoop<'_, PING> {
    async fn start_download(&mut self, ctx: super::FileStreamCtx<'_>) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(ctx.task.file.id())
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();

        let downloader = Downloader {
            state: self.state.clone(),
            file_id: ctx.task.file.id().clone(),
            msg_tx: self.msg_tx.clone(),
            tmp_loc: None,
            logger: self.logger.clone(),
        };
        let file_id = ctx.task.file.id().clone();
        let (job, events) = ctx.start(downloader, chunks_rx).await?;

        self.jobs.insert(
            file_id,
            FileTask {
                job,
                chunks_tx,
                events,
            },
        );

        Ok(())
    }

    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let msg = v2::ServerMsgOnServer::Cancel(v2::Download {
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
        let msg = v2::ServerMsgOnServer::Error(v2::Error {
            file: Some(file_id),
            msg,
        });
        socket.send(Message::from(&msg)).await?;

        Ok(())
    }

    async fn issue_done(&mut self, socket: &mut WebSocket, file_id: FileId) -> anyhow::Result<()> {
        let file = if let Some(file) = self.xfer.files().get(&file_id) {
            file
        } else {
            warn!(self.logger, "Missing file with ID: {file_id:?}");
            return Ok(());
        };

        let msg = v2::ServerMsgOnServer::Done(v2::Progress {
            bytes_transfered: file.size(),
            file: file_id,
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
        anyhow::ensure!(offset == 0, "V2 does not support non zero offset");

        let msg = v2::ServerMsgOnServer::Start(v2::Download { file: file_id });
        socket.send(Message::from(&msg)).await?;
        Ok(())
    }

    async fn on_close(&mut self) {
        debug!(self.logger, "ServerHandler::on_close(), stopping silently",);

        let tasks = self.jobs.drain().map(|(_, task)| async move {
            task.events.stop_silent(Status::Canceled).await;
        });

        futures::future::join_all(tasks).await;
    }

    async fn on_text_msg(&mut self, _: &mut WebSocket, text: &str) -> anyhow::Result<()> {
        let msg: v2::ClientMsgOnServer =
            serde_json::from_str(text).context("Failed to deserialize json")?;

        match msg {
            v2::ClientMsgOnServer::Error(v2::Error { file, msg }) => self.on_error(file, msg).await,
            v2::ClientMsgOnServer::Cancel(v2::Download { file }) => self.on_cancel(file).await,
        }

        Ok(())
    }

    async fn on_bin_msg(&mut self, ws: &mut WebSocket, bytes: Vec<u8>) -> anyhow::Result<()> {
        let v2::ChunkOnServer { file, data } =
            v2::ChunkOnServer::decode(bytes).context("Failed to decode file chunk")?;

        self.on_chunk(ws, file, data).await?;

        Ok(())
    }

    async fn finalize_success(mut self) {
        debug!(self.logger, "Finalizing");
    }
}

impl<const PING: bool> Drop for HandlerLoop<'_, PING> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");

        let jobs = std::mem::take(&mut self.jobs);
        tokio::spawn(async move {
            let tasks = jobs.into_values().map(|task| async move {
                task.events.pause().await;
            });

            futures::future::join_all(tasks).await;
        });
    }
}

impl Downloader {
    async fn send(&mut self, msg: impl Into<Message>) -> crate::Result<()> {
        self.msg_tx
            .send(msg.into().into())
            .await
            .map_err(|_| crate::Error::Canceled)
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        if let Some(path) = self.tmp_loc.as_ref() {
            if let Err(e) = fs::remove_file(&path.0) {
                warn!(self.logger, "Failed to remove tmp file: {e}");
            }
        }
    }
}

#[async_trait::async_trait]
impl handler::Downloader for Downloader {
    async fn init(&mut self, task: &super::FileXferTask) -> crate::Result<handler::DownloadInit> {
        let mut suffix = sha1::Sha1::new();

        suffix.update(task.xfer.id().as_bytes());
        if let Ok(time) = SystemTime::now().elapsed() {
            suffix.update(time.as_nanos().to_ne_bytes());
        }

        let suffix: String = suffix
            .finalize()
            .iter()
            .fold(String::new(), |mut output, b| {
                let _ = write!(output, "{b:02x}");
                output
            });

        let abs_path = task.prepare_abs_path(&self.state).await?;

        let tmp_location: Hidden<PathBuf> = Hidden(
            format!(
                "{}.dropdl-{}",
                abs_path.display(),
                suffix.get(..8).unwrap_or(&suffix),
            )
            .into(),
        );

        super::validate_tmp_location_path(&tmp_location)?;

        self.tmp_loc = Some(tmp_location.clone());
        Ok(handler::DownloadInit::Stream {
            offset: 0,
            tmp_location,
        })
    }

    async fn open(&mut self, path: &Hidden<PathBuf>) -> crate::Result<fs::File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = fs::File::create(&path.0)?;
        Ok(file)
    }

    async fn progress(&mut self, bytes: u64) -> crate::Result<()> {
        self.send(&v2::ServerMsgOnServer::Progress(v2::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn validate<F, Fut>(&mut self, _path: &Hidden<PathBuf>, _: Option<F>) -> crate::Result<()>
    where
        F: FnMut(u64) -> Fut + Send + Sync,
        Fut: Future<Output = ()>,
    {
        Ok(())
    }
}

impl handler::Request for (v2::TransferRequest, IpAddr, Arc<DropConfig>) {
    fn parse(self) -> anyhow::Result<IncomingTransfer> {
        self.try_into().context("Failed to parse transfer request")
    }
}
