use std::{ops::ControlFlow, sync::Arc, time::Duration};

use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::Message;

use super::{ClientReq, WebSocket};
use crate::{ws, OutgoingTransfer};

#[async_trait::async_trait]
pub trait HandlerInit {
    type Pinger: ws::Pinger;
    type Loop: HandlerLoop;

    async fn start(&mut self, socket: &mut WebSocket, xfer: &OutgoingTransfer)
        -> crate::Result<()>;

    fn upgrade(self, msg_tx: Sender<Message>, xfer: Arc<OutgoingTransfer>) -> Self::Loop;
    fn pinger(&mut self) -> Self::Pinger;
}

#[async_trait::async_trait]
pub trait HandlerLoop {
    async fn on_req(&mut self, ws: &mut WebSocket, req: ClientReq) -> anyhow::Result<()>;
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
pub trait Uploader: Send + 'static {
    async fn chunk(&mut self, chunk: &[u8]) -> crate::Result<()>;
    async fn error(&mut self, msg: String);

    // File stream offset
    fn offset(&self) -> u64;
}
