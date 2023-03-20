use std::{collections::HashMap, time::Duration};

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
    size: u64,
    children: HashMap<String, File>,
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
#[serde(tag = "reason", content = "data")]
pub enum FinishEvent {
    TransferCanceled { by_peer: bool },
    FileDownloaded { file: String, final_path: String },
    FileUploaded { file: String },
    FileCanceled { file: String, by_peer: bool },
    FileFailed { file: String, status: u32 },
    TransferFailed { status: u32 },
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
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    pub req_connection_timeout_ms: u64,
    #[serde(default = "default_connection_max_retry_interval_ms")]
    pub connection_max_retry_interval_ms: u64,
    pub transfer_idle_lifetime_ms: u64,
    pub moose_event_path: String,
    pub moose_prod: bool,
}

const fn default_connection_max_retry_interval_ms() -> u64 {
    10000
}

impl From<drop_transfer::Event> for Event {
    fn from(e: drop_transfer::Event) -> Self {
        match e {
            drop_transfer::Event::RequestReceived(tx) => Event::RequestReceived(tx.into()),
            drop_transfer::Event::RequestQueued(tx) => Event::RequestQueued(tx.into()),
            drop_transfer::Event::FileUploadStarted(tx, fid) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.0.to_string_lossy().to_string(),
                })
            }
            drop_transfer::Event::FileDownloadStarted(tx, fid) => {
                Event::TransferStarted(StartEvent {
                    transfer: tx.id().to_string(),
                    file: fid.0.to_string_lossy().to_string(),
                })
            }
            drop_transfer::Event::FileUploadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.0.to_string_lossy().to_string(),
                    transfered: progress,
                })
            }
            drop_transfer::Event::FileDownloadProgress(tx, fid, progress) => {
                Event::TransferProgress(ProgressEvent {
                    transfer: tx.id().to_string(),
                    file: fid.0.to_string_lossy().to_string(),
                    transfered: progress,
                })
            }
            drop_transfer::Event::FileUploadSuccess(tx, fid) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileUploaded {
                    file: fid.0.to_string_lossy().to_string(),
                },
            },
            drop_transfer::Event::FileDownloadSuccess(tx, info) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileDownloaded {
                    file: info.id.0.to_string_lossy().to_string(),
                    final_path: info.final_path.0.to_string_lossy().to_string(),
                },
            },
            drop_transfer::Event::FileUploadCancelled(tx, fid) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileCanceled {
                    file: fid.0.to_string_lossy().to_string(),
                    by_peer: true,
                },
            },
            drop_transfer::Event::FileDownloadCancelled(tx, fid) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileCanceled {
                    file: fid.0.to_string_lossy().to_string(),
                    by_peer: false,
                },
            },
            drop_transfer::Event::FileUploadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.0.to_string_lossy().to_string(),
                    status: From::from(&status),
                },
            },
            drop_transfer::Event::FileDownloadFailed(tx, fid, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::FileFailed {
                    file: fid.0.to_string_lossy().to_string(),
                    status: From::from(&status),
                },
            },
            drop_transfer::Event::TransferCanceled(tx, by_peer) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::TransferCanceled { by_peer },
            },
            drop_transfer::Event::TransferFailed(tx, status) => Event::TransferFinished {
                transfer: tx.id().to_string(),
                data: FinishEvent::TransferFailed {
                    status: From::from(&status),
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
            files: t.files().iter().map(|(_, v)| v.into()).collect(),
        }
    }
}

impl From<&drop_transfer::File> for File {
    fn from(f: &drop_transfer::File) -> Self {
        Self {
            id: f.name().unwrap_or_default(),
            size: f.size().unwrap_or_default(),
            children: f
                .children()
                .map(|c| (c.name().unwrap_or_default(), c.into()))
                .collect(),
        }
    }
}

impl From<drop_transfer::Transfer> for EventRequestQueued {
    fn from(t: drop_transfer::Transfer) -> EventRequestQueued {
        EventRequestQueued {
            transfer: t.id().to_string(),
            files: t.files().iter().map(|(_, v)| v.into()).collect(),
        }
    }
}

impl From<Config> for drop_config::Config {
    fn from(val: Config) -> Self {
        let Config {
            dir_depth_limit,
            transfer_file_limit,
            req_connection_timeout_ms,
            connection_max_retry_interval_ms,
            transfer_idle_lifetime_ms,
            moose_event_path,
            moose_prod,
        } = val;

        drop_config::Config {
            drop: drop_config::DropConfig {
                dir_depth_limit,
                transfer_file_limit,
                req_connection_timeout: Duration::from_millis(req_connection_timeout_ms),
                connection_max_retry_interval: Duration::from_millis(
                    connection_max_retry_interval_ms,
                ),
                transfer_idle_lifetime: Duration::from_millis(transfer_idle_lifetime_ms),
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
        // Without `connection_max_retry_interval_ms`
        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "req_connection_timeout_ms": 1000,
          "transfer_idle_lifetime_ms": 2000,
          "moose_event_path": "test/path",
          "moose_prod": true
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
          "moose_prod": true
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");

        let drop_config::Config {
            drop:
                drop_config::DropConfig {
                    dir_depth_limit,
                    transfer_file_limit,
                    req_connection_timeout,
                    transfer_idle_lifetime,
                    connection_max_retry_interval,
                },
            moose: drop_config::MooseConfig { event_path, prod },
        } = cfg.into();

        assert_eq!(dir_depth_limit, 10);
        assert_eq!(transfer_file_limit, 100);
        assert_eq!(req_connection_timeout, Duration::from_millis(1000));
        assert_eq!(connection_max_retry_interval, Duration::from_millis(500));
        assert_eq!(transfer_idle_lifetime, Duration::from_millis(2000));
        assert_eq!(event_path, "test/path");
        assert!(prod);
    }
}
