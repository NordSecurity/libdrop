use std::collections::HashMap;

use drop_storage::{
    error::Error,
    types::{Event, Transfer},
    Storage, TransferType,
};

pub struct StorageDispatch<'a> {
    storage: &'a drop_storage::Storage,
    file_progress: HashMap<(String, String), i64>,
}

impl<'a> StorageDispatch<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self {
            storage,
            file_progress: HashMap::new(),
        }
    }

    pub async fn insert_transfer(
        &self,
        transfer_type: TransferType,
        transfer: &crate::Transfer,
    ) -> Result<(), Error> {
        self.storage
            .insert_transfer(transfer.storage_info(), transfer_type)
            .await?;
        Ok(())
    }

    pub async fn handle_event(&mut self, event: &crate::Event) -> Result<(), Error> {
        let event = Into::<Event>::into(event);
        match event {
            Event::Pending {
                transfer_type,
                transfer_info,
            } => match transfer_type {
                TransferType::Incoming => {
                    for file in transfer_info.files {
                        self.storage
                            .insert_incoming_path_pending_state(transfer_info.id.clone(), file.id)
                            .await?
                    }
                }
                TransferType::Outgoing => {
                    for file in transfer_info.files {
                        self.storage
                            .insert_outgoing_path_pending_state(transfer_info.id.clone(), file.id)
                            .await?
                    }
                }
            },

            Event::Started {
                transfer_type,
                transfer_id,
                file_id,
            } => match transfer_type {
                TransferType::Incoming => {
                    self.storage
                        .insert_incoming_path_started_state(transfer_id, file_id)
                        .await?
                }
                TransferType::Outgoing => {
                    self.storage
                        .insert_outgoing_path_started_state(transfer_id, file_id)
                        .await?
                }
            },

            Event::FileCanceled {
                transfer_type,
                transfer_id,
                file_id,
                by_peer,
            } => match transfer_type {
                TransferType::Incoming => {
                    let progress = self.get_file_progress(&transfer_id, &file_id);
                    self.storage
                        .insert_incoming_path_cancel_state(transfer_id, file_id, by_peer, progress)
                        .await?
                }
                TransferType::Outgoing => {
                    let progress = self.get_file_progress(&transfer_id, &file_id);
                    self.storage
                        .insert_outgoing_path_cancel_state(transfer_id, file_id, by_peer, progress)
                        .await?
                }
            },

            Event::FileDownloadComplete {
                transfer_id,
                file_id,
                final_path,
            } => {
                self.storage
                    .insert_incoming_path_completed_state(transfer_id, file_id, final_path)
                    .await?
            }

            Event::FileUploadComplete {
                transfer_id,
                file_id,
            } => {
                self.storage
                    .insert_outgoing_path_completed_state(transfer_id, file_id)
                    .await?
            }

            Event::TransferCanceled {
                transfer_type: _,
                transfer_info,
                by_peer,
            } => {
                self.storage
                    .insert_transfer_cancel_state(transfer_info.id, by_peer)
                    .await?
            }

            Event::TransferFailed {
                transfer_type: _,
                transfer_info,
                error_code,
            } => {
                self.storage
                    .insert_transfer_failed_state(transfer_info.id, error_code)
                    .await?
            }

            Event::FileFailed {
                transfer_type,
                transfer_id,
                file_id,
                error_code,
            } => {
                let progress = self.get_file_progress(&transfer_id, &file_id);
                match transfer_type {
                    TransferType::Incoming => {
                        self.storage
                            .insert_incoming_path_failed_state(
                                transfer_id,
                                file_id,
                                error_code,
                                progress,
                            )
                            .await?
                    }
                    TransferType::Outgoing => {
                        self.storage
                            .insert_outgoing_path_failed_state(
                                transfer_id,
                                file_id,
                                error_code,
                                progress,
                            )
                            .await?
                    }
                }
            }

            Event::Progress {
                transfer_id,
                file_id,
                progress,
            } => {
                *self
                    .file_progress
                    .entry((transfer_id, file_id))
                    .or_default() = progress;
            }
        }

        Ok(())
    }

    fn get_file_progress(&mut self, transfer_id: &String, file_id: &String) -> i64 {
        self.file_progress
            .remove(&(transfer_id.to_string(), file_id.to_string()))
            .unwrap_or(0)
    }

    pub async fn purge_transfers(&self, transfer_ids: Vec<String>) -> Result<(), Error> {
        self.storage.purge_transfers(transfer_ids).await
    }

    pub async fn purge_transfers_until(&self, until_timestamp: i64) -> Result<(), Error> {
        self.storage.purge_transfers_until(until_timestamp).await
    }

    pub async fn get_transfers(&self, since_timestamp: i64) -> Result<Vec<Transfer>, Error> {
        self.storage.get_transfers(since_timestamp).await
    }
}

impl From<&crate::Event> for Event {
    fn from(event: &crate::Event) -> Self {
        match event {
            crate::Event::RequestReceived(transfer) => Event::Pending {
                transfer_type: TransferType::Incoming,
                transfer_info: transfer.storage_info(),
            },
            crate::Event::RequestQueued(transfer) => Event::Pending {
                transfer_type: TransferType::Outgoing,
                transfer_info: transfer.storage_info(),
            },
            crate::Event::FileDownloadStarted(transfer, file) => Event::Started {
                transfer_type: TransferType::Incoming,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
            },
            crate::Event::FileUploadStarted(transfer, file) => Event::Started {
                transfer_type: TransferType::Outgoing,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
            },
            crate::Event::FileDownloadCancelled(transfer, file, by_peer) => Event::FileCanceled {
                transfer_type: TransferType::Incoming,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                by_peer: *by_peer,
            },
            crate::Event::FileUploadCancelled(transfer, file, by_peer) => Event::FileCanceled {
                transfer_type: TransferType::Outgoing,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                by_peer: *by_peer,
            },
            crate::Event::FileDownloadSuccess(transfer, file) => Event::FileDownloadComplete {
                transfer_id: transfer.id().to_string(),
                file_id: file.id.to_string(),
                final_path: file.final_path.to_string_lossy().to_string(),
            },
            crate::Event::FileUploadSuccess(transfer, file) => Event::FileUploadComplete {
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
            },
            crate::Event::FileDownloadFailed(transfer, file, error) => Event::FileFailed {
                transfer_type: TransferType::Incoming,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                error_code: error.into(),
            },
            crate::Event::FileUploadFailed(transfer, file, error) => Event::FileFailed {
                transfer_type: TransferType::Outgoing,
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                error_code: error.into(),
            },
            crate::Event::TransferCanceled(transfer, is_sender, by_peer) => {
                let transfer_type = match is_sender {
                    false => TransferType::Incoming,
                    true => TransferType::Outgoing,
                };

                Event::TransferCanceled {
                    transfer_type,
                    transfer_info: transfer.storage_info(),
                    by_peer: *by_peer,
                }
            }
            crate::Event::TransferFailed(transfer, error, by_peer) => {
                let transfer_type = match by_peer {
                    false => TransferType::Outgoing,
                    true => TransferType::Incoming,
                };

                Event::TransferFailed {
                    transfer_type,
                    transfer_info: transfer.storage_info(),
                    error_code: error.into(),
                }
            }
            crate::Event::FileDownloadProgress(transfer, file, progress) => Event::Progress {
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                progress: *progress as i64,
            },
            crate::Event::FileUploadProgress(transfer, file, progress) => Event::Progress {
                transfer_id: transfer.id().to_string(),
                file_id: file.to_string(),
                progress: *progress as i64,
            },
        }
    }
}
