use std::{ops::ControlFlow, time::Duration};

use tokio::sync::mpsc::Sender;
use warp::ws::{Message, WebSocket};

use super::ServerReq;

#[async_trait::async_trait]
pub trait HandlerInit {
    type Request: Request;
    type Loop: HandlerLoop;
    type Pinger: Pinger;

    async fn recv_req(&mut self, ws: &mut WebSocket) -> anyhow::Result<Self::Request>;
    async fn on_error(&mut self, ws: &mut WebSocket, err: anyhow::Error) -> anyhow::Result<()>;

    fn upgrade(self, msg_tx: Sender<Message>, xfer: crate::Transfer) -> Self::Loop;
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

#[async_trait::async_trait]
pub trait Pinger {
    async fn tick(&mut self);
}

pub trait Request {
    fn parse(self) -> anyhow::Result<crate::Transfer>;
}

#[async_trait::async_trait]
pub trait FeedbackReport {
    async fn progress(&mut self, bytes: u64) -> Result<(), crate::Error>;
    async fn done(&mut self, bytes: u64) -> Result<(), crate::Error>;
    async fn error(&mut self, msg: String) -> Result<(), crate::Error>;
}
