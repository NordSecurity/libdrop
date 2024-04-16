use std::time::SystemTime;

use drop_transfer::{File as _, Transfer};

pub struct File {
    pub id: String,
    pub path: String,
    pub size: u64,
}

pub struct Status {
    pub status: crate::StatusCode,
    pub os_error_code: Option<i32>,
}

pub struct Event {
    pub timestamp: i64,
    pub kind: EventKind,
}

pub enum EventKind {
    RequestReceived {
        peer: String,
        transfer_id: String,
        files: Vec<File>,
    },
    RequestQueued {
        peer: String,
        transfer_id: String,
        files: Vec<File>,
    },

    FileStarted {
        transfer_id: String,
        file_id: String,
        transfered: u64,
    },
    FileProgress {
        transfer_id: String,
        file_id: String,
        transfered: u64,
    },
    FileDownloaded {
        transfer_id: String,
        file_id: String,
        final_path: String,
    },
    FileUploaded {
        transfer_id: String,
        file_id: String,
    },
    FileFailed {
        transfer_id: String,
        file_id: String,
        status: Status,
    },
    FileRejected {
        transfer_id: String,
        file_id: String,
        by_peer: bool,
    },
    FilePaused {
        transfer_id: String,
        file_id: String,
    },
    FileThrottled {
        transfer_id: String,
        file_id: String,
        transfered: u64,
    },
    FilePending {
        transfer_id: String,
        file_id: String,
    },

    TransferFinalized {
        transfer_id: String,
        by_peer: bool,
    },
    TransferFailed {
        transfer_id: String,
        status: Status,
    },
    TransferDeferred {
        transfer_id: String,
        peer: String,
        status: Status,
    },

    FinalizeChecksumStarted {
        transfer_id: String,
        file_id: String,
        size: u64,
    },
    FinalizeChecksumFinished {
        transfer_id: String,
        file_id: String,
    },
    FinalizeChecksumProgress {
        transfer_id: String,
        file_id: String,
        bytes_checksummed: u64,
    },

    VerifyChecksumStarted {
        transfer_id: String,
        file_id: String,
        size: u64,
    },
    VerifyChecksumFinished {
        transfer_id: String,
        file_id: String,
    },
    VerifyChecksumProgress {
        transfer_id: String,
        file_id: String,
        bytes_checksummed: u64,
    },

    RuntimeError {
        status: crate::StatusCode,
    },
}

impl From<&drop_transfer::Error> for Status {
    fn from(value: &drop_transfer::Error) -> Self {
        Self {
            status: value.into(),
            os_error_code: value.os_err_code(),
        }
    }
}

impl From<EventKind> for Event {
    fn from(kind: EventKind) -> Self {
        Self {
            timestamp: current_timestamp(),
            kind,
        }
    }
}

impl From<(drop_transfer::Event, SystemTime)> for Event {
    fn from(event: (drop_transfer::Event, SystemTime)) -> Self {
        let (e, timestamp) = event;
        let timestamp = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Self {
            timestamp,
            kind: e.into(),
        }
    }
}

impl From<drop_transfer::Event> for EventKind {
    fn from(event: drop_transfer::Event) -> Self {
        use drop_transfer::Event::*;

        match event {
            RequestReceived(tx) => EventKind::RequestReceived {
                peer: tx.peer().to_string(),
                transfer_id: tx.id().to_string(),
                files: extract_transfer_files(tx.as_ref()),
            },
            RequestQueued(tx) => Self::RequestQueued {
                peer: tx.peer().to_string(),
                transfer_id: tx.id().to_string(),
                files: extract_transfer_files(tx.as_ref()),
            },
            FileUploadStarted(tx, fid, transfered) => Self::FileStarted {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                transfered,
            },
            FileDownloadStarted(tx, fid, _, transfered) => Self::FileStarted {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                transfered,
            },
            FileUploadProgress(tx, fid, progress) => Self::FileProgress {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                transfered: progress,
            },
            FileDownloadProgress(tx, fid, progress) => Self::FileProgress {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                transfered: progress,
            },
            FileUploadSuccess(tx, fid) => Self::FileUploaded {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
            },
            FileDownloadSuccess(tx, info) => Self::FileDownloaded {
                transfer_id: tx.id().to_string(),
                file_id: info.id.to_string(),
                final_path: info.final_path.0.to_string_lossy().to_string(),
            },
            FileUploadFailed(tx, fid, status) => Self::FileFailed {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                status: From::from(&status),
            },
            FileDownloadFailed(tx, fid, status) => Self::FileFailed {
                transfer_id: tx.id().to_string(),
                file_id: fid.to_string(),
                status: From::from(&status),
            },
            IncomingTransferCanceled(tx, by_peer) => Self::TransferFinalized {
                transfer_id: tx.id().to_string(),
                by_peer,
            },
            OutgoingTransferCanceled(tx, by_peer) => Self::TransferFinalized {
                transfer_id: tx.id().to_string(),
                by_peer,
            },
            OutgoingTransferFailed(tx, status, _) => Self::TransferFailed {
                transfer_id: tx.id().to_string(),
                status: From::from(&status),
            },
            FileDownloadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => Self::FileRejected {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                by_peer,
            },
            FileUploadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => Self::FileRejected {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                by_peer,
            },
            FileUploadPaused {
                transfer_id,
                file_id,
            } => Self::FilePaused {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },
            FileDownloadPaused {
                transfer_id,
                file_id,
            } => Self::FilePaused {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },

            FileUploadThrottled {
                transfer_id,
                file_id,
                transfered,
            } => Self::FileThrottled {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                transfered,
            },

            FinalizeChecksumStarted {
                transfer_id,
                file_id,
                size,
            } => Self::FinalizeChecksumStarted {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                size,
            },
            FinalizeChecksumFinished {
                transfer_id,
                file_id,
            } => Self::FinalizeChecksumFinished {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },
            FinalizeChecksumProgress {
                transfer_id,
                file_id,
                progress,
            } => Self::FinalizeChecksumProgress {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                bytes_checksummed: progress,
            },

            VerifyChecksumStarted {
                transfer_id,
                file_id,
                size,
            } => Self::VerifyChecksumStarted {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                size,
            },
            VerifyChecksumFinished {
                transfer_id,
                file_id,
            } => Self::VerifyChecksumFinished {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },
            VerifyChecksumProgress {
                transfer_id,
                file_id,
                progress,
            } => Self::VerifyChecksumProgress {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                bytes_checksummed: progress,
            },

            OutgoingTransferDeferred { transfer, error } => Self::TransferDeferred {
                transfer_id: transfer.id().to_string(),
                peer: transfer.peer().to_string(),
                status: Status::from(&error),
            },
            FileDownloadPending {
                transfer_id,
                file_id,
                ..
            } => Self::FilePending {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },
        }
    }
}

fn extract_transfer_files(t: &impl drop_transfer::Transfer) -> Vec<File> {
    t.files()
        .values()
        .map(|f| File {
            id: f.id().to_string(),
            path: f.subpath().to_string(),
            size: f.size(),
        })
        .collect()
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
