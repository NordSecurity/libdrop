#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum Status {
    Canceled = 1,
    BadPath = 2,
    BadFile = 3,
    ServiceStop = 6,
    BadTransfer = 7,
    BadTransferState = 8,
    BadFileId = 9,
    Io = 15,
    DirectoryNotExpected = 17,
    TransferLimitsExceeded = 20,
    MismatchedSize = 21,
    UnexpectedData = 22,
    InvalidArgument = 23,
    TransferTimeout = 24,
    WsServer = 25,
    WsClient = 26,
    AddrInUse = 27,
    FileModified = 28,
    FilenameTooLong = 29,
    AuthenticationFailed = 30,
    StorageError = 31,
    DbLost = 32,
    FileChecksumMismatch = 33,
}

impl serde::Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (*self as u32).serialize(serializer)
    }
}
