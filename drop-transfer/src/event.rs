use std::{path::Path, sync::Arc};

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
    RequestReceived(Arc<IncomingTransfer>),
    RequestQueued(Arc<OutgoingTransfer>),

    FileUploadStarted(Arc<OutgoingTransfer>, FileId, u64),
    FileDownloadStarted(Arc<IncomingTransfer>, FileId, String, u64),

    FileUploadProgress(Arc<OutgoingTransfer>, FileId, u64),
    FileDownloadProgress(Arc<IncomingTransfer>, FileId, u64),

    FileUploadSuccess(Arc<OutgoingTransfer>, FileId),
    FileDownloadSuccess(Arc<IncomingTransfer>, DownloadSuccess),

    FileUploadFailed(Arc<OutgoingTransfer>, FileId, Error),
    FileDownloadFailed(Arc<IncomingTransfer>, FileId, Error),

    FileUploadPaused {
        transfer_id: Uuid,
        file_id: FileId,
    },
    FileDownloadPaused {
        transfer_id: Uuid,
        file_id: FileId,
    },

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

    IncomingTransferCanceled(Arc<IncomingTransfer>, bool),
    OutgoingTransferCanceled(Arc<OutgoingTransfer>, bool),

    OutgoingTransferFailed(Arc<OutgoingTransfer>, Error, bool),
}
