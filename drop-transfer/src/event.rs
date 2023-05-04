use std::path::Path;

use crate::{file::FileSubPath, utils::Hidden, Error, Transfer};

#[derive(Debug)]
pub struct DownloadSuccess {
    pub id: FileSubPath,
    pub final_path: Hidden<Box<Path>>,
}

#[derive(Debug)]
pub enum Event {
    RequestReceived(Transfer),
    RequestQueued(Transfer),

    FileUploadStarted(Transfer, FileSubPath),
    FileDownloadStarted(Transfer, FileSubPath),

    FileUploadProgress(Transfer, FileSubPath, u64),
    FileDownloadProgress(Transfer, FileSubPath, u64),

    FileUploadSuccess(Transfer, FileSubPath),
    FileDownloadSuccess(Transfer, DownloadSuccess),

    FileUploadCancelled(Transfer, FileSubPath, bool),
    FileDownloadCancelled(Transfer, FileSubPath, bool),

    FileUploadFailed(Transfer, FileSubPath, Error),
    FileDownloadFailed(Transfer, FileSubPath, Error),

    TransferCanceled(Transfer, bool),

    TransferFailed(Transfer, Error),
}
