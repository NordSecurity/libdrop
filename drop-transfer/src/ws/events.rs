use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use drop_analytics::{Moose, TransferFileEventData, TransferStateEventData, MOOSE_STATUS_SUCCESS};
use drop_core::Status;
use tokio::sync::{mpsc::UnboundedSender, Mutex};

use crate::{
    file::FileInfo, utils, Event, File, FileId, IncomingTransfer, OutgoingTransfer, Transfer,
};

struct FileEventTxInner {
    tx: UnboundedSender<(Event, SystemTime)>,
    moose: Arc<dyn Moose>,
    state: FileState,
    transferred: u64,
}

enum FileState {
    Idle,
    Throttled,
    Preflight,
    InFlight { started: Instant },
    Terminal,
}

pub type IncomingFileEventTx = FileEventTx<IncomingTransfer>;
pub type OutgoingFileEventTx = FileEventTx<OutgoingTransfer>;

pub struct FileEventTx<T: Transfer> {
    inner: Mutex<FileEventTxInner>,
    xfer: Arc<T>,
    file_id: FileId,
}

pub struct EventTxFactory {
    events: UnboundedSender<(Event, SystemTime)>,
    moose: Arc<dyn Moose>,
}

pub struct TransferEventTx<T: Transfer> {
    inner: Mutex<TransferEventTxInner>,
    pub xfer: Arc<T>,
}

pub type IncomingTransferEventTx = TransferEventTx<IncomingTransfer>;
pub type OutgoingTransferEventTx = TransferEventTx<OutgoingTransfer>;

enum TransferState {
    Ongoing,
    Terminated,
}

struct TransferEventTxInner {
    tx: UnboundedSender<(Event, SystemTime)>,
    moose: Arc<dyn Moose>,
    state: TransferState,
}

trait EventTx {
    fn emit(&self, event: Event);
}

impl EventTx for UnboundedSender<(Event, SystemTime)> {
    fn emit(&self, event: Event) {
        // Sometimes on shutdown it can error out. It's better not to handle this error
        // at all
        let _ = self.send((event, SystemTime::now()));
    }
}

impl EventTxFactory {
    pub fn new(events: UnboundedSender<(Event, SystemTime)>, moose: Arc<dyn Moose>) -> Self {
        Self { events, moose }
    }

    pub fn file<T: Transfer>(&self, xfer: Arc<T>, file_id: FileId) -> FileEventTx<T> {
        FileEventTx {
            inner: Mutex::new(FileEventTxInner {
                tx: self.events.clone(),
                moose: self.moose.clone(),
                state: FileState::Idle,
                transferred: 0,
            }),
            xfer,
            file_id,
        }
    }

    pub fn transfer<T: Transfer>(&self, xfer: Arc<T>, blocked: bool) -> TransferEventTx<T> {
        TransferEventTx {
            inner: Mutex::new(TransferEventTxInner {
                tx: self.events.clone(),
                moose: self.moose.clone(),
                state: if blocked {
                    TransferState::Terminated
                } else {
                    TransferState::Ongoing
                },
            }),
            xfer,
        }
    }
}

impl<T: Transfer> FileEventTx<T> {
    fn file_info(&self) -> FileInfo {
        self.xfer.files()[&self.file_id].info()
    }

    async fn emit_in_flight(&self, event: Event) {
        let mut lock = self.inner.lock().await;

        if !(matches!(lock.state, FileState::Preflight { .. })
            || matches!(lock.state, FileState::InFlight { .. }))
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

        lock.tx.emit(event);
    }

    async fn start_inner(&self, events: impl IntoIterator<Item = Event>) {
        let mut lock = self.inner.lock().await;

        if matches!(lock.state, FileState::Terminal) {
            return;
        }

        lock.state = FileState::InFlight {
            started: Instant::now(),
        };

        for event in events.into_iter() {
            lock.tx.emit(event);
        }
    }

    async fn stop(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.lock().await;

        let elapsed = match std::mem::replace(&mut lock.state, FileState::Idle) {
            FileState::Idle => return,
            FileState::Throttled => Duration::ZERO,
            FileState::InFlight { started } => started.elapsed(),
            FileState::Preflight => Duration::ZERO,
            FileState::Terminal => return,
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

        lock.tx.emit(event);
    }

    async fn terminate(&self, event: Event, status: Result<(), i32>) {
        let mut lock = self.inner.lock().await;

        let elapsed = match std::mem::replace(&mut lock.state, FileState::Terminal) {
            FileState::Idle => Duration::ZERO,
            FileState::Throttled => Duration::ZERO,
            FileState::InFlight { started } => started.elapsed(),
            FileState::Preflight => Duration::ZERO,
            FileState::Terminal => return,
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

        lock.tx.emit(event);
    }

    pub async fn stop_silent(&self, status: Status) {
        let mut lock = self.inner.lock().await;

        let elapsed = match std::mem::replace(&mut lock.state, FileState::Idle) {
            FileState::Idle => None,
            FileState::Throttled => Some(Duration::ZERO),
            FileState::InFlight { started } => Some(started.elapsed()),
            FileState::Preflight => Some(Duration::ZERO),
            FileState::Terminal => return,
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
    pub async fn pending(&self, base_dir: impl Into<String>) {
        let lock = self.inner.lock().await;

        if !matches!(lock.state, FileState::Idle) {
            return;
        }

        lock.tx.emit(crate::Event::FileDownloadPending {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            base_dir: base_dir.into(),
        });
    }

    pub async fn finalize_checksum_start(&self, size: u64) {
        self.emit_in_flight(crate::Event::FinalizeChecksumStarted {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            size,
        })
        .await
    }

    pub async fn finalize_checksum_finish(&self) {
        self.emit_in_flight(crate::Event::FinalizeChecksumFinished {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
        })
        .await
    }

    pub async fn finalize_checksum_progress(&self, progress: u64) {
        self.emit_in_flight(crate::Event::FinalizeChecksumProgress {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            progress,
        })
        .await
    }

    pub async fn verify_checksum_start(&self, size: u64) {
        self.emit_in_flight(crate::Event::VerifyChecksumStarted {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
            size,
        })
        .await
    }

    pub async fn verify_checksum_finish(&self) {
        self.emit_in_flight(crate::Event::VerifyChecksumFinished {
            transfer_id: self.xfer.id(),
            file_id: self.file_id.clone(),
        })
        .await
    }

    pub async fn verify_checksum_progress(&self, progress: u64) {
        self.emit_in_flight(crate::Event::VerifyChecksumProgress {
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
        lock.state = FileState::Preflight;
    }

    pub async fn failed(&self, err: crate::Error) {
        let status = i32::from(&err);
        self.terminate(
            crate::Event::FileDownloadFailed(self.xfer.clone(), self.file_id.clone(), err),
            Err(status),
        )
        .await
    }

    pub async fn rejected(&self, by_peer: bool) {
        self.terminate(
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
        self.terminate(
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

    pub async fn throttled(&self, transferred: u64) {
        let mut lock = self.inner.lock().await;

        match lock.state {
            FileState::Idle => {
                lock.tx.emit(crate::Event::FileUploadThrottled {
                    transfer_id: self.xfer.id(),
                    file_id: self.file_id.clone(),
                    transferred,
                });

                lock.state = FileState::Throttled;
            }
            FileState::Throttled => (),
            FileState::InFlight { .. } => (),
            FileState::Preflight => (),
            FileState::Terminal => (),
        }
    }

    pub async fn failed(&self, err: crate::Error) {
        let status = i32::from(&err);
        self.terminate(
            crate::Event::FileUploadFailed(self.xfer.clone(), self.file_id.clone(), err),
            Err(status),
        )
        .await
    }

    pub async fn success(&self) {
        self.terminate(
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
        self.terminate(
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

impl<T: Transfer> TransferEventTx<T> {
    async fn emit_ongoing(&self, event: Event) {
        let lock = self.inner.lock().await;

        if let TransferState::Terminated = lock.state {
            return;
        }

        lock.tx.emit(event);
    }

    async fn stop(&self, event: Event) {
        let mut lock = self.inner.lock().await;

        if let TransferState::Terminated =
            std::mem::replace(&mut lock.state, TransferState::Terminated)
        {
            return;
        }

        lock.tx.emit(event);
    }
}

impl TransferEventTx<OutgoingTransfer> {
    pub async fn queued(&self) {
        self.emit_ongoing(Event::RequestQueued(self.xfer.clone()))
            .await;
    }

    pub async fn failed(&self, err: crate::Error, by_peer: bool) {
        let mut lock = self.inner.lock().await;

        if let TransferState::Terminated =
            std::mem::replace(&mut lock.state, TransferState::Terminated)
        {
            return;
        }

        lock.moose.event_transfer_state(TransferStateEventData {
            protocol_version: 0,
            transfer_id: self.xfer.id().to_string(),
            result: i32::from(&err),
        });

        lock.tx.emit(Event::OutgoingTransferFailed(
            self.xfer.clone(),
            err,
            by_peer,
        ));
    }

    pub async fn deferred(&self, err: crate::Error) {
        self.emit_ongoing(Event::OutgoingTransferDeferred {
            transfer: self.xfer.clone(),
            error: err,
        })
        .await;
    }

    pub async fn connected(&self, protocol_version: i32) {
        let lock = self.inner.lock().await;

        if let TransferState::Terminated = lock.state {
            return;
        }

        lock.moose.event_transfer_state(TransferStateEventData {
            protocol_version,
            transfer_id: self.xfer.id().to_string(),
            result: MOOSE_STATUS_SUCCESS,
        });
    }

    pub async fn cancel(&self, by_peer: bool) {
        self.stop(Event::OutgoingTransferCanceled(self.xfer.clone(), by_peer))
            .await;
    }
}

impl TransferEventTx<IncomingTransfer> {
    pub async fn received(&self) {
        self.emit_ongoing(Event::RequestReceived(self.xfer.clone()))
            .await;
    }

    pub async fn cancel(&self, by_peer: bool) {
        self.stop(Event::IncomingTransferCanceled(self.xfer.clone(), by_peer))
            .await;
    }
}

impl<T: Transfer> Drop for FileEventTx<T> {
    fn drop(&mut self) {
        let elapsed = match self.inner.get_mut().state {
            FileState::Idle => None,
            FileState::Throttled => Some(Duration::ZERO),
            FileState::InFlight { started } => Some(started.elapsed()),
            FileState::Preflight => Some(Duration::ZERO),
            FileState::Terminal => return,
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
                    result: Status::Finalized as _,
                });
        }
    }
}
