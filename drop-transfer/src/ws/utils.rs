#[async_trait::async_trait]
impl super::Pinger for tokio::time::Interval {
    async fn tick(&mut self) {
        self.tick().await;
    }
}
