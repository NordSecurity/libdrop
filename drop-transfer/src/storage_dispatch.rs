use std::collections::HashMap;

pub struct StorageDispatch {
    storage: drop_storage::Storage,
    file_progress: HashMap<(String, String), i64>,
}

impl StorageDispatch {
    pub async fn new(
        logger: slog::Logger,
        storage_path: &str,
    ) -> Result<Self, drop_storage::error::Error> {
        Ok(Self {
            storage: drop_storage::Storage::new(logger, storage_path).await?,
            file_progress: HashMap::new(),
        })
    }

    pub async fn insert_transfer(
        &self,
        transfer_type: drop_storage::TransferType,
        transfer: &crate::Transfer,
    ) -> Result<(), drop_storage::error::Error> {
        self.storage
            .insert_transfer(transfer.storage_info(), transfer_type)
            .await?;
        Ok(())
    }

    pub async fn handle_event(
        &mut self,
        event: &crate::Event,
    ) -> Result<(), drop_storage::error::Error> {
        let event = Into::<drop_storage::types::Event>::into(event);
        match event {
            drop_storage::types::Event::Pending(transfer_type, transfer_info) => {
                match transfer_type {
                    drop_storage::types::TransferType::Incoming => {
                        for file in transfer_info.files {
                            self.storage
                                .insert_incoming_path_pending_state(
                                    transfer_info.id.clone(),
                                    file.id,
                                )
                                .await?
                        }
                    }
                    drop_storage::types::TransferType::Outgoing => {
                        for file in transfer_info.files {
                            self.storage
                                .insert_outgoing_path_pending_state(
                                    transfer_info.id.clone(),
                                    file.id,
                                )
                                .await?
                        }
                    }
                }
            }

            drop_storage::types::Event::Started(transfer_type, transfer_id, file_id) => {
                match transfer_type {
                    drop_storage::types::TransferType::Incoming => {
                        self.storage
                            .insert_incoming_path_started_state(transfer_id, file_id)
                            .await?
                    }
                    drop_storage::types::TransferType::Outgoing => {
                        self.storage
                            .insert_outgoing_path_started_state(transfer_id, file_id)
                            .await?
                    }
                }
            }

            drop_storage::types::Event::FileCanceled(
                transfer_type,
                transfer_id,
                file_id,
                by_peer,
            ) => match transfer_type {
                drop_storage::types::TransferType::Incoming => {
                    let progress = self.get_file_progress(&transfer_id, &file_id);
                    self.storage
                        .insert_incoming_path_cancel_state(transfer_id, file_id, by_peer, progress)
                        .await?
                }
                drop_storage::types::TransferType::Outgoing => {
                    let progress = self.get_file_progress(&transfer_id, &file_id);
                    self.storage
                        .insert_outgoing_path_cancel_state(transfer_id, file_id, by_peer, progress)
                        .await?
                }
            },

            drop_storage::types::Event::FileDownloadComplete(transfer_id, file_id, final_path) => {
                self.storage
                    .insert_incoming_path_completed_state(transfer_id, file_id, final_path)
                    .await?
            }

            drop_storage::types::Event::FileUploadComplete(transfer_id, file_id) => {
                self.storage
                    .insert_outgoing_path_completed_state(transfer_id, file_id)
                    .await?
            }

            drop_storage::types::Event::TransferCanceled(_, transfer_info, by_peer) => {
                self.storage
                    .insert_transfer_cancel_state(transfer_info.id, by_peer)
                    .await?
            }

            drop_storage::types::Event::TransferFailed(_, transfer_info, error_code) => {
                self.storage
                    .insert_transfer_failed_state(transfer_info.id, error_code)
                    .await?
            }

            drop_storage::types::Event::FileFailed(
                transfer_type,
                transfer_id,
                file_id,
                error_code,
            ) => {
                let progress = self.get_file_progress(&transfer_id, &file_id);
                match transfer_type {
                    drop_storage::types::TransferType::Incoming => {
                        self.storage
                            .insert_incoming_path_failed_state(
                                transfer_id,
                                file_id,
                                error_code,
                                progress,
                            )
                            .await?
                    }
                    drop_storage::types::TransferType::Outgoing => {
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

            drop_storage::types::Event::Progress(transfer_id, file_id, progress) => {
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

    pub async fn get_transfers(
        &self,
    ) -> Result<Vec<drop_storage::types::Transfer>, drop_storage::error::Error> {
        self.storage.get_transfers().await
    }
}

impl From<&crate::Event> for drop_storage::types::Event {
    fn from(event: &crate::Event) -> Self {
        match event {
            crate::Event::RequestReceived(transfer) => drop_storage::types::Event::Pending(
                drop_storage::types::TransferType::Incoming,
                transfer.storage_info(),
            ),
            crate::Event::RequestQueued(transfer) => drop_storage::types::Event::Pending(
                drop_storage::types::TransferType::Outgoing,
                transfer.storage_info(),
            ),
            crate::Event::FileDownloadStarted(transfer, file) => {
                drop_storage::types::Event::Started(
                    drop_storage::types::TransferType::Incoming,
                    transfer.id().to_string(),
                    file.to_string(),
                )
            }
            crate::Event::FileUploadStarted(transfer, file) => drop_storage::types::Event::Started(
                drop_storage::types::TransferType::Outgoing,
                transfer.id().to_string(),
                file.to_string(),
            ),
            crate::Event::FileDownloadCancelled(transfer, file, by_peer) => {
                drop_storage::types::Event::FileCanceled(
                    drop_storage::types::TransferType::Incoming,
                    transfer.id().to_string(),
                    file.to_string(),
                    *by_peer,
                )
            }
            crate::Event::FileUploadCancelled(transfer, file, by_peer) => {
                drop_storage::types::Event::FileCanceled(
                    drop_storage::types::TransferType::Outgoing,
                    transfer.id().to_string(),
                    file.to_string(),
                    *by_peer,
                )
            }
            crate::Event::FileDownloadSuccess(transfer, file) => {
                drop_storage::types::Event::FileDownloadComplete(
                    transfer.id().to_string(),
                    file.id.to_string(),
                    file.final_path.to_string_lossy().to_string(),
                )
            }
            crate::Event::FileUploadSuccess(transfer, file) => {
                drop_storage::types::Event::FileUploadComplete(
                    transfer.id().to_string(),
                    file.to_string(),
                )
            }
            crate::Event::FileDownloadFailed(transfer, file, error) => {
                drop_storage::types::Event::FileFailed(
                    drop_storage::types::TransferType::Incoming,
                    transfer.id().to_string(),
                    file.to_string(),
                    error.into(),
                )
            }
            crate::Event::FileUploadFailed(transfer, file, error) => {
                drop_storage::types::Event::FileFailed(
                    drop_storage::types::TransferType::Outgoing,
                    transfer.id().to_string(),
                    file.to_string(),
                    error.into(),
                )
            }
            crate::Event::TransferCanceled(transfer, is_sender, by_peer) => {
                let transfer_type = match is_sender {
                    false => drop_storage::types::TransferType::Incoming,
                    true => drop_storage::types::TransferType::Outgoing,
                };

                drop_storage::types::Event::TransferCanceled(
                    transfer_type,
                    transfer.storage_info(),
                    *by_peer,
                )
            }
            crate::Event::TransferFailed(transfer, error, by_peer) => {
                let transfer_type = match by_peer {
                    false => drop_storage::types::TransferType::Outgoing,
                    true => drop_storage::types::TransferType::Incoming,
                };

                drop_storage::types::Event::TransferFailed(
                    transfer_type,
                    transfer.storage_info(),
                    error.into(),
                )
            }
            crate::Event::FileDownloadProgress(transfer, file, progress) => {
                drop_storage::types::Event::Progress(
                    transfer.id().to_string(),
                    file.to_string(),
                    *progress as i64,
                )
            }
            crate::Event::FileUploadProgress(transfer, file, progress) => {
                drop_storage::types::Event::Progress(
                    transfer.id().to_string(),
                    file.to_string(),
                    *progress as i64,
                )
            }
        }
    }
}
