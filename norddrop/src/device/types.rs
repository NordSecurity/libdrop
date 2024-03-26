use std::time::SystemTime;

use drop_transfer::{utils::Hidden, File as _, Transfer};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TransferDescriptor {
    pub path: Hidden<String>,
    pub content_uri: Option<url::Url>,
    pub fd: Option<i32>,
}

#[derive(Serialize, Deserialize)]
pub struct File {
    pub id: String,
    pub path: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Status {
    pub status: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_error_code: Option<i32>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "reason", content = "data")]
pub enum FinishEvent {
    TransferCanceled {
        by_peer: bool,
    },
    FileDownloaded {
        file_id: String,
        final_path: String,
    },
    FileUploaded {
        file_id: String,
    },
    FileFailed {
        file_id: String,
        #[serde(flatten)]
        status: Status,
    },
    TransferFailed {
        #[serde(flatten)]
        status: Status,
    },
    FileRejected {
        file_id: String,
        by_peer: bool,
    },
}

// TODO(msz): make timestamp consistent across events
// and make use of `SystemTime` since uniffi supports it
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    RequestReceived {
        peer: String,
        transfer_id: String,
        files: Vec<File>,
        timestamp: i64,
    },
    RequestQueued {
        peer: String,
        transfer_id: String,
        files: Vec<File>,
        timestamp: i64,
    },
    TransferStarted {
        transfer_id: String,
        file_id: String,
        transfered: u64,
        timestamp: i64,
    },
    TransferProgress {
        transfer_id: String,
        file_id: String,
        transfered: u64,
        timestamp: i64,
    },
    TransferFinished {
        transfer_id: String,
        #[serde(flatten)]
        data: FinishEvent,
        timestamp: i64,
    },
    RuntimeError {
        status: u32,
        timestamp: i64,
    },
    TransferPaused {
        transfer_id: String,
        file_id: String,
        timestamp: i64,
    },
    TransferThrottled {
        transfer_id: String,
        file_id: String,
        transfered: u64,
        timestamp: i64,
    },
    TransferDeferred {
        transfer_id: String,
        peer: String,
        #[serde(flatten)]
        status: Status,
    },
    TransferPending {
        transfer_id: String,
        file_id: String,
    },
    ChecksumStarted {
        transfer_id: String,
        file_id: String,
        size: u64,
        timestamp: i64,
    },
    ChecksumFinished {
        transfer_id: String,
        file_id: String,
        timestamp: i64,
    },
    ChecksumProgress {
        transfer_id: String,
        file_id: String,
        bytes_checksummed: u64,
        timestamp: i64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub dir_depth_limit: u64,
    pub transfer_file_limit: u64,
    pub moose_event_path: String,
    pub moose_prod: bool,
    pub storage_path: String,

    #[serde(rename = "checksum_events_size_threshold_bytes")]
    pub checksum_events_size_threshold: Option<u64>,

    #[serde(default = "Config::default_connection_retries")]
    pub connection_retries: u32,
}

impl Config {
    const fn default_connection_retries() -> u32 {
        5
    }
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
            .as_millis() as i64;

        match e {
            drop_transfer::Event::RequestReceived(tx) => Event::RequestReceived {
                peer: tx.peer().to_string(),
                transfer_id: tx.id().to_string(),
                files: extract_transfer_files(tx.as_ref()),
                timestamp,
            },
            drop_transfer::Event::RequestQueued(tx) => Event::RequestQueued {
                peer: tx.peer().to_string(),
                transfer_id: tx.id().to_string(),
                files: extract_transfer_files(tx.as_ref()),
                timestamp,
            },
            drop_transfer::Event::FileUploadStarted(tx, fid, transfered) => {
                Event::TransferStarted {
                    transfer_id: tx.id().to_string(),
                    file_id: fid.to_string(),
                    transfered,
                    timestamp,
                }
            }
            drop_transfer::Event::FileDownloadStarted(tx, fid, _, transfered) => {
                Event::TransferStarted {
                    transfer_id: tx.id().to_string(),
                    file_id: fid.to_string(),
                    transfered,
                    timestamp,
                }
            }
            drop_transfer::Event::FileUploadProgress(tx, fid, progress) => {
                Event::TransferProgress {
                    transfer_id: tx.id().to_string(),
                    file_id: fid.to_string(),
                    transfered: progress,
                    timestamp,
                }
            }
            drop_transfer::Event::FileDownloadProgress(tx, fid, progress) => {
                Event::TransferProgress {
                    transfer_id: tx.id().to_string(),
                    file_id: fid.to_string(),
                    transfered: progress,
                    timestamp,
                }
            }
            drop_transfer::Event::FileUploadSuccess(tx, fid) => Event::TransferFinished {
                transfer_id: tx.id().to_string(),
                data: FinishEvent::FileUploaded {
                    file_id: fid.to_string(),
                },
                timestamp,
            },
            drop_transfer::Event::FileDownloadSuccess(tx, info) => Event::TransferFinished {
                transfer_id: tx.id().to_string(),
                data: FinishEvent::FileDownloaded {
                    file_id: info.id.to_string(),
                    final_path: info.final_path.0.to_string_lossy().to_string(),
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadFailed(tx, fid, status) => Event::TransferFinished {
                transfer_id: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file_id: fid.to_string(),
                    status: From::from(&status),
                },
                timestamp,
            },
            drop_transfer::Event::FileDownloadFailed(tx, fid, status) => Event::TransferFinished {
                transfer_id: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file_id: fid.to_string(),
                    status: From::from(&status),
                },
                timestamp,
            },
            drop_transfer::Event::IncomingTransferCanceled(tx, by_peer) => {
                Event::TransferFinished {
                    transfer_id: tx.id().to_string(),
                    data: FinishEvent::TransferCanceled { by_peer },
                    timestamp,
                }
            }
            drop_transfer::Event::OutgoingTransferCanceled(tx, by_peer) => {
                Event::TransferFinished {
                    transfer_id: tx.id().to_string(),
                    data: FinishEvent::TransferCanceled { by_peer },
                    timestamp,
                }
            }
            drop_transfer::Event::OutgoingTransferFailed(tx, status, _) => {
                Event::TransferFinished {
                    transfer_id: tx.id().to_string(),
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
                transfer_id: transfer_id.to_string(),
                data: FinishEvent::FileRejected {
                    file_id: file_id.to_string(),
                    by_peer,
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => Event::TransferFinished {
                transfer_id: transfer_id.to_string(),
                data: FinishEvent::FileRejected {
                    file_id: file_id.to_string(),
                    by_peer,
                },
                timestamp,
            },
            drop_transfer::Event::FileUploadPaused {
                transfer_id,
                file_id,
            } => Self::TransferPaused {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                timestamp,
            },
            drop_transfer::Event::FileDownloadPaused {
                transfer_id,
                file_id,
            } => Self::TransferPaused {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                timestamp,
            },

            drop_transfer::Event::FileUploadThrottled {
                transfer_id,
                file_id,
                transfered,
            } => Self::TransferThrottled {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                transfered,
                timestamp,
            },
            drop_transfer::Event::ChecksumStarted {
                transfer_id,
                file_id,
                size,
            } => Self::ChecksumStarted {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                size,
                timestamp,
            },

            drop_transfer::Event::ChecksumFinished {
                transfer_id,
                file_id,
            } => Self::ChecksumFinished {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                timestamp,
            },

            drop_transfer::Event::ChecksumProgress {
                transfer_id,
                file_id,
                progress,
            } => Self::ChecksumProgress {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                bytes_checksummed: progress,
                timestamp,
            },
            drop_transfer::Event::OutgoingTransferDeferred { transfer, error } => {
                Self::TransferDeferred {
                    transfer_id: transfer.id().to_string(),
                    peer: transfer.peer().to_string(),
                    status: Status::from(&error),
                }
            }
            drop_transfer::Event::FileDownloadPending {
                transfer_id,
                file_id,
                ..
            } => Self::TransferPending {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
            },
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
            checksum_events_size_threshold,
            connection_retries,
        } = val;

        drop_config::Config {
            drop: drop_config::DropConfig {
                dir_depth_limit: dir_depth_limit as _,
                transfer_file_limit: transfer_file_limit as _,
                storage_path,
                checksum_events_size_threshold: checksum_events_size_threshold.map(|x| x as _),
                connection_retries,
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
          "max_requests_per_sec": 15,
          "checksum_events_size_threshold_bytes": 1234
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");

        let drop_config::Config {
            drop:
                drop_config::DropConfig {
                    dir_depth_limit,
                    transfer_file_limit,
                    storage_path,
                    checksum_events_size_threshold: checksum_events_size_threshold_bytes,
                    connection_retries,
                },
            moose: drop_config::MooseConfig { event_path, prod },
        } = cfg.into();

        assert_eq!(dir_depth_limit, 10);
        assert_eq!(transfer_file_limit, 100);
        assert_eq!(event_path, "test/path");
        assert_eq!(storage_path, ":memory:");
        assert!(prod);
        assert_eq!(checksum_events_size_threshold_bytes, Some(1234));
        assert_eq!(connection_retries, 5);
    }
}
