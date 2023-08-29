use std::{
    io,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub type WsStream = WebSocketStream<TcpStream>;

pub struct WebSocket {
    stream: WsStream,
    recv_last: Option<Instant>,
    recv_timeout: Duration,
    send_timeout: Duration,
}

impl WebSocket {
    pub fn new(stream: WsStream, recv_timeout: Duration, send_timeout: Duration) -> Self {
        Self {
            stream,
            recv_last: None,
            recv_timeout,
            send_timeout,
        }
    }

    pub async fn send(&mut self, msg: Message) -> crate::Result<()> {
        tokio::time::timeout(self.send_timeout, self.stream.send(msg))
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))??;

        Ok(())
    }

    pub async fn recv(&mut self) -> crate::Result<Message> {
        let timeout = self.recv_last.map_or(self.recv_timeout, |last| {
            self.recv_timeout.saturating_sub(last.elapsed())
        });

        let msg = tokio::time::timeout(timeout, self.stream.next())
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Emptry socket stream"))??;

        self.recv_last = Some(Instant::now());

        Ok(msg)
    }

    pub async fn close(&mut self) -> crate::Result<()> {
        self.send(Message::Close(None)).await
    }

    pub async fn drain(&mut self) -> crate::Result<()> {
        while self.stream.next().await.transpose()?.is_some() {}
        Ok(())
    }
}
