use std::{sync::Arc, time::Duration};

use tokio::{sync::mpsc::Sender, task::JoinSet};
use tokio_tungstenite::tungstenite::Message;

use super::WebSocket;
use crate::{ws, FileId, OutgoingTransfer};

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
    type Pinger: ws::Pinger;
    type Loop: HandlerLoop;

    async fn start(&mut self, socket: &mut WebSocket, xfer: &OutgoingTransfer)
        -> crate::Result<()>;

    fn upgrade(self, msg_tx: Sender<MsgToSend>, xfer: Arc<OutgoingTransfer>) -> Self::Loop;
    fn pinger(&mut self) -> Self::Pinger;
}

#[async_trait::async_trait]
pub trait HandlerLoop {
    async fn issue_reject(&mut self, ws: &mut WebSocket, file_id: FileId) -> anyhow::Result<()>;
    async fn issue_faliure(&mut self, ws: &mut WebSocket, file_id: FileId) -> anyhow::Result<()>;

    async fn on_close(&mut self, by_peer: bool);
    async fn on_text_msg(
        &mut self,
        ws: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        text: String,
    ) -> anyhow::Result<()>;
    async fn on_stop(&mut self);

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration>;
}

#[async_trait::async_trait]
pub trait Uploader: Send + 'static {
    async fn chunk(&mut self, chunk: &[u8]) -> crate::Result<()>;
    async fn error(&mut self, msg: String);

    // File stream offset
    fn offset(&self) -> u64;
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
