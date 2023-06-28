use std::time::Duration;

use drop_transfer::utils::Hidden;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct TransferDescriptor {
    pub path: Hidden<String>,
    pub fd: Option<i32>,
}

#[derive(Serialize)]
pub struct EventTransferRequest {
    peer: String,
    transfer: String,
    files: Vec<File>,
}

#[derive(Serialize)]
pub struct EventRequestQueued {
    transfer: String,
    files: Vec<File>,
}

#[derive(Serialize)]
pub struct EventTransfer {
    transfer: String,
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
}

#[derive(Serialize)]
pub struct CancelEvent {
    transfer: String,
    files: Vec<String>,
}

#[derive(Serialize)]
pub struct ProgressEvent {
    transfer: String,
    file: String,
    transfered: u64,
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
    FileCanceled {
        file: String,
        by_peer: bool,
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
    },
    RuntimeError {
        status: drop_core::Status,
    },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    #[serde(default = "default_connection_max_retry_interval_ms")]
    pub connection_max_retry_interval_ms: u64,
    pub transfer_idle_lifetime_ms: u64,
    pub moose_event_path: String,
    pub moose_prod: bool,
    pub storage_path: String,
    pub tracker_context: String,
}

const fn default_connection_max_retry_interval_ms() -> u64 {
    10000
}

impl From<&drop_transfer::Error> for Status {
    fn from(value: &drop_transfer::Error) -> Self {
        Self {
            status: value.into(),
            os_error_code: value.os_err_code(),
        }
    }
}

impl From<drop_transfer::Event> for Event {
    fn from(e: drop_transfer::Event) -> Self {
        match e {
            drop_transfer::Event::RequestReceived(tx) => Event::RequestReceived(tx.into()),
            drop_transfer::Event::RequestQueued(tx) => Event::RequestQueued(tx.into()),
            drop_transfer::Event::FileUploadStarted(tx, fid) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                })
            }
            drop_transfer::Event::FileDownloadStarted(tx, fid, _) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                })
            }
            drop_transfer::Event::FileUploadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered: progress,
                })
            }
            drop_transfer::Event::FileDownloadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.to_string(),
                    transfered: progress,
                })
            }
            drop_transfer::Event::FileUploadSuccess(tx, fid) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileUploaded {
                    file: fid.to_string(),
                },
            },
            drop_transfer::Event::FileDownloadSuccess(tx, info) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileDownloaded {
                    file: info.id.to_string(),
                    final_path: info.final_path.0.to_string_lossy().to_string(),
                },
            },
            drop_transfer::Event::FileUploadCancelled(tx, fid, by_peer) => {
                Event::TransferFinished {
                    transfer: tx.id().to_string(),
                    data: FinishEvent::FileCanceled {
                        file: fid.to_string(),
                        by_peer,
                    },
                }
            }
            drop_transfer::Event::FileDownloadCancelled(tx, fid, by_peer) => {
                Event::TransferFinished {
                    transfer: tx.id().to_string(),
                    data: FinishEvent::FileCanceled {
                        file: fid.to_string(),
                        by_peer,
                    },
                }
            }
            drop_transfer::Event::FileUploadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.to_string(),
                    status: From::from(&status),
                },
            },
            drop_transfer::Event::FileDownloadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.to_string(),
                    status: From::from(&status),
                },
            },
            drop_transfer::Event::TransferCanceled(tx, _, by_peer) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::TransferCanceled { by_peer },
            },
            drop_transfer::Event::TransferFailed(tx, status, _) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::TransferFailed {
                    status: From::from(&status),
                },
            },
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
            },
        }
    }
}

impl From<drop_transfer::Transfer> for EventTransfer {
    fn from(t: drop_transfer::Transfer) -> EventTransfer {
        EventTransfer {
            transfer: t.id().to_string(),
        }
    }
}

impl From<drop_transfer::Transfer> for EventTransferRequest {
    fn from(t: drop_transfer::Transfer) -> EventTransferRequest {
        EventTransferRequest {
            peer: t.peer().to_string(),
            transfer: t.id().to_string(),
            files: extract_transfer_files(&t),
        }
    }
}

impl From<drop_transfer::Transfer> for EventRequestQueued {
    fn from(t: drop_transfer::Transfer) -> EventRequestQueued {
        EventRequestQueued {
            transfer: t.id().to_string(),
            files: extract_transfer_files(&t),
        }
    }
}

fn extract_transfer_files(t: &drop_transfer::Transfer) -> Vec<File> {
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
            connection_max_retry_interval_ms,
            transfer_idle_lifetime_ms,
            moose_event_path,
            moose_prod,
            storage_path,
            tracker_context,
        } = val;

        drop_config::Config {
            drop: drop_config::DropConfig {
                dir_depth_limit,
                transfer_file_limit,
                connection_max_retry_interval: Duration::from_millis(
                    connection_max_retry_interval_ms,
                ),
                transfer_idle_lifetime: Duration::from_millis(transfer_idle_lifetime_ms),
                storage_path,
            },
            moose: drop_config::MooseConfig {
                event_path: moose_event_path,
                prod: moose_prod,
                tracker_context,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_config() {
        // Without `connection_max_retry_interval_ms`
        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "req_connection_timeout_ms": 1000,
          "transfer_idle_lifetime_ms": 2000,
          "moose_event_path": "test/path",
          "moose_prod": true,
          "storage_path": ":memory:",
          "tracker_context": "test"
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");
        assert_eq!(cfg.connection_max_retry_interval_ms, 10000);

        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "req_connection_timeout_ms": 1000,
          "transfer_idle_lifetime_ms": 2000,
          "connection_max_retry_interval_ms": 500,
          "moose_event_path": "test/path",
          "moose_prod": true,
          "storage_path": ":memory:",
          "tracker_context": "test"
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");

        let drop_config::Config {
            drop:
                drop_config::DropConfig {
                    dir_depth_limit,
                    transfer_file_limit,
                    transfer_idle_lifetime,
                    connection_max_retry_interval,
                    storage_path,
                },
            moose:
                drop_config::MooseConfig {
                    event_path,
                    prod,
                    tracker_context,
                },
        } = cfg.into();

        assert_eq!(dir_depth_limit, 10);
        assert_eq!(transfer_file_limit, 100);
        assert_eq!(connection_max_retry_interval, Duration::from_millis(500));
        assert_eq!(transfer_idle_lifetime, Duration::from_millis(2000));
        assert_eq!(event_path, "test/path");
        assert_eq!(storage_path, ":memory:");
        assert_eq!(tracker_context, "test");
        assert!(prod);
    }
}
