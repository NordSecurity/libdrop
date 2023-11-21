use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use tokio::{sync::mpsc::Sender, task::JoinSet};
use warp::ws::Message;

use super::socket::WebSocket;
use crate::{transfer::IncomingTransfer, utils::Hidden, ws, FileId};

pub struct MsgToSend {
    pub msg: Message,
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
    fn recv_timeout(&mut self) -> Duration {
        drop_config::TRANFER_IDLE_LIFETIME
    }
}

#[async_trait::async_trait]
pub trait HandlerLoop {
    async fn start_download(&mut self, ctx: super::FileStreamCtx<'_>) -> anyhow::Result<()>;
    async fn issue_start(
        &mut self,
        ws: &mut WebSocket,
        file: FileId,
        offset: u64,
    ) -> anyhow::Result<()>;
    async fn issue_reject(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;
    async fn issue_failure(
        &mut self,
        ws: &mut WebSocket,
        file: FileId,
        msg: String,
    ) -> anyhow::Result<()>;
    async fn issue_done(&mut self, ws: &mut WebSocket, file: FileId) -> anyhow::Result<()>;

    async fn on_close(&mut self);
    async fn on_text_msg(&mut self, ws: &mut WebSocket, text: &str) -> anyhow::Result<()>;
    async fn on_bin_msg(&mut self, ws: &mut WebSocket, bytes: Vec<u8>) -> anyhow::Result<()>;

    async fn finalize_success(self);
}

pub trait Request {
    fn parse(self) -> anyhow::Result<IncomingTransfer>;
}

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
    async fn validate(
        &mut self,
        location: &Hidden<PathBuf>,
        progress: Option<tokio::sync::watch::Sender<u64>>,
    ) -> crate::Result<()>;
}

impl<T> From<T> for MsgToSend
where
    T: Into<Message>,
{
    fn from(value: T) -> Self {
        Self { msg: value.into() }
    }
}
