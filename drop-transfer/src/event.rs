use std::path::Path;

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
    FileDownloadStarted(Transfer, FileId),

    FileUploadProgress(Transfer, FileId, u64),
    FileDownloadProgress(Transfer, FileId, u64),

    FileUploadSuccess(Transfer, FileId),
    FileDownloadSuccess(Transfer, DownloadSuccess),

    FileUploadCancelled(Transfer, FileId, bool),
    FileDownloadCancelled(Transfer, FileId, bool),

    FileUploadFailed(Transfer, FileId, Error),
    FileDownloadFailed(Transfer, FileId, Error),

    TransferCanceled(Transfer, bool),

    TransferFailed(Transfer, Error, bool),
}
