use std::time::Duration;

use futures::StreamExt;

use crate::service::State;

pub struct Pinger<const PING: bool = true> {
    interval: tokio::time::Interval,
}

impl<const PING: bool> Pinger<PING> {
    pub(crate) fn new(state: &State) -> Self {
        let interval = tokio::time::interval(state.config.transfer_idle_lifetime / 2);
        Self { interval }
    }

    pub async fn tick(&mut self) {
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
            .map_err(|_| crate::Error::TransferTimeout)?
            .transpose()?
    } else {
        stream.next().await.transpose()?
    };

    Ok(msg)
}
