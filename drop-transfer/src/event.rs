use std::path::Path;

use uuid::Uuid;

use crate::{file::FileId, utils::Hidden, Error, Transfer};

#[derive(Debug)]
pub struct DownloadSuccess {
    pub id: FileId,
    pub final_path: Hidden<Box<Path>>,
}

#[derive(Debug)]
pub enum Event {
    RequestReceived(Transfer),
    RequestQueued(Transfer),

    FileUploadStarted(Transfer, FileId),
    FileDownloadStarted(Transfer, FileId, String),

    FileUploadProgress(Transfer, FileId, u64),
    FileDownloadProgress(Transfer, FileId, u64),

    FileUploadSuccess(Transfer, FileId),
    FileDownloadSuccess(Transfer, DownloadSuccess),

    FileUploadCancelled(Transfer, FileId, bool),
    FileDownloadCancelled(Transfer, FileId, bool),

    FileUploadFailed(Transfer, FileId, Error),
    FileDownloadFailed(Transfer, FileId, Error),

    FileUploadRejected {
        transfer_id: Uuid,
        file_id: FileId,
        by_peer: bool,
    },
    FileDownloadRejected {
        transfer_id: Uuid,
        file_id: FileId,
        by_peer: bool,
    },

    TransferCanceled(Transfer, bool, bool),

    TransferFailed(Transfer, Error, bool),
}
