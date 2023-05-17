use sqlx::types::chrono::NaiveDateTime;

type TransferId = String;
type FilePath = String;

#[derive(Debug, Copy, Clone)]
pub enum TransferType {
    Incoming = 0,
    Outgoing = 1,
}

impl From<TransferType> for i32 {
    fn from(transfer_type: TransferType) -> Self {
        match transfer_type {
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

    FileCanceled(TransferType, TransferId, FilePath, bool),
    TransferCanceled(TransferType, TransferInfo, bool),

    FileFailed(TransferType, TransferId, FilePath, u32),
    TransferFailed(TransferType, TransferInfo, u32),

    FileUploadComplete(TransferId, FilePath),
    FileDownloadComplete(TransferId, FilePath, String),

    Progress(TransferId, FilePath, i64),
}

#[derive(Debug)]
pub enum DbTransferType {
    Incoming(Vec<IncomingPath>),
    Outgoing(Vec<OutgoingPath>),
}

#[derive(Debug)]
pub struct Peer {
    pub id: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct Transfer {
    pub id: String,
    pub peer_id: String,
    pub transfer_type: DbTransferType,
    pub created_at: NaiveDateTime,
    pub active_states: Vec<TransferActiveState>,
    pub cancel_states: Vec<TransferCancelState>,
    pub failed_states: Vec<TransferFailedState>,
}

#[derive(Debug)]
pub struct TransferActiveState {
    pub transfer_id: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct TransferCancelState {
    pub transfer_id: String,
    pub by_peer: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct TransferFailedState {
    pub transfer_id: String,
    pub status_code: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct OutgoingPath {
    pub id: i64,
    pub transfer_id: String,
    pub path: String,
    pub bytes: i64,
    pub created_at: NaiveDateTime,
    pub pending_states: Vec<OutgoingPathPendingState>,
    pub started_states: Vec<OutgoingPathStartedState>,
    pub cancel_states: Vec<OutgoingPathCancelState>,
    pub failed_states: Vec<OutgoingPathFailedState>,
    pub completed_states: Vec<OutgoingPathCompletedState>,
}

#[derive(Debug)]
pub struct OutgoingPathPendingState {
    pub path_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct OutgoingPathStartedState {
    pub path_id: i64,
    pub bytes_sent: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct OutgoingPathCancelState {
    pub path_id: i64,
    pub by_peer: i64,
    pub bytes_sent: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct OutgoingPathFailedState {
    pub path_id: i64,
    pub status_code: i64,
    pub bytes_sent: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct OutgoingPathCompletedState {
    pub path_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct IncomingPath {
    pub id: i64,
    pub transfer_id: String,
    pub path: String,
    pub bytes: i64,
    pub created_at: NaiveDateTime,
    pub pending_states: Vec<IncomingPathPendingState>,
    pub started_states: Vec<IncomingPathStartedState>,
    pub cancel_states: Vec<IncomingPathCancelState>,
    pub failed_states: Vec<IncomingPathFailedState>,
    pub completed_states: Vec<IncomingPathCompletedState>,
}

#[derive(Debug)]
pub struct IncomingPathPendingState {
    pub path_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct IncomingPathStartedState {
    pub path_id: i64,
    pub bytes_received: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct IncomingPathCancelState {
    pub path_id: i64,
    pub by_peer: i64,
    pub bytes_received: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct IncomingPathFailedState {
    pub path_id: i64,
    pub status_code: i64,
    pub bytes_received: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct IncomingPathCompletedState {
    pub path_id: i64,
    pub final_path: String,
    pub created_at: NaiveDateTime,
}
