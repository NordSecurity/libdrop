use serde::Serialize;

type TransferId = uuid::Uuid;
type FileId = String;

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum TransferType {
    Incoming = 0,
    Outgoing = 1,
}

#[derive(Debug)]
pub struct TransferIncomingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub size: i64,
}

#[derive(Debug)]
pub struct TransferOutgoingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub base_path: String,
    pub size: i64,
}

#[derive(Debug)]
pub enum TransferFiles {
    Incoming(Vec<TransferIncomingPath>),
    Outgoing(Vec<TransferOutgoingPath>),
}

#[derive(Debug)]
pub struct TransferInfo {
    pub id: TransferId,
    pub peer: String,
    pub files: TransferFiles,
}

#[derive(Debug)]
pub struct FileChecksum {
    pub file_id: FileId,
    pub checksum: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum Event {
    Pending {
        transfer_info: TransferInfo,
    },
    FileUploadStarted {
        transfer_id: TransferId,
        file_id: FileId,
    },
    FileDownloadStarted {
        transfer_id: TransferId,
        file_id: FileId,
        base_dir: String,
    },
    FileCanceled {
        transfer_type: TransferType,
        transfer_id: TransferId,
        file_id: FileId,
        by_peer: bool,
    },
    TransferCanceled {
        transfer_type: TransferType,
        transfer_info: TransferInfo,
        by_peer: bool,
    },
    FileFailed {
        transfer_type: TransferType,
        transfer_id: TransferId,
        file_id: FileId,
        error_code: u32,
    },
    TransferFailed {
        transfer_type: TransferType,
        transfer_info: TransferInfo,
        error_code: u32,
    },
    FileUploadComplete {
        transfer_id: TransferId,
        file_id: FileId,
    },
    FileDownloadComplete {
        transfer_id: TransferId,
        file_id: FileId,
        final_path: String,
    },
    Progress {
        transfer_id: TransferId,
        file_id: FileId,
        progress: i64,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "paths")]
pub enum DbTransferType {
    #[serde(rename = "incoming")]
    Incoming(Vec<IncomingPath>),
    #[serde(rename = "outgoing")]
    Outgoing(Vec<OutgoingPath>),
}

#[derive(Debug, Serialize)]
pub struct Peer {
    pub id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct Transfer {
    pub id: TransferId,
    pub peer_id: String,
    #[serde(flatten)]
    pub transfer_type: DbTransferType,
    pub created_at: i64,
    pub active_states: Vec<TransferActiveState>,
    pub cancel_states: Vec<TransferCancelState>,
    pub failed_states: Vec<TransferFailedState>,
}

#[derive(Debug, Serialize)]
pub struct TransferActiveState {
    #[serde(skip_serializing)]
    pub transfer_id: TransferId,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct TransferCancelState {
    #[serde(skip_serializing)]
    pub transfer_id: TransferId,
    pub by_peer: bool,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct TransferFailedState {
    #[serde(skip_serializing)]
    pub transfer_id: TransferId,
    pub status_code: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    pub transfer_id: TransferId,
    pub base_path: String,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub created_at: i64,
    pub pending_states: Vec<OutgoingPathPendingState>,
    pub started_states: Vec<OutgoingPathStartedState>,
    pub cancel_states: Vec<OutgoingPathCancelState>,
    pub failed_states: Vec<OutgoingPathFailedState>,
    pub completed_states: Vec<OutgoingPathCompletedState>,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathPendingState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathStartedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub bytes_sent: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathCancelState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub by_peer: bool,
    pub bytes_sent: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathFailedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub status_code: i64,
    pub bytes_sent: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathCompletedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IncomingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    pub transfer_id: TransferId,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub created_at: i64,
    pub pending_states: Vec<IncomingPathPendingState>,
    pub started_states: Vec<IncomingPathStartedState>,
    pub cancel_states: Vec<IncomingPathCancelState>,
    pub failed_states: Vec<IncomingPathFailedState>,
    pub completed_states: Vec<IncomingPathCompletedState>,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathPendingState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathStartedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub base_dir: String,
    pub bytes_received: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathCancelState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub by_peer: bool,
    pub bytes_received: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathFailedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub status_code: i64,
    pub bytes_received: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathCompletedState {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub final_path: String,
    pub created_at: i64,
}
