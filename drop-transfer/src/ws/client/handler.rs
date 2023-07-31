use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::Message;

use super::WebSocket;
use crate::{ws, FileId, OutgoingTransfer};

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
    async fn issue_reject(&mut self, ws: &mut WebSocket, file_id: FileId) -> anyhow::Result<()>;

    async fn on_close(&mut self, by_peer: bool);
    async fn on_text_msg(&mut self, ws: &mut WebSocket, text: String) -> anyhow::Result<()>;
    async fn on_stop(&mut self);
    async fn on_conn_break(&mut self);

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration>;
}

#[async_trait::async_trait]
pub trait Uploader: Send + 'static {
    async fn chunk(&mut self, chunk: &[u8]) -> crate::Result<()>;
    async fn error(&mut self, msg: String);

    // File stream offset
    fn offset(&self) -> u64;
}
