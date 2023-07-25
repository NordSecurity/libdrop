use std::io::Error as IoError;

use tokio_tungstenite::tungstenite;

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
    #[error("Transfer timeout")]
    TransferTimeout,
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
    #[error("Storage error")]
    StorageError,
    #[error("Checksum validation failed")]
    ChecksumMismatch,
    #[error("File is rejected")]
    Rejected,
}

impl Error {
    pub fn os_err_code(&self) -> Option<i32> {
        match self {
            Error::Io(ioerr) => ioerr.raw_os_error().map(|c| c as _),
            Error::WsServer(_) => {
                // TODO(msz): Theoretically it should be possible to extract OS error from WS
                // server but warp does not make it easy. Maybe one
                // day we will rewrite warp server with raw tungstenite
                None
            }
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
            Error::Io(_) => Status::Io as _,
            Error::DirectoryNotExpected => Status::DirectoryNotExpected as _,
            Error::TransferLimitsExceeded => Status::TransferLimitsExceeded as _,
            Error::MismatchedSize => Status::MismatchedSize as _,
            Error::UnexpectedData => Status::UnexpectedData as _,
            Error::InvalidArgument => Status::InvalidArgument as _,
            Error::TransferTimeout => Status::TransferTimeout as _,
            Error::WsServer(_) => Status::WsServer as _,
            Error::WsClient(_) => Status::WsClient as _,
            Error::AddrInUse => Status::AddrInUse as _,
            Error::FileModified => Status::FileModified as _,
            Error::FilenameTooLong => Status::FilenameTooLong as _,
            Error::AuthenticationFailed => Status::AuthenticationFailed as _,
            Error::StorageError => Status::StorageError as _,
            Error::ChecksumMismatch => Status::FileChecksumMismatch as _,
            Error::Rejected => Status::FileRejected as _,
        }
    }
}

pub trait ResultExt {
    fn to_status(&self) -> Result<(), i32>;
}

impl<T> ResultExt for super::Result<T> {
    fn to_status(&self) -> Result<(), i32> {
        match self {
            Ok(_) => Ok(()),
            Err(err) => Err(u32::from(err) as _),
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

impl From<drop_storage::error::Error> for Error {
    fn from(_: drop_storage::error::Error) -> Self {
        Self::StorageError
    }
}
