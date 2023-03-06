use std::path::Path;

use crate::{utils::Hidden, Error, Transfer};

#[derive(Debug)]
pub struct DownloadSuccess {
    pub id: Hidden<Box<Path>>,
    pub final_path: Hidden<Box<Path>>,
}

#[derive(Debug)]
pub enum Event {
    RequestReceived(Transfer),
    RequestQueued(Transfer),

    FileUploadStarted(Transfer, Hidden<Box<Path>>),
    FileDownloadStarted(Transfer, Hidden<Box<Path>>),

    FileUploadProgress(Transfer, Hidden<Box<Path>>, u64),
    FileDownloadProgress(Transfer, Hidden<Box<Path>>, u64),

    FileUploadSuccess(Transfer, Hidden<Box<Path>>),
    FileDownloadSuccess(Transfer, DownloadSuccess),

    FileUploadCancelled(Transfer, Hidden<Box<Path>>),
    FileDownloadCancelled(Transfer, Hidden<Box<Path>>),

    FileUploadFailed(Transfer, Hidden<Box<Path>>, Error),
    FileDownloadFailed(Transfer, Hidden<Box<Path>>, Error),

    TransferCanceled(Transfer, bool),

    TransferFailed(Transfer, Error),
}
