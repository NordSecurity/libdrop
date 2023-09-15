use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use drop_analytics::{FileInfo, Moose};
use drop_core::Status;
use tokio::sync::{mpsc::Sender, RwLock};

use crate::{Event, File, FileId, IncomingTransfer, OutgoingTransfer, Transfer};

struct FileEventTxInner {
    tx: Sender<Event>,
    moose: Arc<dyn Moose>,
    started: Option<Instant>,
}

pub type IncomingFileEventTx = FileEventTx<IncomingTransfer>;
pub type OutgoingFileEventTx = FileEventTx<OutgoingTransfer>;

pub struct FileEventTx<T: Transfer> {
    inner: RwLock<FileEventTxInner>,
    xfer: Arc<T>,
    file_id: FileId,
}

pub struct FileEventTxFactory {
    events: Sender<Event>,
    moose: Arc<dyn Moose>,
}

impl FileEventTxFactory {
    pub fn new(events: Sender<Event>, moose: Arc<dyn Moose>) -> Self {
        Self { events, moose }
    }

    pub fn create<T: Transfer>(&self, xfer: Arc<T>, file_id: FileId) -> FileEventTx<T> {
        FileEventTx {
            inner: RwLock::new(FileEventTxInner {
                tx: self.events.clone(),
                moose: self.moose.clone(),
                started: None,
            }),
            xfer,
            file_id,
        }
    }
}

impl<T: Transfer> FileEventTx<T> {
    fn file_info(&self) -> Option<FileInfo> {
        Some(self.xfer.files()[&self.file_id].info())
    }

    async fn emit(&self, event: Event) {
        let lock = self.inner.read().await;

        if lock.started.is_none() {
            return;
        }

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    async fn start_inner(&self, event: Event) {
        let mut lock = self.inner.write().await;
        lock.started = Some(Instant::now());

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    async fn stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.write().await;

        let elapsed = if let Some(started) = lock.started.take() {
            started.elapsed()
        } else {
            return;
        };

        lock.moose.service_quality_transfer_file(
            status,
            self.xfer.id().to_string(),
            elapsed.as_millis() as i32,
            T::direction(),
            self.file_info(),
        );

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    async fn force_stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.write().await;

        let elapsed = if let Some(started) = lock.started.take() {
            started.elapsed()
        } else {
            Duration::default()
        };

        lock.moose.service_quality_transfer_file(
            status,
            self.xfer.id().to_string(),
            elapsed.as_millis() as i32,
            T::direction(),
            self.file_info(),
        );

        lock.tx
            .send(event)
            .await
            .expect("Event channel shouldn't be closed");
    }

    pub async fn stop_silent(&self, status: Status) {
        let mut lock = self.inner.write().await;

        if let Some(started) = lock.started.take() {
            let info = self.file_info();

            lock.moose.service_quality_transfer_file(
                Err(status as _),
                self.xfer.id().to_string(),
                started.elapsed().as_millis() as _,
                T::direction(),
                info,
            );
        }
    }
}

impl FileEventTx<IncomingTransfer> {
    pub async fn progress(&self, transfered: u64) {
        self.emit(crate::Event::FileDownloadProgress(
            self.xfer.clone(),
            self.file_id.clone(),
            transfered,
        ))
        .await
    }

    pub async fn start(&self, base_dir: impl Into<String>, offset: u64) {
        self.start_inner(crate::Event::FileDownloadStarted(
            self.xfer.clone(),
            self.file_id.clone(),
            base_dir.into(),
            offset,
        ))
        .await
    }

    pub async fn failed(&self, err: crate::Error) {
        let status = i32::from(&err);
        self.force_stop(
            crate::Event::FileDownloadFailed(self.xfer.clone(), self.file_id.clone(), err),
            Err(status),
        )
        .await
    }

    pub async fn rejected(&self, by_peer: bool) {
        self.force_stop(
            crate::Event::FileDownloadRejected {
                transfer_id: self.xfer.id(),
                file_id: self.file_id.clone(),
                by_peer,
            },
            Err(Status::FileRejected as _),
        )
        .await
    }

    pub async fn success(&self, final_path: impl Into<PathBuf>) {
        self.stop(
            crate::Event::FileDownloadSuccess(
                self.xfer.clone(),
                crate::event::DownloadSuccess {
                    id: self.file_id.clone(),
                    final_path: crate::utils::Hidden(final_path.into().into_boxed_path()),
                },
            ),
            Ok(()),
        )
        .await
    }

    pub async fn pause(&self) {
        self.stop(
            crate::Event::FileDownloadPaused {
                transfer_id: self.xfer.id(),
                file_id: self.file_id.clone(),
            },
            Ok(()),
        )
        .await
    }
}

impl FileEventTx<OutgoingTransfer> {
    pub async fn start(&self, offset: u64) {
        self.start_inner(crate::Event::FileUploadStarted(
            self.xfer.clone(),
            self.file_id.clone(),
            offset,
        ))
        .await
    }

    pub async fn progress(&self, transfered: u64) {
        self.emit(crate::Event::FileUploadProgress(
            self.xfer.clone(),
            self.file_id.clone(),
            transfered,
        ))
        .await
    }

    pub async fn failed(&self, err: crate::Error) {
        let status = i32::from(&err);
        self.force_stop(
            crate::Event::FileUploadFailed(self.xfer.clone(), self.file_id.clone(), err),
            Err(status),
        )
        .await
    }

    pub async fn success(&self) {
        self.force_stop(
            crate::Event::FileUploadSuccess(self.xfer.clone(), self.file_id.clone()),
            Ok(()),
        )
        .await
    }

    pub async fn pause(&self) {
        self.stop(
            crate::Event::FileUploadPaused {
                transfer_id: self.xfer.id(),
                file_id: self.file_id.clone(),
            },
            Ok(()),
        )
        .await
    }

    pub async fn rejected(&self, by_peer: bool) {
        self.force_stop(
            crate::Event::FileUploadRejected {
                transfer_id: self.xfer.id(),
                file_id: self.file_id.clone(),
                by_peer,
            },
            Err(Status::FileRejected as _),
        )
        .await
    }
}

impl<T: Transfer> Drop for FileEventTx<T> {
    fn drop(&mut self) {
        if let Some(started) = self.inner.get_mut().started {
            let info = self.file_info();

            self.inner.get_mut().moose.service_quality_transfer_file(
                Err(Status::Canceled as _),
                self.xfer.id().to_string(),
                started.elapsed().as_millis() as _,
                T::direction(),
                info,
            );
        }
    }
}
