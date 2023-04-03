use tokio::sync::{mpsc::Sender, RwLock};

use crate::{service::State, Event};

struct FileEventTxInner {
    running: bool,
    tx: Sender<Event>,
}

pub struct FileEventTx {
    inner: RwLock<FileEventTxInner>,
}

impl FileEventTx {
    pub(crate) fn new(state: &State) -> Self {
        Self {
            inner: RwLock::new(FileEventTxInner {
                running: false,
                tx: state.event_tx.clone(),
            }),
        }
    }

    pub async fn start(&self, event: Event) {
        let mut lock = self.inner.write().await;
        lock.running = true;

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    pub async fn emit(&self, event: Event) {
        let lock = self.inner.read().await;

        if !lock.running {
            return;
        }

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    /// Emits the event even when the file upload is not started
    pub async fn emit_force(&self, event: Event) {
        self.inner
            .read()
            .await
            .tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    pub async fn stop(&self, event: Event) {
        let mut lock = self.inner.write().await;

        if !lock.running {
            return;
        }

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");

        lock.running = false;
    }

    pub async fn stop_silent(&self) {
        let mut lock = self.inner.write().await;
        lock.running = false;
    }
}
