use std::{sync::Arc, time::Instant};

use drop_analytics::{FileInfo, Moose};
use drop_core::Status;
use tokio::sync::{mpsc::Sender, RwLock};
use uuid::Uuid;

use crate::{service::State, Event};

struct FileEventTxInner {
    tx: Sender<Event>,
    moose: Arc<dyn Moose>,
    xfer_id: Uuid,
    file_info: FileInfo,
    started: Option<Instant>,
}

pub struct FileEventTx {
    inner: RwLock<FileEventTxInner>,
}

impl FileEventTx {
    pub(crate) fn new(state: &State, transfer_id: Uuid, file_info: FileInfo) -> Self {
        Self {
            inner: RwLock::new(FileEventTxInner {
                tx: state.event_tx.clone(),
                moose: state.moose.clone(),
                xfer_id: transfer_id,
                file_info,
                started: None,
            }),
        }
    }

    pub async fn start(&self, event: Event) {
        let mut lock = self.inner.write().await;
        lock.started = Some(Instant::now());

        lock.moose.service_quality_transfer_file(
            Ok(()),
            drop_analytics::Phase::Start,
            lock.xfer_id.to_string(),
            0,
            Some(lock.file_info.clone()),
        );

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    pub async fn emit(&self, event: Event) {
        let lock = self.inner.read().await;

        if lock.started.is_none() {
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

    pub async fn stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.write().await;

        let elapsed = if let Some(started) = lock.started.take() {
            started.elapsed()
        } else {
            return;
        };

        lock.moose.service_quality_transfer_file(
            status,
            drop_analytics::Phase::End,
            lock.xfer_id.to_string(),
            elapsed.as_millis() as i32,
            Some(lock.file_info.clone()),
        );

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    pub async fn stop_silent(&self) {
        let mut lock = self.inner.write().await;
        lock.started = None;
    }
}

impl Drop for FileEventTxInner {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            self.moose.service_quality_transfer_file(
                Err(Status::Canceled as _),
                drop_analytics::Phase::End,
                self.xfer_id.to_string(),
                started.elapsed().as_millis() as _,
                Some(self.file_info.clone()),
            );
        }
    }
}
