use std::io::Error as IoError;

use drop_analytics::MOOSE_STATUS_SUCCESS;
use tokio_tungstenite::tungstenite;

use crate::manager::FileTerminalState;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Operation was canceled")]
    Canceled,
    #[error("Invalid path: {0}")]
    BadPath(String),
    #[error("Could not open file")]
    BadFile,
    #[error("Transfer not found")]
    BadTransfer,
    #[error("Invalid transfer state: {0}")]
    BadTransferState(String),
    #[error("Invalid file ID")]
    BadFileId,
    #[error("File size has changed")]
    MismatchedSize,
    #[error("Unexpected data")]
    UnexpectedData,
    #[error("IO error {0}")]
    Io(#[from] IoError),
    #[error("Got directory when expecting a file")]
    DirectoryNotExpected,
    #[error("Transfer limits exceeded")]
    TransferLimitsExceeded,
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("Server connection failure: {0}")]
    WsServer(#[from] warp::Error),
    #[error("Client connection failure: {0}")]
    WsClient(#[from] tungstenite::Error),
    #[error("Address already in use")]
    AddrInUse,
    #[error("File modified")]
    FileModified,
    #[error("Filename is too long")]
    FilenameTooLong,
    #[error("Failed to authenticate to the peer")]
    AuthenticationFailed,
    #[error("Storage error: {0}")]
    StorageError(#[from] drop_storage::error::Error),
    #[error("Checksum validation failed")]
    ChecksumMismatch,
    #[error("File in mismatched state: {0:?}")]
    FileStateMismatch(FileTerminalState),
    #[error("Empty transfer")]
    EmptyTransfer,
}

impl Error {
    pub fn os_err_code(&self) -> Option<i32> {
        match self {
            Error::Io(ioerr) => ioerr.raw_os_error().map(|c| c as _),
            Error::WsServer(_) => None,
            Error::WsClient(terr) => {
                if let tungstenite::Error::Io(ioerr) = terr {
                    ioerr.raw_os_error().map(|c| c as _)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl From<&Error> for u32 {
    fn from(err: &Error) -> Self {
        use drop_core::Status;

        match err {
            Error::Canceled => Status::Canceled as _,
            Error::BadPath(_) => Status::BadPath as _,
            Error::BadFile => Status::BadFile as _,
            Error::BadTransfer => Status::BadTransfer as _,
            Error::BadTransferState(_) => Status::BadTransferState as _,
            Error::BadFileId => Status::BadFileId as _,
            Error::Io(_) => Status::IoError as _,
            Error::DirectoryNotExpected => Status::BadFile as _,
            Error::TransferLimitsExceeded => Status::TransferLimitsExceeded as _,
            Error::MismatchedSize => Status::MismatchedSize as _,
            Error::UnexpectedData => Status::MismatchedSize as _,
            Error::InvalidArgument => Status::InvalidArgument as _,
            Error::WsServer(_) => Status::IoError as _,
            Error::WsClient(_) => Status::IoError as _,
            Error::AddrInUse => Status::AddrInUse as _,
            Error::FileModified => Status::FileModified as _,
            Error::FilenameTooLong => Status::FilenameTooLong as _,
            Error::AuthenticationFailed => Status::AuthenticationFailed as _,
            Error::StorageError(_) => Status::StorageError as _,
            Error::ChecksumMismatch => Status::FileChecksumMismatch as _,
            Error::FileStateMismatch(FileTerminalState::Rejected) => Status::FileRejected as _,
            Error::FileStateMismatch(FileTerminalState::Completed) => Status::FileFinished as _,
            Error::FileStateMismatch(FileTerminalState::Failed) => Status::FileFailed as _,
            Error::EmptyTransfer => Status::EmptyTransfer as _,
        }
    }
}

impl From<&Error> for i32 {
    fn from(value: &Error) -> Self {
        u32::from(value) as _
    }
}

pub trait ResultExt {
    fn to_moose_status(&self) -> i32;
}

impl<T> ResultExt for super::Result<T> {
    fn to_moose_status(&self) -> i32 {
        match self {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(err) => u32::from(err) as _,
        }
    }
}

impl From<walkdir::Error> for Error {
    fn from(value: walkdir::Error) -> Self {
        value
            .into_io_error()
            .map(Into::into)
            .unwrap_or_else(|| Error::BadPath("Filesystem loop detected".into()))
    }
}
