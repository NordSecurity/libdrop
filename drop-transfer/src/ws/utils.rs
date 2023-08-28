use std::time::Duration;

use anyhow::Context;
use futures::StreamExt;

pub struct Pinger<const PING: bool = true> {
    interval: tokio::time::Interval,
}

impl<const PING: bool> Pinger<PING> {
    pub(crate) fn new() -> Self {
        let interval = tokio::time::interval(drop_config::PING_INTERVAL);
        Self { interval }
    }
}

#[async_trait::async_trait]
impl<const PING: bool> super::Pinger for Pinger<PING> {
    async fn tick(&mut self) {
        if PING {
            self.interval.tick().await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}

pub async fn recv<S, M, E>(stream: &mut S, timeout: Option<Duration>) -> anyhow::Result<Option<M>>
where
    S: StreamExt<Item = Result<M, E>> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    let msg = if let Some(timeout) = timeout {
        tokio::time::timeout(timeout, stream.next())
            .await
            .context("Receive timeout")?
            .transpose()?
    } else {
        stream.next().await.transpose()?
    };

    Ok(msg)
}

#[async_trait::async_trait]
impl super::Pinger for tokio::time::Interval {
    async fn tick(&mut self) {
        self.tick().await;
    }
}
