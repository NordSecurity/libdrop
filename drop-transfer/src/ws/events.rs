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
    state: State,
    transferred: u64,
}

enum State {
    Idle,
    Throttled,
    Preflight,
    InFlight { started: Instant },
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
                state: State::Idle,
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

    async fn emit_in_flight(&self, event: Event) {
        let mut lock = self.inner.lock().await;

        if !(matches!(lock.state, State::Preflight { .. })
            || matches!(lock.state, State::InFlight { .. }))
        {
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

    async fn start_inner(&self, events: impl IntoIterator<Item = Event>) {
        let mut lock = self.inner.lock().await;
        lock.state = State::InFlight {
            started: Instant::now(),
        };

        for event in events.into_iter() {
            lock.emit_event(event);
        }
    }

    async fn stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.lock().await;

        let elapsed = match std::mem::replace(&mut lock.state, State::Idle) {
            State::Idle => return,
            State::Throttled => Duration::ZERO,
            State::InFlight { started } => started.elapsed(),
            State::Preflight => Duration::ZERO,
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

        let elapsed = match std::mem::replace(&mut lock.state, State::Idle) {
            State::Idle => Duration::ZERO,
            State::Throttled => Duration::ZERO,
            State::InFlight { started } => started.elapsed(),
            State::Preflight => Duration::ZERO,
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

        let elapsed = match std::mem::replace(&mut lock.state, State::Idle) {
            State::Idle => None,
            State::Throttled => Some(Duration::ZERO),
            State::InFlight { started } => Some(started.elapsed()),
            State::Preflight => Some(Duration::ZERO),
        };

        if let Some(elapsed) = elapsed {
            let file_info = self.file_info();

            lock.moose.event_transfer_file(TransferFileEventData {
                phase: drop_analytics::TransferFilePhase::Finished,
                transfer_id: self.xfer.id().to_string(),
                transfer_time: elapsed.as_millis() as i32,
                path_id: file_info.path_id,
                direction: file_info.direction,
                transferred: utils::to_kb(lock.transferred),
                result: status as _,
            });
        }
    }

    pub fn file_id(&self) -> &FileId {
        &self.file_id
    }
}

impl FileEventTx<IncomingTransfer> {
    pub async fn checksum_start(&self, size: u64) {
        self.emit_in_flight(crate::Event::ChecksumStarted {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            size,
        })
        .await
    }

    pub async fn checksum_finish(&self) {
        self.emit_in_flight(crate::Event::ChecksumFinished {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
        })
        .await
    }

    pub async fn checksum_progress(&self, progress: u64) {
        self.emit_in_flight(crate::Event::ChecksumProgress {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            progress,
        })
        .await
    }

    pub async fn progress(&self, transfered: u64) {
        self.emit_in_flight(crate::Event::FileDownloadProgress(
            self.xfer.clone(),
            self.file_id.clone(),
            transfered,
        ))
        .await
    }

    pub async fn start(&self, base_dir: impl Into<String>, offset: u64) {
        self.start_inner([crate::Event::FileDownloadStarted(
            self.xfer.clone(),
            self.file_id.clone(),
            base_dir.into(),
            offset,
        )])
        .await
    }

    pub async fn preflight(&self) {
        let mut lock = self.inner.lock().await;
        lock.state = State::Preflight;
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
        let events = [crate::Event::FileUploadStarted(
            self.xfer.clone(),
            self.file_id.clone(),
            offset,
        )];

        self.start_inner(events).await
    }

    pub async fn start_with_progress(&self, offset: u64) {
        let events = [
            crate::Event::FileUploadStarted(self.xfer.clone(), self.file_id.clone(), offset),
            crate::Event::FileUploadProgress(self.xfer.clone(), self.file_id.clone(), offset),
        ];

        self.start_inner(events).await
    }

    pub async fn progress(&self, transfered: u64) {
        self.emit_in_flight(crate::Event::FileUploadProgress(
            self.xfer.clone(),
            self.file_id.clone(),
            transfered,
        ))
        .await
    }

    pub async fn throttled(&self, transfered: u64) {
        let mut lock = self.inner.lock().await;

        match lock.state {
            State::Idle => {
                lock.emit_event(crate::Event::FileUploadThrottled {
                    transfer_id: self.xfer.id(),
                    file_id: self.file_id.clone(),
                    transfered,
                });

                lock.state = State::Throttled;
            }
            State::Throttled => (),
            State::InFlight { .. } => (),
            State::Preflight => (),
        }
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
        let elapsed = match self.inner.get_mut().state {
            State::Idle => None,
            State::Throttled => Some(Duration::ZERO),
            State::InFlight { started } => Some(started.elapsed()),
            State::Preflight => Some(Duration::ZERO),
        };

        if let Some(elapsed) = elapsed {
            let file_info = self.file_info();

            let transferred = self.inner.get_mut().transferred;

            self.inner
                .get_mut()
                .moose
                .event_transfer_file(TransferFileEventData {
                    phase: drop_analytics::TransferFilePhase::Finished,
                    transfer_id: self.xfer.id().to_string(),
                    transfer_time: elapsed.as_millis() as i32,
                    path_id: file_info.path_id,
                    direction: file_info.direction,
                    transferred: utils::to_kb(transferred),
                    result: Status::Canceled as _,
                });
        }
    }
}
