use std::collections::HashMap;

use drop_storage::Storage;
use uuid::Uuid;

use crate::{transfer::Transfer, FileId};

pub struct StorageDispatch<'a> {
    storage: &'a drop_storage::Storage,
    file_progress: HashMap<Uuid, HashMap<FileId, i64>>,
}

impl<'a> StorageDispatch<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self {
            storage,
            file_progress: HashMap::new(),
        }
    }

    pub async fn handle_event(&mut self, event: &crate::Event) {
        match event {
            crate::Event::FileUploadStarted(transfer, file_id, bytes) => {
                self.store_progres(transfer.id(), file_id, *bytes as _);
                self.storage
                    .insert_outgoing_path_started_state(
                        transfer.id(),
                        file_id.as_ref(),
                        *bytes as _,
                    )
                    .await
            }
            crate::Event::FileDownloadStarted(transfer, file_id, _, bytes) => {
                self.store_progres(transfer.id(), file_id, *bytes as _);
                self.storage
                    .insert_incoming_path_started_state(
                        transfer.id(),
                        file_id.as_ref(),
                        *bytes as _,
                    )
                    .await
            }
            crate::Event::FileDownloadSuccess(transfer, download) => {
                self.storage
                    .insert_incoming_path_completed_state(
                        transfer.id(),
                        download.id.as_ref(),
                        &download.final_path.to_string_lossy(),
                    )
                    .await
            }
            crate::Event::FileUploadSuccess(transfer, file_id) => {
                self.storage
                    .insert_outgoing_path_completed_state(transfer.id(), file_id.as_ref())
                    .await
            }
            crate::Event::IncomingTransferCanceled(transfer, by_peer) => {
                self.storage
                    .insert_transfer_cancel_state(transfer.id(), *by_peer)
                    .await;
                self.clear_transfer(transfer.id());
            }
            crate::Event::OutgoingTransferCanceled(transfer, by_peer) => {
                self.storage
                    .insert_transfer_cancel_state(transfer.id(), *by_peer)
                    .await;
                self.clear_transfer(transfer.id());
            }
            crate::Event::OutgoingTransferFailed(transfer, err, _) => {
                self.storage
                    .insert_transfer_failed_state(transfer.id(), err.into())
                    .await;
                self.clear_transfer(transfer.id());
            }
            crate::Event::FileUploadFailed(transfer, file_id, err) => {
                self.storage
                    .insert_outgoing_path_failed_state(
                        transfer.id(),
                        file_id.as_ref(),
                        err.into(),
                        self.get_file_progress(transfer.id(), file_id),
                    )
                    .await
            }
            crate::Event::FileDownloadFailed(transfer, file_id, err) => {
                self.storage
                    .insert_incoming_path_failed_state(
                        transfer.id(),
                        file_id.as_ref(),
                        err.into(),
                        self.get_file_progress(transfer.id(), file_id),
                    )
                    .await
            }
            crate::Event::FileUploadProgress(transfer, file_id, progress) => {
                self.store_progres(transfer.id(), file_id, *progress as _)
            }
            crate::Event::FileDownloadProgress(transfer, file_id, progress) => {
                self.store_progres(transfer.id(), file_id, *progress as _)
            }
            crate::Event::FileUploadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => {
                self.storage
                    .insert_outgoing_path_reject_state(
                        *transfer_id,
                        file_id.as_ref(),
                        *by_peer,
                        self.get_file_progress(*transfer_id, file_id),
                    )
                    .await
            }
            crate::Event::FileDownloadRejected {
                transfer_id,
                file_id,
                by_peer,
            } => {
                self.storage
                    .insert_incoming_path_reject_state(
                        *transfer_id,
                        file_id.as_ref(),
                        *by_peer,
                        self.get_file_progress(*transfer_id, file_id),
                    )
                    .await
            }
            crate::Event::FileUploadPaused {
                transfer_id,
                file_id,
            } => {
                self.storage
                    .insert_outgoing_path_paused_state(
                        *transfer_id,
                        file_id.as_ref(),
                        self.get_file_progress(*transfer_id, file_id),
                    )
                    .await
            }
            crate::Event::FileDownloadPaused {
                transfer_id,
                file_id,
            } => {
                self.storage
                    .insert_incoming_path_paused_state(
                        *transfer_id,
                        file_id.as_ref(),
                        self.get_file_progress(*transfer_id, file_id),
                    )
                    .await
            }

            // not stored in the database
            crate::Event::RequestReceived(_) => (),
            crate::Event::RequestQueued(_) => (),
            crate::Event::FileUploadThrottled { .. } => (),

            crate::Event::OutgoingTransferDeferred { .. } => (),

            crate::Event::FinalizeChecksumStarted { .. } => (),
            crate::Event::FinalizeChecksumFinished { .. } => (),
            crate::Event::FinalizeChecksumProgress { .. } => (),

            crate::Event::VerifyChecksumStarted { .. } => (),
            crate::Event::VerifyChecksumFinished { .. } => (),
            crate::Event::VerifyChecksumProgress { .. } => (),

            crate::Event::FileDownloadPending { .. } => (),
        }
    }

    fn get_file_progress(&mut self, transfer_id: Uuid, file_id: &FileId) -> i64 {
        self.file_progress
            .entry(transfer_id)
            .or_default()
            .remove(file_id)
            .unwrap_or(0)
    }

    fn store_progres(&mut self, transfer_id: Uuid, file_id: &FileId, progress: i64) {
        *self
            .file_progress
            .entry(transfer_id)
            .or_default()
            .entry(file_id.clone())
            .or_default() = progress;
    }

    fn clear_transfer(&mut self, transfer_id: Uuid) {
        self.file_progress.remove(&transfer_id);
    }
}
