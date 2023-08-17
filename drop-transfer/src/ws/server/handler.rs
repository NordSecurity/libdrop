use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use tokio::{sync::mpsc::Sender, task::JoinSet};
use warp::ws::{Message, WebSocket};

use crate::{transfer::IncomingTransfer, utils::Hidden, ws, FileId};

#[derive(Debug)]
pub enum Ack {
    Finished(FileId),
}

pub struct MsgToSend {
    pub msg: Message,
    pub ack: Option<Ack>,
}

#[async_trait::async_trait]
pub trait HandlerInit {
    type Request: Request;
    type Loop: HandlerLoop;
    type Pinger: ws::Pinger;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request>;
    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()>;
    async fn upgrade(
        self,
        ws: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        msg_tx: Sender<MsgToSend>,
        xfer: Arc<IncomingTransfer>,
    ) -> Option<Self::Loop>;

    fn pinger(&mut self) -> Self::Pinger;
}

#[async_trait::async_trait]
pub trait HandlerLoop {
    async fn issue_download(
        &mut self,
        ws: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        task: super::FileXferTask,
    ) -> anyhow::Result<()>;
    async fn issue_cancel(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;
    async fn issue_reject(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;
    async fn issue_failure(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;
    async fn issue_done(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;

    async fn on_close(&mut self, by_peer: bool);
    async fn on_text_msg(&mut self, ws: &mut WebSocket, text: &str) -> anyhow::Result<()>;
    async fn on_bin_msg(&mut self, ws: &mut WebSocket, bytes: Vec<u8>) -> anyhow::Result<()>;

    async fn finalize_success(self);

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration>;
}

pub trait Request {
    fn parse(self) -> anyhow::Result<IncomingTransfer>;
}

#[derive(Debug, Clone)]
pub enum DownloadInit {
    Stream {
        offset: u64,
        tmp_location: Hidden<PathBuf>,
    },
}

#[async_trait::async_trait]
pub trait Downloader {
    async fn init(&mut self, task: &super::FileXferTask) -> crate::Result<DownloadInit>;
    async fn open(&mut self, tmp_location: &Hidden<PathBuf>) -> crate::Result<fs::File>;
    async fn progress(&mut self, bytes: u64) -> crate::Result<()>;
    async fn done(&mut self, bytes: u64) -> crate::Result<()>;
    async fn error(&mut self, msg: String) -> crate::Result<()>;
    async fn validate(&mut self, location: &Hidden<PathBuf>) -> crate::Result<()>;
}

impl<T> From<T> for MsgToSend
where
    T: Into<Message>,
{
    fn from(value: T) -> Self {
        Self {
            msg: value.into(),
            ack: None,
        }
    }
}
