use std::io::{Error as IoError, ErrorKind};

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
    #[error("Peer closed the connection")]
    ConnectionClosedByPeer,
    #[error("Peer responded with too many requests status")]
    TooManyRequests,
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

impl From<&Error> for drop_core::Status {
    fn from(err: &Error) -> Self {
        use drop_core::Status;

        match err {
            Error::Canceled => Status::Finalized,
            Error::BadPath(_) => Status::BadPath,
            Error::BadFile => Status::BadFile,
            Error::BadTransfer => Status::BadTransfer,
            Error::BadTransferState(_) => Status::BadTransferState,
            Error::BadFileId => Status::BadFileId,
            Error::Io(io) => match io.kind() {
                ErrorKind::PermissionDenied => Status::PermissionDenied,
                _ => Status::IoError,
            },
            Error::DirectoryNotExpected => Status::BadFile,
            Error::TransferLimitsExceeded => Status::TransferLimitsExceeded,
            Error::MismatchedSize => Status::MismatchedSize,
            Error::UnexpectedData => Status::MismatchedSize,
            Error::InvalidArgument => Status::InvalidArgument,
            Error::WsServer(_) => Status::IoError,
            Error::WsClient(_) => Status::IoError,
            Error::AddrInUse => Status::AddrInUse,
            Error::FileModified => Status::FileModified,
            Error::FilenameTooLong => Status::FilenameTooLong,
            Error::AuthenticationFailed => Status::AuthenticationFailed,
            Error::StorageError(_) => Status::StorageError,
            Error::ChecksumMismatch => Status::FileChecksumMismatch,
            Error::FileStateMismatch(FileTerminalState::Rejected) => Status::FileRejected,
            Error::FileStateMismatch(FileTerminalState::Completed) => Status::FileFinished,
            Error::FileStateMismatch(FileTerminalState::Failed) => Status::FileFailed,
            Error::EmptyTransfer => Status::EmptyTransfer,
            Error::ConnectionClosedByPeer => Status::ConnectionClosedByPeer,
            Error::TooManyRequests => Status::TooManyRequests,
        }
    }
}

impl From<&Error> for u32 {
    fn from(value: &Error) -> Self {
        drop_core::Status::from(value) as u32
    }
}

impl From<&Error> for i32 {
    fn from(value: &Error) -> Self {
        u32::from(value) as i32
    }
}

pub trait ResultExt {
    fn to_moose_status(&self) -> i32;
}

impl<T> ResultExt for super::Result<T> {
    fn to_moose_status(&self) -> i32 {
        match self {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(err) => i32::from(err) as _,
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
