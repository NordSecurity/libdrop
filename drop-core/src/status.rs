#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum Status {
    Canceled = 1,
    BadPath = 2,
    BadFile = 3,
    BadTransfer = 7,
    BadTransferState = 8,
    BadFileId = 9,
    IoError = 15,
    TransferLimitsExceeded = 20,
    MismatchedSize = 21,
    InvalidArgument = 23,
    AddrInUse = 27,
    FileModified = 28,
    FilenameTooLong = 29,
    AuthenticationFailed = 30,
    StorageError = 31,
    DbLost = 32,
    FileChecksumMismatch = 33,
    FileRejected = 34,
    FileFailed = 35,
    FileFinished = 36,
    EmptyTransfer = 37,
    ConnectionClosedByPeer = 38,
    TooManyRequests = 39,
}

impl serde::Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (*self as u32).serialize(serializer)
    }
}
