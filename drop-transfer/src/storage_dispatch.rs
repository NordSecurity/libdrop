pub async fn handle_event(
    storage: std::sync::Arc<drop_storage::Storage>,
    event: &crate::Event,
) -> Result<(), drop_storage::error::Error> {
    let event = Into::<drop_storage::types::Event>::into(event);
    match event {
        drop_storage::types::Event::Pending(transfer_type, transfer_info) => match transfer_type {
            drop_storage::types::TransferType::Incoming => {
                for file in transfer_info.files {
                    storage
                        .insert_incoming_path_pending_state(transfer_info.id.clone(), file.path)
                        .await?
                }
            }
            drop_storage::types::TransferType::Outgoing => {
                for file in transfer_info.files {
                    storage
                        .insert_outgoing_path_pending_state(transfer_info.id.clone(), file.path)
                        .await?
                }
            }
        },

        drop_storage::types::Event::Started(transfer_type, transfer_id, file_id) => {
            match transfer_type {
                drop_storage::types::TransferType::Incoming => {
                    storage
                        .insert_incoming_path_started_state(transfer_id, file_id)
                        .await?
                }
                drop_storage::types::TransferType::Outgoing => {
                    storage
                        .insert_outgoing_path_started_state(transfer_id, file_id)
                        .await?
                }
            }
        }

        drop_storage::types::Event::FileCanceled(transfer_type, transfer_id, file_id) => {
            match transfer_type {
                drop_storage::types::TransferType::Incoming => {
                    storage
                        .insert_incoming_path_cancel_state(transfer_id, file_id, false, 0)
                        .await?
                }
                drop_storage::types::TransferType::Outgoing => {
                    storage
                        .insert_outgoing_path_cancel_state(transfer_id, file_id, false, 0)
                        .await?
                }
            }
        }

        drop_storage::types::Event::FileDownloadComplete(transfer_id, file_id, final_path) => {
            storage
                .insert_incoming_path_completed_state(transfer_id, file_id, final_path)
                .await?
        }

        drop_storage::types::Event::FileUploadComplete(transfer_id, file_id) => {
            storage
                .insert_outgoing_path_completed_state(transfer_id, file_id)
                .await?
        }

        drop_storage::types::Event::TransferCanceled(transfer_type, transfer_info, by_peer) => {
            match transfer_type {
                drop_storage::types::TransferType::Incoming => {
                    for file in transfer_info.files {
                        storage
                            .insert_incoming_path_cancel_state(
                                transfer_info.id.clone(),
                                file.path,
                                by_peer,
                                0,
                            )
                            .await?
                    }
                }
                drop_storage::types::TransferType::Outgoing => {
                    for file in transfer_info.files {
                        storage
                            .insert_outgoing_path_cancel_state(
                                transfer_info.id.clone(),
                                file.path,
                                by_peer,
                                0,
                            )
                            .await?
                    }
                }
            }
        }

        drop_storage::types::Event::TransferFailed(transfer_type, transfer_info, error_code) => {
            match transfer_type {
                drop_storage::types::TransferType::Incoming => {
                    for file in transfer_info.files {
                        storage
                            .insert_incoming_path_failed_state(
                                transfer_info.id.clone(),
                                file.path,
                                error_code,
                                0,
                            )
                            .await?
                    }
                }
                drop_storage::types::TransferType::Outgoing => {
                    for file in transfer_info.files {
                        storage
                            .insert_outgoing_path_failed_state(
                                transfer_info.id.clone(),
                                file.path,
                                error_code,
                                0,
                            )
                            .await?
                    }
                }
            }
        }

        drop_storage::types::Event::FileFailed(
            transfer_type,
            transfer_id,
            file_id,
            error_code,
            bytes_transferred,
        ) => match transfer_type {
            drop_storage::types::TransferType::Incoming => {
                storage
                    .insert_incoming_path_failed_state(
                        transfer_id,
                        file_id,
                        error_code,
                        bytes_transferred,
                    )
                    .await?
            }
            drop_storage::types::TransferType::Outgoing => {
                storage
                    .insert_outgoing_path_failed_state(
                        transfer_id,
                        file_id,
                        error_code,
                        bytes_transferred,
                    )
                    .await?
            }
        },

        drop_storage::types::Event::Progress => {}
    }

    Ok(())
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
            crate::Event::FileDownloadCancelled(transfer, file, _) => {
                drop_storage::types::Event::FileCanceled(
                    drop_storage::types::TransferType::Incoming,
                    transfer.id().to_string(),
                    file.to_string(),
                )
            }
            crate::Event::FileUploadCancelled(transfer, file, _) => {
                drop_storage::types::Event::FileCanceled(
                    drop_storage::types::TransferType::Outgoing,
                    transfer.id().to_string(),
                    file.to_string(),
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
                    0,
                )
            }
            crate::Event::FileUploadFailed(transfer, file, error) => {
                drop_storage::types::Event::FileFailed(
                    drop_storage::types::TransferType::Outgoing,
                    transfer.id().to_string(),
                    file.to_string(),
                    error.into(),
                    0,
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

            _ => drop_storage::types::Event::Progress,
        }
    }
}
