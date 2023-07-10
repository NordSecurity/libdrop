use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::Serialize;

type TransferId = uuid::Uuid;
type FileId = String;

fn serialize_datetime<S>(timestamp: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_i64(timestamp.timestamp_millis())
}

#[derive(Debug, Serialize)]
#[serde(tag = "state")]
pub enum OutgoingPathStateEventData {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "started")]
    Started { bytes_sent: i64 },
    #[serde(rename = "cancel")]
    Cancel { by_peer: bool, bytes_sent: i64 },
    #[serde(rename = "failed")]
    Failed { status_code: i64, bytes_sent: i64 },
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "rejected")]
    Rejected { by_peer: bool },
}

#[derive(Debug, Serialize)]
#[serde(tag = "state")]
pub enum IncomingPathStateEventData {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "started")]
    Started {
        base_dir: String,
        bytes_received: i64,
    },
    #[serde(rename = "cancel")]
    Cancel { by_peer: bool, bytes_received: i64 },
    #[serde(rename = "failed")]
    Failed {
        status_code: i64,
        bytes_received: i64,
    },
    #[serde(rename = "completed")]
    Completed { final_path: String },
    #[serde(rename = "rejected")]
    Rejected { by_peer: bool },
}

#[derive(Debug, Serialize)]
pub struct OutgoingPathStateEvent {
    #[serde(skip_serializing)]
    pub path_id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: OutgoingPathStateEventData,
}

#[derive(Debug, Serialize)]
pub struct IncomingPathStateEvent {
    #[serde(skip_serializing)]
    pub path_id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: IncomingPathStateEventData,
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
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: TransferStateEventData,
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum TransferType {
    Incoming = 0,
    Outgoing = 1,
}

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct TransferIncomingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub size: i64,
}

#[derive(Debug)]
pub struct TransferOutgoingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub uri: url::Url,
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

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct IncomingTransferInfo {
    pub peer: String,
    pub files: Vec<TransferIncomingPath>,
}

#[derive(Debug)]
pub struct FileChecksum {
    pub file_id: FileId,
    pub checksum: Option<Vec<u8>>,
}

pub struct IncomingFileToRetry {
    pub file_id: String,
    pub subpath: String,
    pub basepath: String,
    pub size: u64,
}

pub struct FinishedIncomingFile {
    pub subpath: String,
    pub final_path: String,
}

pub struct OutgoingFileToRetry {
    pub file_id: String,
    pub subpath: String,
    pub uri: url::Url,
    pub size: i64,
}

pub struct TransferToRetry {
    pub uuid: uuid::Uuid,
    pub peer: String,
    pub files: Vec<OutgoingFileToRetry>,
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
    FileProgress {
        transfer_id: TransferId,
        file_id: FileId,
        progress: i64,
    },
    FileReject {
        transfer_type: TransferType,
        transfer_id: TransferId,
        file_id: FileId,
        by_peer: bool,
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
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct Transfer {
    pub id: TransferId,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    pub peer_id: String,
    pub states: Vec<TransferStateEvent>,
    #[serde(flatten)]
    pub transfer_type: DbTransferType,
}

#[derive(Debug, Serialize)]
pub struct OutgoingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    pub transfer_id: TransferId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_uri: Option<url::Url>,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub states: Vec<OutgoingPathStateEvent>,
}

#[derive(Debug, Serialize)]
pub struct IncomingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    pub transfer_id: TransferId,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub states: Vec<IncomingPathStateEvent>,
}
