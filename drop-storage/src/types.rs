type TransferId = String;
type FilePath = String;

#[derive(Debug, Copy, Clone)]
pub enum TransferType {
    Incoming = 0,
    Outgoing = 1,
}

impl Into<i32> for &TransferType {
    fn into(self) -> i32 {
        match self {
            TransferType::Incoming => 0,
            TransferType::Outgoing => 1,
        }
    }
}

#[derive(Debug)]
pub struct TransferPath {
    pub path: String,
    pub size: i64,
}

#[derive(Debug)]
pub struct TransferInfo {
    pub id: String,
    pub peer: String,
    pub files: Vec<TransferPath>,
}

#[derive(Debug)]
pub enum Event {
    Pending(TransferType, TransferInfo),
    Started(TransferType, TransferId, FilePath),

    FileCanceled(TransferType, TransferId, FilePath),
    TransferCanceled(TransferType, TransferInfo, bool),

    FileFailed(TransferType, TransferId, FilePath, u32, i64),
    TransferFailed(TransferType, TransferInfo, u32),

    FileUploadComplete(TransferId, FilePath),
    FileDownloadComplete(TransferId, FilePath, String),

    // Unused
    Progress,
}
