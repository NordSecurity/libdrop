use std::time::SystemTime;

use drop_transfer::{utils::Hidden, File as _, Transfer};
use serde::{Deserialize, Serialize};

use super::utils;

#[derive(Deserialize, Debug)]
pub struct TransferDescriptor {
    pub path: Hidden<String>,
    pub content_uri: Option<url::Url>,
    pub fd: Option<i32>,
}

#[derive(Serialize)]
pub struct EventTransferRequest {
    peer: String,
    transfer: String,
    files: Vec<File>,
    timestamp: u64,
}

#[derive(Serialize)]
pub struct EventRequestQueued {
    transfer: String,
    files: Vec<File>,
    timestamp: u64,
}

#[derive(Serialize)]
struct File {
    id: String,
    path: String,
    size: u64,
}

#[derive(Serialize)]
pub struct StartEvent {
    transfer: String,
    file: String,
    transfered: u64,
    timestamp: u64,
}

#[derive(Serialize)]
pub struct ProgressEvent {
    transfer: String,
    file: String,
    transfered: u64,
    timestamp: u64,
}

#[derive(Serialize)]
pub struct Status {
    status: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_error_code: Option<i32>,
}

#[derive(Serialize)]
#[serde(tag = "reason", content = "data")]
pub enum FinishEvent {
    TransferCanceled {
        by_peer: bool,
    },
    FileDownloaded {
        file: String,
        final_path: String,
    },
    FileUploaded {
        file: String,
    },
    FileFailed {
        file: String,
        #[serde(flatten)]
        status: Status,
    },
    TransferFailed {
        #[serde(flatten)]
        status: Status,
    },
    FileRejected {
        file: String,
        by_peer: bool,
    },
}

#[derive(serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    RequestReceived(EventTransferRequest),
    RequestQueued(EventRequestQueued),
    TransferStarted(StartEvent),
    TransferProgress(ProgressEvent),
    TransferFinished {
        transfer: String,
        #[serde(flatten)]
        data: FinishEvent,
        timestamp: u64,
    },
    RuntimeError {
        status: drop_core::Status,
        timestamp: u64,
    },
    TransferPaused {
        transfer: String,
        file: String,
        timestamp: u64,
    },
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    pub moose_event_path: String,
    pub moose_prod: bool,
    pub storage_path: String,
}

impl From<&drop_transfer::Error> for Status {
    fn from(value: &drop_transfer::Error) -> Self {
        Self {
            status: value.into(),
            os_error_code: value.os_err_code(),
        }
    }
}

impl From<(drop_transfer::Event, SystemTime)> for Event {
    fn from(event: (drop_transfer::Event, SystemTime)) -> Self {
        let (e, timestamp) = event;
        let timestamp = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        match e {
            drop_transfer::Event::RequestReceived(tx) => Event::RequestReceived(tx.as_ref().into()),
            drop_transfer::Event::RequestQueued(tx) => Event::RequestQueued(tx.as_ref().into()),
            drop_transfer::Event::FileUploadStarted(tx, fid, transfered) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered,
                    timestamp,
                })
            }
            drop_transfer::Event::FileDownloadStarted(tx, fid, _, transfered) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered,
                    timestamp,
                })
            }
            drop_transfer::Event::FileUploadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered: progress,
                    timestamp,
                })
            }
            drop_transfer::Event::FileDownloadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered: progress,
                    timestamp,
                })
            }
            drop_transfer::Event::FileUploadSuccess(tx, fid) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileUploaded {
                    file: fid.to_string(),
                },
                timestamp,
            },
            drop_transfer::Event::FileDownloadSuccess(tx, info) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileDownloaded {
                    file: info.id.to_string(),
                    final_path: info.final_path.0.to_string_lossy().to_string(),
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.to_string(),
                    status: From::from(&status),
                },
                timestamp,
            },
            drop_transfer::Event::FileDownloadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.to_string(),
                    status: From::from(&status),
                },
                timestamp,
            },
            drop_transfer::Event::IncomingTransferCanceled(tx, by_peer) => {
                Event::TransferFinished {
                    transfer: tx.id().to_string(),
                    data: FinishEvent::TransferCanceled { by_peer },
                    timestamp,
                }
            }
            drop_transfer::Event::OutgoingTransferCanceled(tx, by_peer) => {
                Event::TransferFinished {
                    transfer: tx.id().to_string(),
                    data: FinishEvent::TransferCanceled { by_peer },
                    timestamp,
                }
            }
            drop_transfer::Event::OutgoingTransferFailed(tx, status, _) => {
                Event::TransferFinished {
                    transfer: tx.id().to_string(),
                    data: FinishEvent::TransferFailed {
                        status: From::from(&status),
                    },
                    timestamp,
                }
            }
            drop_transfer::Event::FileDownloadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => Event::TransferFinished {
                transfer: transfer_id.to_string(),
                data: FinishEvent::FileRejected {
                    file: file_id.to_string(),
                    by_peer,
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => Event::TransferFinished {
                transfer: transfer_id.to_string(),
                data: FinishEvent::FileRejected {
                    file: file_id.to_string(),
                    by_peer,
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadPaused {
                transfer_id,
                file_id,
            } => Self::TransferPaused {
                transfer: transfer_id.to_string(),
                file: file_id.to_string(),
                timestamp,
            },
            drop_transfer::Event::FileDownloadPaused {
                transfer_id,
                file_id,
            } => Self::TransferPaused {
                transfer: transfer_id.to_string(),
                file: file_id.to_string(),
                timestamp,
            },
        }
    }
}

impl<T: drop_transfer::Transfer> From<&T> for EventTransferRequest {
    fn from(t: &T) -> EventTransferRequest {
        EventTransferRequest {
            peer: t.peer().to_string(),
            transfer: t.id().to_string(),
            files: extract_transfer_files(t),
            timestamp: utils::current_timestamp(),
        }
    }
}

impl<T: drop_transfer::Transfer> From<&T> for EventRequestQueued {
    fn from(t: &T) -> EventRequestQueued {
        EventRequestQueued {
            transfer: t.id().to_string(),
            files: extract_transfer_files(t),
            timestamp: utils::current_timestamp(),
        }
    }
}

fn extract_transfer_files(t: &impl drop_transfer::Transfer) -> Vec<File> {
    t.files()
        .values()
        .map(|f| File {
            id: f.id().to_string(),
            path: f.subpath().to_string(),
            size: f.size(),
        })
        .collect()
}

impl From<Config> for drop_config::Config {
    fn from(val: Config) -> Self {
        let Config {
            dir_depth_limit,
            transfer_file_limit,
            moose_event_path,
            moose_prod,
            storage_path,
        } = val;

        drop_config::Config {
            drop: drop_config::DropConfig {
                dir_depth_limit,
                transfer_file_limit,
                storage_path,
            },
            moose: drop_config::MooseConfig {
                event_path: moose_event_path,
                prod: moose_prod,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_config() {
        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "transfer_idle_lifetime_ms": 2000,
          "connection_max_retry_interval_ms": 500,
          "moose_event_path": "test/path",
          "moose_prod": true,
          "moose_app_version": "1.2.5",
          "storage_path": ":memory:",
          "max_uploads_in_flight": 16,
          "max_requests_per_sec": 15
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");

        let drop_config::Config {
            drop:
                drop_config::DropConfig {
                    dir_depth_limit,
                    transfer_file_limit,
                    storage_path,
                },
            moose: drop_config::MooseConfig { event_path, prod },
        } = cfg.into();

        assert_eq!(dir_depth_limit, 10);
        assert_eq!(transfer_file_limit, 100);
        assert_eq!(event_path, "test/path");
        assert_eq!(storage_path, ":memory:");
        assert!(prod);
    }
}
