use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AliveGuard(mpsc::Sender<()>);

pub struct AliveWaiter(AliveGuard, mpsc::Receiver<()>);

impl AliveWaiter {
    pub fn new() -> Self {
        let (send, recv) = mpsc::channel(1);
        Self(AliveGuard(send), recv)
    }

    pub fn guard(&self) -> AliveGuard {
        self.0.clone()
    }

    pub async fn wait_for_all(self) {
        // Drop the sender and wait for the receiver to get the notification about last
        // sender being dropped. Based on <https://tokio.rs/tokio/topics/shutdown>

        let Self(guard, mut recv) = self;
        drop(guard);

        let _ = recv.recv().await;
    }
}
