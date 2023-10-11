use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use drop_analytics::{Moose, TransferFileEventData, MOOSE_STATUS_SUCCESS};
use drop_core::Status;
use tokio::sync::{mpsc::UnboundedSender, Mutex};

use crate::{
    file::FileInfo, utils, Event, File, FileId, IncomingTransfer, OutgoingTransfer, Transfer,
};

struct FileEventTxInner {
    tx: UnboundedSender<(Event, SystemTime)>,
    moose: Arc<dyn Moose>,
    started: Option<Instant>,
    transferred: u64,
}

impl FileEventTxInner {
    fn emit_event(&self, event: Event) {
        self.tx
            .send((event, SystemTime::now()))
            .expect("Failed to emit File event");
    }
}

pub type IncomingFileEventTx = FileEventTx<IncomingTransfer>;
pub type OutgoingFileEventTx = FileEventTx<OutgoingTransfer>;

pub struct FileEventTx<T: Transfer> {
    inner: Mutex<FileEventTxInner>,
    xfer: Arc<T>,
    file_id: FileId,
}

pub struct FileEventTxFactory {
    events: UnboundedSender<(Event, SystemTime)>,
    moose: Arc<dyn Moose>,
}

impl FileEventTxFactory {
    pub fn new(events: UnboundedSender<(Event, SystemTime)>, moose: Arc<dyn Moose>) -> Self {
        Self { events, moose }
    }

    pub fn create<T: Transfer>(&self, xfer: Arc<T>, file_id: FileId) -> FileEventTx<T> {
        FileEventTx {
            inner: Mutex::new(FileEventTxInner {
                tx: self.events.clone(),
                moose: self.moose.clone(),
                started: None,
                transferred: 0,
            }),
            xfer,
            file_id,
        }
    }
}

impl<T: Transfer> FileEventTx<T> {
    fn file_info(&self) -> FileInfo {
        self.xfer.files()[&self.file_id].info()
    }

    async fn emit(&self, event: Event) {
        let mut lock = self.inner.lock().await;

        if lock.started.is_none() {
            return;
        }

        match event {
            Event::FileUploadProgress(_, _, progress)
            | Event::FileDownloadProgress(_, _, progress) => {
                lock.transferred = progress;
            }
            _ => {}
        }

        lock.emit_event(event);
    }

    async fn start_inner(&self, event: Event) {
        let mut lock = self.inner.lock().await;
        lock.started = Some(Instant::now());

        lock.emit_event(event);
    }

    async fn stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.lock().await;

        let elapsed = if let Some(started) = lock.started.take() {
            started.elapsed()
        } else {
            return;
        };

        let phase = match event {
            Event::FileUploadPaused { .. } | Event::FileDownloadPaused { .. } => {
                drop_analytics::TransferFilePhase::Paused
            }
            _ => drop_analytics::TransferFilePhase::Finished,
        };

        let result = match status {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(err) => err,
        };

        let file_info = self.file_info();

        lock.moose.event_transfer_file(TransferFileEventData {
            phase,
            transfer_id: self.xfer.id().to_string(),
            transfer_time: elapsed.as_millis() as i32,
            path_id: file_info.path_id,
            direction: file_info.direction,
            transferred: utils::to_kb(lock.transferred),
            result,
        });

        lock.emit_event(event);
    }

    async fn force_stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.lock().await;

        let elapsed = if let Some(started) = lock.started.take() {
            started.elapsed()
        } else {
            Duration::default()
        };

        let phase = match event {
            Event::FileUploadPaused { .. } | Event::FileDownloadPaused { .. } => {
                drop_analytics::TransferFilePhase::Paused
            }
            _ => drop_analytics::TransferFilePhase::Finished,
        };

        let result = match status {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(err) => err,
        };

        let file_info = self.file_info();

        lock.moose.event_transfer_file(TransferFileEventData {
            phase,
            transfer_id: self.xfer.id().to_string(),
            transfer_time: elapsed.as_millis() as i32,
            path_id: file_info.path_id,
            direction: file_info.direction,
            transferred: utils::to_kb(lock.transferred),
            result,
        });

        lock.emit_event(event);
    }

    pub async fn stop_silent(&self, status: Status) {
        let mut lock = self.inner.lock().await;

        if let Some(started) = lock.started.take() {
            let file_info = self.file_info();

            lock.moose.event_transfer_file(TransferFileEventData {
                phase: drop_analytics::TransferFilePhase::Finished,
                transfer_id: self.xfer.id().to_string(),
                transfer_time: started.elapsed().as_millis() as i32,
                path_id: file_info.path_id,
                direction: file_info.direction,
                transferred: utils::to_kb(lock.transferred),
                result: status as _,
            });
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
        self.force_stop(
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
            let file_info = self.file_info();

            let transferred = self.inner.get_mut().transferred;

            self.inner
                .get_mut()
                .moose
                .event_transfer_file(TransferFileEventData {
                    phase: drop_analytics::TransferFilePhase::Finished,
                    transfer_id: self.xfer.id().to_string(),
                    transfer_time: started.elapsed().as_millis() as i32,
                    path_id: file_info.path_id,
                    direction: file_info.direction,
                    transferred: utils::to_kb(transferred),
                    result: Status::Canceled as _,
                });
        }
    }
}
