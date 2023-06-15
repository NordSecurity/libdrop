use serde::Serialize;

type TransferId = uuid::Uuid;
type FileId = String;

#[derive(Debug, Serialize)]
#[serde(tag = "state")]
pub enum PathStateEventData {
    #[serde(rename = "pending")]
    OutgoingPending,
    #[serde(rename = "started")]
    OutgoingStarted { bytes_sent: i64 },
    #[serde(rename = "cancel")]
    OutgoingCancel { by_peer: bool, bytes_sent: i64 },
    #[serde(rename = "failed")]
    OutgoingFailed { status_code: i64, bytes_sent: i64 },
    #[serde(rename = "completed")]
    OutgoingCompleted,
    #[serde(rename = "pending")]
    IncomingPending,
    #[serde(rename = "started")]
    IncomingStarted {
        base_dir: String,
        bytes_received: i64,
    },
    #[serde(rename = "cancel")]
    IncomingCancel { by_peer: bool, bytes_received: i64 },
    #[serde(rename = "failed")]
    IncomingFailed {
        status_code: i64,
        bytes_received: i64,
    },
    #[serde(rename = "completed")]
    IncomingCompleted { final_path: String },
}

#[derive(Debug, Serialize)]
pub struct PathStateEvent {
    #[serde(skip_serializing)]
    pub path_id: i64,
    pub created_at: i64,
    #[serde(flatten)]
    pub data: PathStateEventData,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state")]
pub enum TransferStateEventData {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "cancel")]
    Cancel { by_peer: bool },
    #[serde(rename = "failed")]
    Failed { status_code: i64 },
}

#[derive(Debug, Serialize)]
pub struct TransferStateEvent {
    #[serde(skip_serializing)]
    pub transfer_id: TransferId,
    pub created_at: i64,
    #[serde(flatten)]
    pub data: TransferStateEventData,
}

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
    pub created_at: i64,
    pub peer_id: String,
    pub states: Vec<TransferStateEvent>,
    #[serde(flatten)]
    pub transfer_type: DbTransferType,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    pub created_at: i64,
    pub transfer_id: TransferId,
    pub base_path: String,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub states: Vec<PathStateEvent>,
}

#[derive(Debug, Serialize)]
pub struct IncomingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    pub created_at: i64,
    pub transfer_id: TransferId,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub states: Vec<PathStateEvent>,
}
