use std::collections::HashMap;

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
            drop_transfer::Event::FileDownloadStarted(tx, fid) => {
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
            id: f.name().to_string(),
            size: f.size().unwrap_or_default(),
            children: f
                .children()
                .map(|c| (c.name().to_string(), c.into()))
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
