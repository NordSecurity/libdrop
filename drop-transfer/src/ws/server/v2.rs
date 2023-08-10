use std::{
    collections::HashMap,
    fs,
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Context;
use drop_config::DropConfig;
use futures::{SinkExt, StreamExt};
use sha1::Digest;
use slog::{debug, error, warn};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedSender},
    task::{AbortHandle, JoinSet},
};
use warp::ws::{Message, WebSocket};

use super::handler;
use crate::{
    file::FileSubPath,
    protocol::v2,
    service::State,
    tasks::AliveGuard,
    transfer::{IncomingTransfer, Transfer},
    utils::Hidden,
    ws::{self, events::FileEventTx},
    File, FileId,
};

pub struct HandlerInit<'a, const PING: bool = true> {
    peer: IpAddr,
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
}

pub struct HandlerLoop<'a, const PING: bool> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
    msg_tx: Sender<Message>,
    xfer: Arc<IncomingTransfer>,
    jobs: HashMap<FileSubPath, FileTask>,
}

struct Downloader {
    state: Arc<State>,
    file_id: FileSubPath,
    msg_tx: Sender<Message>,
    tmp_loc: Option<Hidden<PathBuf>>,
}

struct FileTask {
    job: AbortHandle,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx<IncomingTransfer>>,
}

impl<'a, const PING: bool> HandlerInit<'a, PING> {
    pub(crate) fn new(
        peer: IpAddr,
        state: &'a Arc<State>,
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
impl<'a, const PING: bool> handler::HandlerInit for HandlerInit<'a, PING> {
    type Request = (v2::TransferRequest, IpAddr, Arc<DropConfig>);
    type Loop = HandlerLoop<'a, PING>;
    type Pinger = ws::utils::Pinger<PING>;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request> {
        let msg = ws
            .next()
            .await
            .context("Did not received transfer request")?
            .context("Failed to receive transfer request")?;

        let msg = msg.to_str().ok().context("Expected JOSN message")?;

        let req = serde_json::from_str(msg).context("Failed to deserialize transfer request")?;

        Ok((req, self.peer, self.state.config.clone()))
    }

    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()> {
        let msg = v2::ServerMsg::Error(v2::Error {
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
        msg_tx: Sender<Message>,
        xfer: Arc<IncomingTransfer>,
    ) -> Option<Self::Loop> {
        let Self {
            peer: _,
            state,
            logger,
            alive,
        } = self;

        Some(HandlerLoop {
            state,
            msg_tx,
            xfer,
            jobs: HashMap::new(),
            logger,
            alive,
        })
    }

    fn pinger(&mut self) -> Self::Pinger {
        ws::utils::Pinger::<PING>::new(self.state)
    }
}

impl<const PING: bool> HandlerLoop<'_, PING> {
    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file: FileSubPath,
        chunk: Vec<u8>,
    ) -> anyhow::Result<()> {
        if let Some(task) = self.jobs.get(&file) {
            if let Err(err) = task.chunks_tx.send(chunk) {
                let msg = v2::Error {
                    msg: format!("Failed to consue chunk for file: {file:?}, msg: {err}",),
                    file: Some(file),
                };

                socket
                    .send(Message::from(&v2::ServerMsg::Error(msg)))
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
        }) = self.jobs.remove(&file)
        {
            if !task.is_finished() {
                task.abort();
                if by_peer {
                    events.cancelled(by_peer).await;
                }
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
            }) = self.jobs.remove(&file)
            {
                if !task.is_finished() {
                    let file_id = self
                        .xfer
                        .file_by_subpath(&file)
                        .expect("File should be there sice we have a task registered")
                        .id();

                    debug!(
                        self.logger,
                        "Aborting download job: {}:{file_id}",
                        self.xfer.id()
                    );

                    task.abort();

                    if let Err(err) = self
                        .state
                        .transfer_manager
                        .incoming_finsh_download(self.xfer.id(), file_id)
                        .await
                    {
                        warn!(self.logger, "Failed to store download finish: {err}");
                    }

                    events
                        .failed(crate::Error::BadTransferState(format!(
                            "Sender reported an error: {msg}"
                        )))
                        .await;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> handler::HandlerLoop for HandlerLoop<'_, PING> {
    async fn issue_download(
        &mut self,
        _: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        task: super::FileXferTask,
    ) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(task.file.subpath())
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let subpath = task.file.subpath().clone();
        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();

        let downloader = Downloader {
            state: self.state.clone(),
            file_id: task.file.subpath().clone(),
            msg_tx: self.msg_tx.clone(),
            tmp_loc: None,
        };
        let (job, events) = super::start_download(
            jobs,
            self.alive.clone(),
            self.state.clone(),
            task,
            downloader,
            chunks_rx,
            self.logger.clone(),
        )
        .await?;

        self.jobs.insert(
            subpath,
            FileTask {
                job,
                chunks_tx,
                events,
            },
        );

        Ok(())
    }

    async fn issue_cancel(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let file_subpath = if let Some(file) = self.xfer.files().get(&file_id) {
            file.subpath().clone()
        } else {
            warn!(self.logger, "Missing file with ID: {file_id:?}");
            return Ok(());
        };

        let msg = v2::ServerMsg::Cancel(v2::Download {
            file: file_subpath.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file_subpath, false).await;

        Ok(())
    }

    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let file_subpath = if let Some(file) = self.xfer.files().get(&file_id) {
            file.subpath().clone()
        } else {
            warn!(self.logger, "Missing file with ID: {file_id:?}");
            return Ok(());
        };

        let msg = v2::ServerMsg::Cancel(v2::Download {
            file: file_subpath.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.state
            .transfer_manager
            .incoming_rejection_ack(self.xfer.id(), &file_id)
            .await?;

        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
        }) = self.jobs.remove(&file_subpath)
        {
            if !task.is_finished() {
                task.abort();
                events.cancelled_on_rejection().await;
            }
        }

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ServerHandler::on_close(by_peer: {})", by_peer);

        self.on_stop().await;

        if by_peer {
            self.state
                .event_tx
                .send(crate::Event::IncomingTransferCanceled(
                    self.xfer.clone(),
                    by_peer,
                ))
                .await
                .expect("Could not send a file cancelled event, channel closed");
        }
    }

    async fn on_text_msg(&mut self, _: &mut WebSocket, text: &str) -> anyhow::Result<()> {
        let msg: v2::ClientMsg =
            serde_json::from_str(text).context("Failed to deserialize json")?;

        match msg {
            v2::ClientMsg::Error(v2::Error { file, msg }) => self.on_error(file, msg).await,
            v2::ClientMsg::Cancel(v2::Download { file }) => self.on_cancel(file, true).await,
        }

        Ok(())
    }

    async fn on_bin_msg(&mut self, ws: &mut WebSocket, bytes: Vec<u8>) -> anyhow::Result<()> {
        let v2::Chunk { file, data } =
            v2::Chunk::decode(bytes).context("Failed to decode file chunk")?;

        self.on_chunk(ws, file, data).await?;

        Ok(())
    }

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.jobs.drain().map(|(_, task)| async move {
            task.events.cancel_silent().await;
        });

        futures::future::join_all(tasks).await;
    }

    async fn finalize_success(self) {}

    async fn on_conn_break(&mut self) {
        debug!(self.logger, "Waiting for background jobs to pause");

        let tasks = self.jobs.drain().map(|(_, task)| async move {
            task.events.pause().await;
        });

        futures::future::join_all(tasks).await;
    }

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration> {
        if PING {
            Some(
                self.state
                    .config
                    .transfer_idle_lifetime
                    .saturating_sub(last_recv_elapsed),
            )
        } else {
            None
        }
    }
}

impl<const PING: bool> Drop for HandlerLoop<'_, PING> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");
    }
}

impl Downloader {
    async fn send(&mut self, msg: impl Into<Message>) -> crate::Result<()> {
        self.msg_tx
            .send(msg.into())
            .await
            .map_err(|_| crate::Error::Canceled)
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        if let Some(path) = self.tmp_loc.as_ref() {
            let _ = fs::remove_file(&path.0);
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
            .map(|b| format!("{:02x}", b))
            .collect();

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

        let msg = v2::ServerMsg::Start(v2::Download {
            file: self.file_id.clone(),
        });
        self.send(Message::from(&msg)).await?;

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
        self.send(&v2::ServerMsg::Progress(v2::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn done(&mut self, bytes: u64) -> crate::Result<()> {
        self.send(&v2::ServerMsg::Done(v2::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn error(&mut self, msg: String) -> crate::Result<()> {
        self.send(&v2::ServerMsg::Error(v2::Error {
            file: Some(self.file_id.clone()),
            msg,
        }))
        .await
    }

    async fn validate(&mut self, _: &Hidden<PathBuf>) -> crate::Result<()> {
        Ok(())
    }
}

impl handler::Request for (v2::TransferRequest, IpAddr, Arc<DropConfig>) {
    fn parse(self) -> anyhow::Result<IncomingTransfer> {
        self.try_into().context("Failed to parse transfer request")
    }
}
