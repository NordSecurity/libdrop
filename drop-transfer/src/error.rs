use std::io::Error as IoError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Operation was canceled")]
    Canceled,
    #[error("Invalid path")]
    BadPath,
    #[error("Could not open file")]
    BadFile,
    #[error("Service failed to stop")]
    ServiceStop,
    #[error("Transfer not found")]
    BadTransfer,
    #[error("Invalid transfer state")]
    BadTransferState,
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
    WsClient(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Address already in use")]
    AddrInUse,
    #[error("File modified")]
    FileModified,
    #[error("Filename is too long")]
    FilenameTooLong,
    #[error("Failed to authenticate to the peer")]
    AuthenticationFailed,
}

impl From<&Error> for u32 {
    fn from(err: &Error) -> Self {
        match err {
            Error::Canceled => 1,
            Error::BadPath => 2,
            Error::BadFile => 3,
            Error::ServiceStop => 6,
            Error::BadTransfer => 7,
            Error::BadTransferState => 8,
            Error::BadFileId => 9,
            Error::Io(_) => 15,
            Error::DirectoryNotExpected => 17,
            Error::TransferLimitsExceeded => 20,
            Error::MismatchedSize => 21,
            Error::UnexpectedData => 22,
            Error::InvalidArgument => 23,
            Error::TransferTimeout => 24,
            Error::WsServer(_) => 25,
            Error::WsClient(_) => 26,
            Error::AddrInUse => 27,
            Error::FileModified => 28,
            Error::FilenameTooLong => 29,
            Error::AuthenticationFailed => 30,
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
