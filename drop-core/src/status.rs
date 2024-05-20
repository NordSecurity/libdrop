#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum Status {
    Finalized = 1,
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
    PermissionDenied = 40,
}

impl serde::Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (*self as u32).serialize(serializer)
    }
}

impl From<u32> for Status {
    fn from(value: u32) -> Self {
        use Status::*;

        match value {
            1 => Finalized,
            2 => BadPath,
            3 => BadFile,
            7 => BadTransfer,
            8 => BadTransferState,
            9 => BadFileId,
            15 => IoError,
            20 => TransferLimitsExceeded,
            21 => MismatchedSize,
            23 => InvalidArgument,
            27 => AddrInUse,
            28 => FileModified,
            29 => FilenameTooLong,
            30 => AuthenticationFailed,
            31 => StorageError,
            32 => DbLost,
            33 => FileChecksumMismatch,
            34 => FileRejected,
            35 => FileFailed,
            36 => FileFinished,
            37 => EmptyTransfer,
            38 => ConnectionClosedByPeer,
            39 => TooManyRequests,
            40 => PermissionDenied,
            _unknown => IoError, /* Use IO error because we have no clue what it is. This
                                  * shouldn't happen */
        }
    }
}
