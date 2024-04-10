use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::sync;

type TransferId = uuid::Uuid;
type FileId = String;

fn serialize_datetime<S>(timestamp: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_i64(timestamp.and_utc().timestamp_millis())
}

#[derive(Serialize)]
#[serde(tag = "state")]
pub enum OutgoingPathStateEventData {
    #[serde(rename = "started")]
    Started { bytes_sent: i64 },
    #[serde(rename = "failed")]
    Failed { status_code: i64, bytes_sent: i64 },
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "rejected")]
    Rejected { by_peer: bool, bytes_sent: i64 },
    #[serde(rename = "paused")]
    Paused { bytes_sent: i64 },
}

#[derive(Serialize)]
#[serde(tag = "state")]
pub enum IncomingPathStateEventData {
    #[serde(rename = "pending")]
    Pending { base_dir: String },
    #[serde(rename = "started")]
    Started { bytes_received: i64 },
    #[serde(rename = "failed")]
    Failed {
        status_code: i64,
        bytes_received: i64,
    },
    #[serde(rename = "completed")]
    Completed { final_path: String },
    #[serde(rename = "rejected")]
    Rejected { by_peer: bool, bytes_received: i64 },
    #[serde(rename = "paused")]
    Paused { bytes_received: i64 },
}

#[derive(Serialize)]
pub struct OutgoingPathStateEvent {
    #[serde(skip_serializing)]
    pub path_id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: OutgoingPathStateEventData,
}

#[derive(Serialize)]
pub struct IncomingPathStateEvent {
    #[serde(skip_serializing)]
    pub path_id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: IncomingPathStateEventData,
}

#[derive(Serialize)]
#[serde(tag = "state")]
pub enum TransferStateEventData {
    #[serde(rename = "cancel")]
    Cancel { by_peer: bool },
    #[serde(rename = "failed")]
    Failed { status_code: i64 },
}

#[derive(Serialize)]
pub struct TransferStateEvent {
    #[serde(skip_serializing)]
    pub transfer_id: TransferId,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    #[serde(flatten)]
    pub data: TransferStateEventData,
}

#[derive(Copy, Clone)]
#[repr(u32)]
pub enum TransferType {
    Incoming = 0,
    Outgoing = 1,
}

pub struct TransferIncomingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub size: i64,
}

pub struct TransferOutgoingPath {
    pub file_id: FileId,
    pub relative_path: String,
    pub uri: url::Url,
    pub size: i64,
}

pub enum TransferFiles {
    Incoming(Vec<TransferIncomingPath>),
    Outgoing(Vec<TransferOutgoingPath>),
}

pub struct TransferInfo {
    pub id: TransferId,
    pub peer: String,
    pub files: TransferFiles,
}

pub struct FileChecksum {
    pub file_id: FileId,
    pub checksum: Option<Vec<u8>>,
}

pub struct IncomingFileToRetry {
    pub file_id: String,
    pub subpath: String,
    pub size: u64,
}

pub struct IncomingTransferToRetry {
    pub uuid: uuid::Uuid,
    pub peer: String,
    pub files: Vec<IncomingFileToRetry>,
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

pub struct OutgoingTransferToRetry {
    pub uuid: uuid::Uuid,
    pub peer: String,
    pub files: Vec<OutgoingFileToRetry>,
}

pub struct TempFileLocation {
    pub file_id: String,
    pub base_path: String,
}

pub struct FileSyncState {
    pub sync: sync::FileState,
    pub is_rejected: bool,
    pub is_success: bool,
    pub is_failed: bool,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "paths")]
pub enum DbTransferType {
    #[serde(rename = "incoming")]
    Incoming(Vec<IncomingPath>),
    #[serde(rename = "outgoing")]
    Outgoing(Vec<OutgoingPath>),
}

#[derive(Serialize)]
pub struct Transfer {
    pub id: TransferId,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    pub peer_id: String,
    pub states: Vec<TransferStateEvent>,
    #[serde(flatten)]
    pub transfer_type: DbTransferType,
}

#[derive(Serialize)]
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
    pub bytes_sent: i64,
    pub states: Vec<OutgoingPathStateEvent>,
}

#[derive(Serialize)]
pub struct IncomingPath {
    #[serde(skip_serializing)]
    pub id: i64,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: NaiveDateTime,
    pub transfer_id: TransferId,
    pub relative_path: String,
    pub file_id: String,
    pub bytes: i64,
    pub bytes_received: i64,
    pub states: Vec<IncomingPathStateEvent>,
}
