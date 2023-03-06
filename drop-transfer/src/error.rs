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
    #[error("System time before UNIX epoch")]
    BadSystemTime,
    #[error("Truncated file")]
    TruncatedFile,
    #[error("Malformed UUID")]
    BadUuid,
    #[error("File size has changed")]
    MismatchedSize,
    #[error("Unexpected data")]
    UnexpectedData,
    #[error("Event channel closed")]
    ChannelClosed,
    #[error("IO error {0}")]
    Io(#[from] IoError),
    #[error("Could not send data to remote peer")]
    DataSend,
    #[error("Got directory when expecting a file")]
    DirectoryNotExpected,
    #[error("Transfers cannot be empty")]
    EmptyTransfer,
    #[error("Transfer closed by peer while expecting more data")]
    TransferClosedByPeer,
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
            Error::BadSystemTime => 10,
            Error::TruncatedFile => 11,
            Error::BadUuid => 13,
            Error::ChannelClosed => 14,
            Error::Io(_) => 15,
            Error::DataSend => 16,
            Error::DirectoryNotExpected => 17,
            Error::EmptyTransfer => 18,
            Error::TransferClosedByPeer => 19,
            Error::TransferLimitsExceeded => 20,
            Error::MismatchedSize => 21,
            Error::UnexpectedData => 22,
            Error::InvalidArgument => 23,
            Error::TransferTimeout => 24,
            Error::WsServer(_) => 25,
            Error::WsClient(_) => 26,
            Error::AddrInUse => 27,
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
