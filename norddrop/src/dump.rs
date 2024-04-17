use drop_storage::types as db;

pub enum TransferStateKind {
    Cancel { by_peer: bool },
    Failed { status: i64 },
}

pub struct TransferState {
    pub created_at: i64,
    pub kind: TransferStateKind,
}

pub enum IncomingPathStateKind {
    Pending { base_dir: String },
    Started { bytes_received: u64 },
    Failed { status: i64, bytes_received: u64 },
    Completed { final_path: String },
    Rejected { by_peer: bool, bytes_received: u64 },
    Paused { bytes_received: u64 },
}

pub struct IncomingPathState {
    pub created_at: i64,
    pub kind: IncomingPathStateKind,
}

pub struct IncomingPath {
    pub file_id: String,
    pub relative_path: String,
    pub bytes: u64,
    pub bytes_received: u64,
    pub states: Vec<IncomingPathState>,
}

pub enum OutgoingPathStateKind {
    Started { bytes_sent: u64 },
    Failed { status: i64, bytes_sent: u64 },
    Completed,
    Rejected { by_peer: bool, bytes_sent: u64 },
    Paused { bytes_sent: u64 },
}

pub struct OutgoingPathState {
    pub created_at: i64,
    pub kind: OutgoingPathStateKind,
}

pub enum OutgoingFileSource {
    BasePath { base_path: String },
    ContentUri { uri: String },
}

pub struct OutgoingPath {
    pub file_id: String,
    pub relative_path: String,
    pub bytes: u64,
    pub bytes_sent: u64,
    pub source: OutgoingFileSource,
    pub states: Vec<OutgoingPathState>,
}

pub enum TransferKind {
    Incoming { paths: Vec<IncomingPath> },
    Outgoing { paths: Vec<OutgoingPath> },
}

pub struct TransferInfo {
    pub id: String,
    pub created_at: i64,
    pub peer: String,
    pub states: Vec<TransferState>,
    pub kind: TransferKind,
}

impl From<db::TransferStateEventData> for TransferStateKind {
    fn from(value: db::TransferStateEventData) -> Self {
        match value {
            db::TransferStateEventData::Cancel { by_peer } => Self::Cancel { by_peer },
            db::TransferStateEventData::Failed { status_code } => Self::Failed {
                status: status_code,
            },
        }
    }
}

impl From<db::TransferStateEvent> for TransferState {
    fn from(state: db::TransferStateEvent) -> Self {
        TransferState {
            created_at: state.created_at.and_utc().timestamp_millis(),
            kind: state.data.into(),
        }
    }
}

impl From<db::Transfer> for TransferInfo {
    fn from(info: db::Transfer) -> Self {
        TransferInfo {
            id: info.id.to_string(),
            created_at: info.created_at.and_utc().timestamp_millis(),
            peer: info.peer_id,
            states: info.states.into_iter().map(TransferState::from).collect(),
            kind: info.transfer_type.into(),
        }
    }
}

impl From<db::IncomingPathStateEventData> for IncomingPathStateKind {
    fn from(value: db::IncomingPathStateEventData) -> Self {
        match value {
            db::IncomingPathStateEventData::Pending { base_dir } => {
                IncomingPathStateKind::Pending { base_dir }
            }
            db::IncomingPathStateEventData::Started { bytes_received } => {
                IncomingPathStateKind::Started {
                    bytes_received: bytes_received as _,
                }
            }
            db::IncomingPathStateEventData::Failed {
                status_code,
                bytes_received,
            } => IncomingPathStateKind::Failed {
                status: status_code,
                bytes_received: bytes_received as _,
            },
            db::IncomingPathStateEventData::Completed { final_path } => {
                IncomingPathStateKind::Completed { final_path }
            }
            db::IncomingPathStateEventData::Rejected {
                by_peer,
                bytes_received,
            } => IncomingPathStateKind::Rejected {
                by_peer,
                bytes_received: bytes_received as _,
            },
            db::IncomingPathStateEventData::Paused { bytes_received } => {
                IncomingPathStateKind::Paused {
                    bytes_received: bytes_received as _,
                }
            }
        }
    }
}

impl From<db::IncomingPathStateEvent> for IncomingPathState {
    fn from(state: db::IncomingPathStateEvent) -> Self {
        IncomingPathState {
            created_at: state.created_at.and_utc().timestamp_millis(),
            kind: state.data.into(),
        }
    }
}

impl From<db::IncomingPath> for IncomingPath {
    fn from(path: db::IncomingPath) -> Self {
        IncomingPath {
            file_id: path.file_id,
            relative_path: path.relative_path,
            bytes: path.bytes as _,
            bytes_received: path.bytes_received as _,
            states: path
                .states
                .into_iter()
                .map(IncomingPathState::from)
                .collect(),
        }
    }
}

impl From<db::OutgoingPathStateEventData> for OutgoingPathStateKind {
    fn from(value: db::OutgoingPathStateEventData) -> Self {
        match value {
            db::OutgoingPathStateEventData::Started { bytes_sent } => {
                OutgoingPathStateKind::Started {
                    bytes_sent: bytes_sent as _,
                }
            }
            db::OutgoingPathStateEventData::Failed {
                status_code,
                bytes_sent,
            } => OutgoingPathStateKind::Failed {
                status: status_code,
                bytes_sent: bytes_sent as _,
            },
            db::OutgoingPathStateEventData::Completed => OutgoingPathStateKind::Completed,
            db::OutgoingPathStateEventData::Rejected {
                by_peer,
                bytes_sent,
            } => OutgoingPathStateKind::Rejected {
                by_peer,
                bytes_sent: bytes_sent as _,
            },
            db::OutgoingPathStateEventData::Paused { bytes_sent } => {
                OutgoingPathStateKind::Paused {
                    bytes_sent: bytes_sent as _,
                }
            }
        }
    }
}

impl From<db::OutgoingPathStateEvent> for OutgoingPathState {
    fn from(state: db::OutgoingPathStateEvent) -> Self {
        OutgoingPathState {
            created_at: state.created_at.and_utc().timestamp_millis(),
            kind: state.data.into(),
        }
    }
}

impl From<db::OutgoingPath> for OutgoingPath {
    fn from(path: db::OutgoingPath) -> Self {
        OutgoingPath {
            file_id: path.file_id,
            relative_path: path.relative_path,
            bytes: path.bytes as _,
            bytes_sent: path.bytes_sent as _,
            source: match path.content_uri {
                Some(uri) => OutgoingFileSource::ContentUri {
                    uri: uri.to_string(),
                },
                None => OutgoingFileSource::BasePath {
                    base_path: path
                        .base_path
                        .and_then(|p| p.to_str().map(|s| s.to_owned()))
                        .unwrap_or_default(),
                },
            },
            states: path
                .states
                .into_iter()
                .map(OutgoingPathState::from)
                .collect(),
        }
    }
}

impl From<db::DbTransferType> for TransferKind {
    fn from(value: db::DbTransferType) -> Self {
        match value {
            drop_storage::types::DbTransferType::Incoming(paths) => TransferKind::Incoming {
                paths: paths.into_iter().map(IncomingPath::from).collect(),
            },
            drop_storage::types::DbTransferType::Outgoing(paths) => TransferKind::Outgoing {
                paths: paths.into_iter().map(OutgoingPath::from).collect(),
            },
        }
    }
}
