use std::{fs, ops::ControlFlow, path::PathBuf, time::Duration};

use tokio::sync::mpsc::Sender;
use warp::ws::{Message, WebSocket};

use super::ServerReq;
use crate::{utils::Hidden, ws};

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
        msg_tx: Sender<Message>,
        xfer: crate::Transfer,
    ) -> Option<Self::Loop>;

    fn pinger(&mut self) -> Self::Pinger;
}

#[async_trait::async_trait]
pub trait HandlerLoop {
    async fn on_req(&mut self, ws: &mut WebSocket, req: ServerReq) -> anyhow::Result<()>;
    async fn on_close(&mut self, by_peer: bool);
    async fn on_recv(
        &mut self,
        ws: &mut WebSocket,
        msg: Message,
    ) -> anyhow::Result<ControlFlow<()>>;
    async fn on_stop(&mut self);
    async fn finalize_failure(self, err: anyhow::Error);

    fn recv_timeout(&mut self) -> Option<Duration>;
}

pub trait Request {
    fn parse(self) -> anyhow::Result<crate::Transfer>;
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
