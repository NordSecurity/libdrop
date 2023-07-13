use std::path::Path;

use uuid::Uuid;

use crate::{
    file::FileId,
    transfer::{IncomingTransfer, OutgoingTransfer},
    utils::Hidden,
    Error,
};

#[derive(Debug)]
pub struct DownloadSuccess {
    pub id: FileId,
    pub final_path: Hidden<Box<Path>>,
}

#[derive(Debug)]
pub enum Event {
    RequestReceived(IncomingTransfer),
    RequestQueued(OutgoingTransfer),

    FileUploadStarted(OutgoingTransfer, FileId),
    FileDownloadStarted(IncomingTransfer, FileId, String),

    FileUploadProgress(OutgoingTransfer, FileId, u64),
    FileDownloadProgress(IncomingTransfer, FileId, u64),

    FileUploadSuccess(OutgoingTransfer, FileId),
    FileDownloadSuccess(IncomingTransfer, DownloadSuccess),

    FileUploadCancelled(OutgoingTransfer, FileId, bool),
    FileDownloadCancelled(IncomingTransfer, FileId, bool),

    FileUploadFailed(OutgoingTransfer, FileId, Error),
    FileDownloadFailed(IncomingTransfer, FileId, Error),

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

    IncomingTransferCanceled(IncomingTransfer, bool),
    OutgoingTransferCanceled(OutgoingTransfer, bool),

    IncomingTransferFailed(IncomingTransfer, Error, bool),
    OutgoingTransferFailed(OutgoingTransfer, Error, bool),
}
