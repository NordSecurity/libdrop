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

    FileDownloadPending {
        transfer_id: Uuid,
        file_id: FileId,
        base_dir: String,
    },

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

    FileUploadThrottled {
        transfer_id: Uuid,
        file_id: FileId,
        transfered: u64,
    },

    IncomingTransferCanceled(Arc<IncomingTransfer>, bool),
    OutgoingTransferCanceled(Arc<OutgoingTransfer>, bool),

    OutgoingTransferFailed(Arc<OutgoingTransfer>, Error, bool),

    OutgoingTransferDeferred {
        transfer: Arc<OutgoingTransfer>,
        error: Error,
    },

    FinalizeChecksumStarted {
        transfer_id: Uuid,
        file_id: FileId,
        size: u64,
    },
    FinalizeChecksumFinished {
        transfer_id: Uuid,
        file_id: FileId,
    },
    FinalizeChecksumProgress {
        transfer_id: Uuid,
        file_id: FileId,
        progress: u64,
    },

    VerifyChecksumStarted {
        transfer_id: Uuid,
        file_id: FileId,
        size: u64,
    },
    VerifyChecksumFinished {
        transfer_id: Uuid,
        file_id: FileId,
    },
    VerifyChecksumProgress {
        transfer_id: Uuid,
        file_id: FileId,
        progress: u64,
    },
}
