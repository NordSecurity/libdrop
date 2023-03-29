use std::{
    collections::HashMap,
    net::IpAddr,
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use drop_config::DropConfig;
use futures::{SinkExt, StreamExt};
use slog::{debug, error, warn};
use tokio::{
    sync::mpsc::{self, Sender, UnboundedSender},
    task::JoinHandle,
};
use warp::ws::{Message, WebSocket};

use super::{handler, ServerReq};
use crate::{protocol::v2, service::State, ws::events::FileEventTx, FileId};

pub struct HandlerInit<const PING: bool = true> {
    peer: IpAddr,
    state: Arc<State>,
    logger: slog::Logger,
}

pub struct HandlerLoop<const PING: bool> {
    state: Arc<State>,
    msg_tx: Sender<Message>,
    xfer: crate::Transfer,
    last_recv: Instant,
    jobs: HashMap<FileId, FileTask>,
    logger: slog::Logger,
}

pub struct Pinger<const PING: bool = true> {
    interval: tokio::time::Interval,
}

struct FeedbackReport {
    file_id: crate::FileId,
    msg_tx: Sender<Message>,
}

struct FileTask {
    job: JoinHandle<()>,
    chunks_tx: UnboundedSender<Vec<u8>>,
    events: Arc<FileEventTx>,
}

impl<const PING: bool> HandlerInit<PING> {
    pub(crate) fn new(peer: IpAddr, state: Arc<State>, logger: slog::Logger) -> Self {
        Self {
            peer,
            state,
            logger,
        }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> handler::HandlerInit for HandlerInit<PING> {
    type Request = (v2::TransferRequest, IpAddr, DropConfig);
    type Loop = HandlerLoop<PING>;
    type Pinger = Pinger<PING>;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request> {
        let msg = ws
            .next()
            .await
            .context("Did not received transfer request")?
            .context("Failed to receive transfer request")?;

        let msg = msg.to_str().ok().context("Expected JOSN message")?;

        let req = serde_json::from_str(msg).context("Failed to deserialize transfer request")?;

        Ok((req, self.peer, self.state.config))
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
        Pinger::<PING>::new(&self.state)
    }
}

impl<const PING: bool> HandlerLoop<PING> {
    async fn issue_download(
        &mut self,
        socket: &mut WebSocket,
        file: FileId,
        task: Box<super::FileXferTask>,
    ) -> anyhow::Result<()> {
        let is_running = self
            .jobs
            .get(&file)
            .map_or(false, |state| !state.job.is_finished());

        if is_running {
            return Ok(());
        }

        let msg = v2::ServerMsg::Start(v2::Download { file: file.clone() });
        socket.send(Message::from(&msg)).await?;

        let state = FileTask::start(
            self.msg_tx.clone(),
            self.state.clone(),
            file.clone(),
            task,
            self.logger.clone(),
        );

        self.jobs.insert(file, state);

        Ok(())
    }

    async fn issue_cancel(&mut self, socket: &mut WebSocket, file: FileId) -> anyhow::Result<()> {
        debug!(self.logger, "ServerHandler::issue_cancel");

        let msg = v2::ServerMsg::Cancel(v2::Download { file: file.clone() });
        socket.send(Message::from(&msg)).await?;

        self.on_cancel(file).await;

        Ok(())
    }

    async fn on_chunk(
        &mut self,
        socket: &mut WebSocket,
        file: FileId,
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

    async fn on_cancel(&mut self, file: FileId) {
        if let Some(FileTask {
            job: task,
            events,
            chunks_tx: _,
        }) = self.jobs.remove(&file)
        {
            if !task.is_finished() {
                task.abort();

                self.state.moose.service_quality_transfer_file(
                    Err(u32::from(&crate::Error::Canceled) as i32),
                    drop_analytics::Phase::End,
                    self.xfer.id().to_string(),
                    0,
                    self.xfer
                        .file(&file)
                        .expect("File should exists since we have a transfer task running")
                        .info(),
                );

                events
                    .stop(crate::Event::FileDownloadCancelled(self.xfer.clone(), file))
                    .await;
            }
        }
    }

    async fn on_error(&mut self, file: Option<FileId>, msg: String) {
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
                    task.abort();

                    events
                        .stop(crate::Event::FileDownloadFailed(
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
impl<const PING: bool> handler::HandlerLoop for HandlerLoop<PING> {
    async fn on_req(&mut self, ws: &mut WebSocket, req: ServerReq) -> anyhow::Result<()> {
        match req {
            ServerReq::Download { file, task } => self.issue_download(ws, file, task).await?,
            ServerReq::Cancel { file } => self.issue_cancel(ws, file).await?,
        }

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ServerHandler::on_close(by_peer: {})", by_peer);

        self.xfer
            .flat_file_list()
            .iter()
            .filter(|(file_id, _)| {
                self.jobs
                    .get(file_id)
                    .map_or(false, |state| !state.job.is_finished())
            })
            .for_each(|(_, file)| {
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
            let msg: v2::ClientMsg =
                serde_json::from_str(json).context("Failed to deserialize json")?;

            match msg {
                v2::ClientMsg::Error(v2::Error { file, msg }) => self.on_error(file, msg).await,
                v2::ClientMsg::Cancel(v2::Download { file }) => self.on_cancel(file).await,
            }
        } else if msg.is_binary() {
            let v2::Chunk { file, data } =
                v2::Chunk::decode(msg.into_bytes()).context("Failed to decode file chunk")?;

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
}

impl<const PING: bool> Drop for HandlerLoop<PING> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping server handler");
        self.jobs.values().for_each(|task| task.job.abort());
    }
}

impl<const PING: bool> Pinger<PING> {
    fn new(state: &State) -> Self {
        let interval = tokio::time::interval(state.config.transfer_idle_lifetime / 2);
        Self { interval }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> handler::Pinger for Pinger<PING> {
    async fn tick(&mut self) {
        if PING {
            self.interval.tick().await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}

impl FeedbackReport {
    async fn send(&mut self, msg: impl Into<Message>) -> crate::Result<()> {
        self.msg_tx
            .send(msg.into())
            .await
            .map_err(|_| crate::Error::Canceled)
    }
}

#[async_trait::async_trait]
impl handler::FeedbackReport for FeedbackReport {
    async fn progress(&mut self, bytes: u64) -> Result<(), crate::Error> {
        self.send(&v2::ServerMsg::Progress(v2::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn done(&mut self, bytes: u64) -> Result<(), crate::Error> {
        self.send(&v2::ServerMsg::Done(v2::Progress {
            file: self.file_id.clone(),
            bytes_transfered: bytes,
        }))
        .await
    }

    async fn error(&mut self, msg: String) -> Result<(), crate::Error> {
        self.send(&v2::ServerMsg::Error(v2::Error {
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
        file_id: FileId,
        task: Box<super::FileXferTask>,
        logger: slog::Logger,
    ) -> Self {
        let events = Arc::new(FileEventTx::new(&state));
        let (chunks_tx, chunks_rx) = mpsc::unbounded_channel();

        let job = tokio::spawn(task.run(
            state,
            Arc::clone(&events),
            FeedbackReport {
                file_id: file_id.clone(),
                msg_tx,
            },
            chunks_rx,
            file_id,
            logger,
        ));

        Self {
            job,
            chunks_tx,
            events,
        }
    }
}

impl handler::Request for (v2::TransferRequest, IpAddr, DropConfig) {
    fn parse(self) -> anyhow::Result<crate::Transfer> {
        self.try_into().context("Failed to parse transfer request")
    }
}
