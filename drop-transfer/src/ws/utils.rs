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

#[async_trait::async_trait]
impl super::Pinger for tokio::time::Interval {
    async fn tick(&mut self) {
        self.tick().await;
    }
}
