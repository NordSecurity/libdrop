use std::{
    collections::{hash_map::Entry, HashMap},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use drop_config::DropConfig;
use drop_storage::{sync, types::OutgoingFileToRetry, Storage};
use slog::{debug, error, info, warn, Logger};
use tokio::sync::{mpsc::UnboundedSender, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    file::FileSubPath,
    service::State,
    tasks::AliveGuard,
    transfer::{IncomingTransfer, OutgoingTransfer},
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
        FileEventTx, FileEventTxFactory, IncomingFileEventTx, OutgoingFileEventTx,
    },
    File, FileId, FileToRecv, FileToSend, Transfer,
};

pub struct CloseResult<T: Transfer> {
    pub xfer: Arc<T>,
    pub events: Vec<Arc<FileEventTx<T>>>,
}

pub struct FileCancelResult {
    pub xfer: Arc<IncomingTransfer>,
    pub events: Arc<IncomingFileEventTx>,
}

pub struct RejectionResult<T: Transfer> {
    pub xfer: Arc<T>,
    pub events: Arc<FileEventTx<T>>,
}

struct TransferSync {
    local: sync::TransferState,
    remote: sync::TransferState,
}

struct IncomingFileSync {
    local: sync::FileState,
    remote: sync::FileState,
    in_flight: Option<PathBuf>,
}

struct OutgoingFileSync {
    local: sync::FileState,
    remote: sync::FileState,
}

pub struct IncomingState {
    pub xfer: Arc<IncomingTransfer>,
    conn: Option<UnboundedSender<ServerReq>>,
    pub dir_mappings: DirMapping,
    xfer_sync: TransferSync,
    file_sync: HashMap<FileId, IncomingFileSync>,
    events: HashMap<FileId, Arc<IncomingFileEventTx>>,
}

pub struct OutgoingState {
    pub xfer: Arc<OutgoingTransfer>,
    conn: Option<UnboundedSender<ClientReq>>,
    xfer_sync: TransferSync,
    file_sync: HashMap<FileId, OutgoingFileSync>,
    events: HashMap<FileId, Arc<OutgoingFileEventTx>>,
}

/// Transfer manager is responsible for keeping track of all ongoing or pending
/// transfers and their status
pub struct TransferManager {
    pub incoming: Mutex<HashMap<Uuid, IncomingState>>,
    pub outgoing: Mutex<HashMap<Uuid, OutgoingState>>,
    storage: Arc<Storage>,
    logger: Logger,
    event_factory: FileEventTxFactory,
}

#[derive(Default)]
pub struct DirMapping {
    mappings: HashMap<PathBuf, String>,
}

impl TransferManager {
    pub fn new(storage: Arc<Storage>, event_factory: FileEventTxFactory, logger: Logger) -> Self {
        Self {
            incoming: Default::default(),
            outgoing: Default::default(),
            storage,
            logger,
            event_factory,
        }
    }

    /// Returns `true` if the transfer is new one
    pub async fn register_incoming(
        &self,
        xfer: Arc<IncomingTransfer>,
        conn: UnboundedSender<ServerReq>,
    ) -> anyhow::Result<bool> {
        let mut lock = self.incoming.lock().await;

        match lock.entry(xfer.id()) {
            Entry::Occupied(mut occ) => {
                let state = occ.get_mut();

                ensure_resume_matches_existing_transfer(&*xfer, &*state.xfer)?;

                info!(
                    self.logger,
                    "Transfer {} resume. Resuming started files",
                    xfer.id()
                );

                match state.xfer_sync.local {
                    sync::TransferState::Canceled => {
                        debug!(self.logger, "Incoming transfer is locally cancelled");
                        let _ = conn.send(ServerReq::Close);
                        drop(conn)
                    }
                    _ => {
                        state.reject_pending(&conn, &self.logger);
                        state.resume_pending(&conn, &self.logger);
                        state.conn = Some(conn);
                    }
                }

                Ok(false)
            }
            Entry::Vacant(vacc) => {
                self.storage.insert_transfer(&xfer.storage_info()).await;
                self.storage
                    .update_transfer_sync_states(
                        xfer.id(),
                        Some(sync::TransferState::Active),
                        Some(sync::TransferState::Active),
                    )
                    .await;

                vacc.insert(IncomingState {
                    conn: Some(conn),
                    dir_mappings: Default::default(),
                    xfer_sync: TransferSync {
                        local: sync::TransferState::Active,
                        remote: sync::TransferState::Active,
                    },
                    file_sync: xfer
                        .files()
                        .keys()
                        .map(|file_id| {
                            (
                                file_id.clone(),
                                IncomingFileSync {
                                    local: sync::FileState::Alive,
                                    remote: sync::FileState::Alive,
                                    in_flight: None,
                                },
                            )
                        })
                        .collect(),
                    xfer,
                    events: HashMap::new(),
                });

                Ok(true)
            }
        }
    }

    pub async fn outgoing_connected(
        &self,
        transfer_id: Uuid,
        conn: UnboundedSender<ClientReq>,
    ) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;
        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        match (state.xfer_sync.local, state.xfer_sync.remote) {
            (sync::TransferState::New, _) => {
                self.storage
                    .update_transfer_sync_states(
                        transfer_id,
                        Some(sync::TransferState::Active),
                        Some(sync::TransferState::Active),
                    )
                    .await;

                state.xfer_sync.local = sync::TransferState::Active;
                state.xfer_sync.remote = sync::TransferState::Active;
            }
            (_, sync::TransferState::New) => {
                self.storage
                    .update_transfer_sync_states(
                        transfer_id,
                        Some(sync::TransferState::Active),
                        None,
                    )
                    .await;
                state.xfer_sync.remote = sync::TransferState::Active;
            }
            _ => (),
        }

        match state.xfer_sync.local {
            sync::TransferState::Canceled => {
                debug!(self.logger, "Outgoing transfer is locally cancelled");
                let _ = conn.send(ClientReq::Close);
                drop(conn);
            }
            _ => {
                state.reject_pending(&conn, &self.logger);
                state.conn = Some(conn);
            }
        }

        Ok(())
    }

    pub async fn insert_outgoing(&self, xfer: Arc<OutgoingTransfer>) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;

        match lock.entry(xfer.id()) {
            Entry::Occupied(_) => {
                warn!(
                    self.logger,
                    "Outgoing transfer UUID colision: {}",
                    xfer.id()
                );

                return Err(crate::Error::BadTransferState(
                    "Transfer already exists".into(),
                ));
            }
            Entry::Vacant(entry) => {
                self.storage.insert_transfer(&xfer.storage_info()).await;

                entry.insert(OutgoingState {
                    conn: None,
                    xfer_sync: TransferSync {
                        local: sync::TransferState::New,
                        remote: sync::TransferState::New,
                    },
                    file_sync: xfer
                        .files()
                        .keys()
                        .map(|file_id| {
                            (
                                file_id.clone(),
                                OutgoingFileSync {
                                    local: sync::FileState::Alive,
                                    remote: sync::FileState::Alive,
                                },
                            )
                        })
                        .collect(),
                    xfer,
                    events: HashMap::new(),
                });
            }
        };

        Ok(())
    }

    pub async fn incoming_file_events(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<Arc<IncomingFileEventTx>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        Ok(state.file_events(&self.event_factory, file_id))
    }

    pub async fn outgoing_file_events(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<Arc<OutgoingFileEventTx>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        Ok(state.file_events(&self.event_factory, file_id))
    }

    pub async fn outgoing_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<RejectionResult<OutgoingTransfer>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if matches!(
            state.xfer_sync.local,
            drop_storage::sync::TransferState::Canceled
        ) {
            return Err(crate::Error::BadTransfer);
        }

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        match sync.local {
            sync::FileState::Rejected => return Err(crate::Error::Rejected),
            sync::FileState::Alive => {
                self.storage
                    .update_outgoing_file_sync_states(
                        state.xfer.id(),
                        file_id.as_ref(),
                        None,
                        Some(sync::FileState::Rejected),
                    )
                    .await;
                sync.local = sync::FileState::Rejected;

                if let Some(conn) = &state.conn {
                    let _ = conn.send(ClientReq::Reject {
                        file: file_id.clone(),
                    });
                }
            }
        }

        Ok(RejectionResult {
            xfer: state.xfer.clone(),
            events: state.file_events(&self.event_factory, file_id),
        })
    }

    pub async fn outgoing_rejection_ack(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        self.storage
            .update_outgoing_file_sync_states(
                transfer_id,
                file_id.as_ref(),
                Some(sync::FileState::Rejected),
                None,
            )
            .await;
        sync.remote = sync::FileState::Rejected;

        Ok(())
    }

    pub async fn outgoing_rejection_recv(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        self.storage
            .update_outgoing_file_sync_states(
                transfer_id,
                file_id.as_ref(),
                Some(sync::FileState::Rejected),
                Some(sync::FileState::Rejected),
            )
            .await;
        sync.remote = sync::FileState::Rejected;
        sync.local = sync::FileState::Rejected;

        Ok(())
    }

    pub async fn incoming_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<RejectionResult<IncomingTransfer>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if matches!(
            state.xfer_sync.local,
            drop_storage::sync::TransferState::Canceled
        ) {
            return Err(crate::Error::BadTransfer);
        }

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        match sync.local {
            sync::FileState::Rejected => return Err(crate::Error::Rejected),
            sync::FileState::Alive => {
                self.storage
                    .update_incoming_file_sync_states(
                        state.xfer.id(),
                        file_id.as_ref(),
                        None,
                        Some(sync::FileState::Rejected),
                    )
                    .await;
                self.storage
                    .stop_incoming_file(state.xfer.id(), file_id.as_ref())
                    .await;

                sync.local = sync::FileState::Rejected;
                let _ = sync.in_flight.take();

                if let Some(conn) = &state.conn {
                    let _ = conn.send(ServerReq::Reject {
                        file: file_id.clone(),
                    });
                }
            }
        }

        Ok(RejectionResult {
            xfer: state.xfer.clone(),
            events: state.file_events(&self.event_factory, file_id),
        })
    }

    pub async fn incoming_rejection_ack(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        self.storage
            .update_incoming_file_sync_states(
                transfer_id,
                file_id.as_ref(),
                Some(sync::FileState::Rejected),
                None,
            )
            .await;
        sync.remote = sync::FileState::Rejected;

        Ok(())
    }

    pub async fn incoming_rejection_recv(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        self.storage
            .update_incoming_file_sync_states(
                transfer_id,
                file_id.as_ref(),
                Some(sync::FileState::Rejected),
                Some(sync::FileState::Rejected),
            )
            .await;
        sync.remote = sync::FileState::Rejected;
        sync.local = sync::FileState::Rejected;
        sync.in_flight.take();

        Ok(())
    }

    pub async fn incoming_remove(&self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.incoming.lock().await;
        if !lock.contains_key(&transfer_id) {
            return Err(crate::Error::BadTransfer);
        }
        self.storage.transfer_sync_clear(transfer_id).await;
        lock.remove(&transfer_id);

        Ok(())
    }

    pub async fn incoming_finish_download(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if matches!(state.xfer_sync.local, sync::TransferState::Canceled) {
            return Err(crate::Error::BadTransfer);
        }

        let state = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        if matches!(state.local, sync::FileState::Rejected) {
            return Err(crate::Error::Rejected);
        }

        self.storage
            .stop_incoming_file(transfer_id, file_id.as_ref())
            .await;
        let _ = state.in_flight.take();

        Ok(())
    }

    pub async fn incoming_issue_close(
        &self,
        transfer_id: Uuid,
    ) -> crate::Result<CloseResult<IncomingTransfer>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        self.storage
            .update_transfer_sync_states(
                transfer_id,
                None,
                Some(drop_storage::sync::TransferState::Canceled),
            )
            .await;
        state.xfer_sync.local = sync::TransferState::Canceled;

        if let Some(conn) = state.conn.take() {
            let _ = conn.send(ServerReq::Close);
        }

        for val in state.file_sync.values_mut() {
            val.in_flight.take();
        }

        let res = CloseResult {
            xfer: state.xfer.clone(),
            events: state.events.values().cloned().collect(),
        };

        Ok(res)
    }

    pub async fn outgoing_issue_close(
        &self,
        transfer_id: Uuid,
    ) -> crate::Result<CloseResult<OutgoingTransfer>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        self.storage
            .update_transfer_sync_states(
                transfer_id,
                None,
                Some(drop_storage::sync::TransferState::Canceled),
            )
            .await;
        state.xfer_sync.local = sync::TransferState::Canceled;

        if let Some(conn) = state.conn.take() {
            let _ = conn.send(ClientReq::Close);
        }

        let res = CloseResult {
            xfer: state.xfer.clone(),
            events: state.events.values().cloned().collect(),
        };

        Ok(res)
    }

    pub async fn outgoing_ensure_file_not_rejected(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
        let lock = self.outgoing.lock().await;
        let state = lock.get(&transfer_id).ok_or(crate::Error::BadTransfer)?;
        let state = state
            .file_sync
            .get(file_id)
            .ok_or(crate::Error::BadFileId)?;

        if matches!(state.local, drop_storage::sync::FileState::Rejected) {
            return Err(crate::Error::Rejected);
        }
        Ok(())
    }

    pub async fn outgoing_remove(&self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;
        if !lock.contains_key(&transfer_id) {
            return Err(crate::Error::BadTransfer);
        }
        self.storage.transfer_sync_clear(transfer_id).await;
        lock.remove(&transfer_id);

        Ok(())
    }

    pub async fn incoming_disconnect(&self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.incoming.lock().await;
        let _ = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?
            .conn
            .take();
        Ok(())
    }

    pub async fn incoming_cancel_file(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<Option<FileCancelResult>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;
        state.ensure_not_cancelled()?;

        let sync = state
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        if matches!(sync.local, sync::FileState::Rejected) {
            return Err(crate::Error::Rejected);
        }

        let cancelled = if sync.in_flight.is_some() {
            self.storage
                .stop_incoming_file(state.xfer.id(), file_id.as_ref())
                .await;
            let _ = sync.in_flight.take();

            if let Some(conn) = &state.conn {
                let _ = conn.send(ServerReq::Cancel {
                    file: file_id.clone(),
                });
            }

            true
        } else {
            false
        };

        Ok(cancelled.then(|| FileCancelResult {
            xfer: state.xfer.clone(),
            events: state.file_events(&self.event_factory, file_id),
        }))
    }

    pub async fn outgoing_disconnect(&self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.outgoing.lock().await;
        let _ = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?
            .conn
            .take();
        Ok(())
    }
}

impl OutgoingState {
    fn reject_pending(&self, conn: &UnboundedSender<ClientReq>, logger: &Logger) {
        for file_id in self
            .file_sync
            .iter()
            .filter_map(|(k, v)| match (v.local, v.remote) {
                (sync::FileState::Rejected, sync::FileState::Alive) => Some(k),
                _ => None,
            })
        {
            info!(logger, "Rejecting file: {file_id}");
            let _ = conn.send(ClientReq::Reject {
                file: file_id.clone(),
            });
        }
    }

    fn ensure_not_cancelled(&self) -> crate::Result<()> {
        if matches!(
            self.xfer_sync.local,
            drop_storage::sync::TransferState::Canceled
        ) {
            return Err(crate::Error::BadTransfer);
        }
        Ok(())
    }

    fn file_events(
        &mut self,
        factory: &FileEventTxFactory,
        file_id: &FileId,
    ) -> Arc<OutgoingFileEventTx> {
        self.events
            .entry(file_id.clone())
            .or_insert_with_key(|file_id| {
                Arc::new(factory.create(self.xfer.clone(), file_id.clone()))
            })
            .clone()
    }
}

impl IncomingState {
    /// Returs `true` when the new download can be started and `false` in case
    /// the downaload is already happening
    pub fn validate_for_download(&self, file_id: &FileId) -> crate::Result<bool> {
        if matches!(self.xfer_sync.local, sync::TransferState::Canceled) {
            return Err(crate::Error::BadTransfer);
        }

        let state = self.file_sync.get(file_id).ok_or(crate::Error::BadFileId)?;

        if matches!(state.local, sync::FileState::Rejected) {
            return Err(crate::Error::Rejected);
        }

        Ok(state.in_flight.is_none())
    }

    pub async fn start_download(
        &mut self,
        storage: &Storage,
        file_id: &FileId,
        parent_dir: &Path,
        logger: &Logger,
    ) -> crate::Result<()> {
        if storage
            .start_incoming_file(
                self.xfer.id(),
                file_id.as_ref(),
                &parent_dir.to_string_lossy(),
            )
            .await
            .is_none()
        {
            warn!(logger, "Failed to store started file state into the DB");
        }

        self.file_sync
            .get_mut(file_id)
            .expect("Missing file sync state")
            .in_flight = Some(parent_dir.to_path_buf());

        let file = &self.xfer.files()[file_id];

        if let Some(conn) = &self.conn {
            let task = FileXferTask::new(file.clone(), self.xfer.clone(), parent_dir.into());

            let _ = conn.send(ServerReq::Download {
                task: Box::new(task),
            });
        }

        Ok(())
    }

    fn file_events(
        &mut self,
        factory: &FileEventTxFactory,
        file_id: &FileId,
    ) -> Arc<IncomingFileEventTx> {
        self.events
            .entry(file_id.clone())
            .or_insert_with_key(|file_id| {
                Arc::new(factory.create(self.xfer.clone(), file_id.clone()))
            })
            .clone()
    }

    fn ensure_not_cancelled(&self) -> crate::Result<()> {
        if matches!(
            self.xfer_sync.local,
            drop_storage::sync::TransferState::Canceled
        ) {
            return Err(crate::Error::BadTransfer);
        }
        Ok(())
    }

    fn reject_pending(&self, conn: &UnboundedSender<ServerReq>, logger: &Logger) {
        for file_id in self
            .file_sync
            .iter()
            .filter_map(|(k, v)| match (v.local, v.remote) {
                (sync::FileState::Rejected, sync::FileState::Alive) => Some(k),
                _ => None,
            })
        {
            info!(logger, "Rejecting file: {file_id}");
            let _ = conn.send(ServerReq::Reject {
                file: file_id.clone(),
            });
        }
    }

    fn resume_pending(&self, conn: &UnboundedSender<ServerReq>, logger: &Logger) {
        let iter = self.file_sync.iter().filter_map(|(k, v)| match v.local {
            sync::FileState::Rejected => None,
            _ => v.in_flight.as_deref().map(|p| (k, p)),
        });

        for (file_id, base_dir) in iter {
            info!(logger, "Resuming file: {file_id}",);

            let xfile = &self.xfer.files()[file_id];
            let task = FileXferTask::new(xfile.clone(), self.xfer.clone(), base_dir.into());
            let _ = conn.send(ServerReq::Download {
                task: Box::new(task),
            });
        }
    }
}

impl DirMapping {
    /// This function composes the final path for the file.
    /// For ordinary files (subpath contains only one element) it just joins
    /// `dest_dir` with `file_subpath`. For directories (subpath of the form
    /// `dir1/dir2/.../filename`) it does a couple of things:
    ///
    /// * it checks if `dest_dir/dir1` already exists and if we created it
    /// * if it doesn't then it creates the directory
    /// * if it exists and is not created by us it keeps appending (1), (2), ...
    ///   suffix and repeats the prevoius step
    /// * finally appends the rest of subpath components into the final path
    ///  `dest_dir/<mapped dir1>/dir2/../filename`
    ///
    /// The results are cached in RAM to speed this up
    pub fn compose_final_path(
        &mut self,
        dest_dir: &Path,
        file_subpath: &FileSubPath,
    ) -> crate::Result<PathBuf> {
        let mut iter = file_subpath.iter().map(crate::utils::normalize_filename);

        let probe = iter.next().ok_or_else(|| {
            crate::Error::BadPath("Path should contain at least one component".into())
        })?;
        let next = iter.next();

        let mapped = match next {
            Some(next) => {
                // Check if dir exists and is known to us
                let name = match self.mappings.entry(dest_dir.join(probe)) {
                    // Dir is known, reuse
                    Entry::Occupied(occ) => occ.get().clone(),
                    // Dir in new, check if there is name conflict and add to known
                    Entry::Vacant(vacc) => {
                        let mapped = crate::utils::filepath_variants(vacc.key())?.find(|dst_location| {
                                // Skip if there is already a file with the same name.
                                // Additionaly there could be a dangling symlink with the same name,
                                // the `symlink_metadata()` ensures we can catch that.
                                matches!(dst_location.symlink_metadata() , Err(err) if err.kind() == io::ErrorKind::NotFound)
                            })
                            .expect("The filepath variants iterator should never end");

                        let value = vacc.insert(
                            mapped
                                .file_name()
                                .ok_or_else(|| crate::Error::BadPath("Missing file name".into()))?
                                .to_str()
                                .ok_or_else(|| crate::Error::BadPath("Invalid UTF8 path".into()))?
                                .to_string(),
                        );

                        value.clone()
                    }
                };

                [name, next].into_iter().chain(iter).collect()
            }
            None => {
                // Ordinary file
                probe.into()
            }
        };

        Ok(mapped)
    }

    pub fn register_preexisting_final_path(
        &mut self,
        file_subpath: &FileSubPath,
        full_path: impl AsRef<Path>,
    ) {
        self.mappings
            .extend(extract_directory_mapping(file_subpath, full_path.as_ref()));
    }
}

pub(crate) async fn resume(
    state: &Arc<State>,
    logger: &Logger,
    guard: &AliveGuard,
    stop: &CancellationToken,
) {
    *state.transfer_manager.incoming.lock().await =
        restore_incoming(&state.storage, &state.config, logger).await;
    *state.transfer_manager.outgoing.lock().await =
        restore_outgoing(state, logger, guard, stop).await;
}

async fn restore_incoming(
    storage: &Storage,
    config: &DropConfig,
    logger: &Logger,
) -> HashMap<Uuid, IncomingState> {
    let transfers = storage.incoming_transfers_to_resume().await;

    let mut xfers = HashMap::new();
    for transfer in transfers {
        let restore_transfer = async {
            let files = transfer
                .files
                .into_iter()
                .map(|dbfile| {
                    FileToRecv::new(dbfile.file_id.into(), dbfile.subpath.into(), dbfile.size)
                })
                .collect();

            let xfer = IncomingTransfer::new_with_uuid(
                transfer.peer.parse().context("Failed to parse peer IP")?,
                files,
                transfer.uuid,
                config,
            )
            .context("Failed to create transfer")?;

            let sync = storage
                .transfer_sync_state(xfer.id())
                .await
                .context("Missing sync state for transfer")?;

            let mut file_sync = HashMap::new();

            for file_id in xfer.files().keys() {
                let sync = storage
                    .incoming_file_sync_state(xfer.id(), file_id.as_ref())
                    .await
                    .context("Missing sync state for file")?;

                file_sync.insert(
                    file_id.clone(),
                    IncomingFileSync {
                        local: sync.local_state,
                        remote: sync.remote_state,
                        in_flight: None,
                    },
                );
            }

            let in_flights = storage.incoming_files_to_resume(xfer.id()).await;

            for file in in_flights {
                if let Some(state) = file_sync.get_mut(&file.file_id) {
                    state.in_flight = Some(file.base_dir.into());
                }
            }

            let mut xstate = IncomingState {
                xfer: Arc::new(xfer),
                conn: None,
                dir_mappings: Default::default(),
                xfer_sync: TransferSync {
                    local: sync.local_state,
                    remote: sync.remote_state,
                },
                file_sync,
                events: HashMap::new(),
            };

            debug!(
                logger,
                "Restoring transfer: {}, states: local {:?}, remote {:?}",
                xstate.xfer.id(),
                xstate.xfer_sync.local,
                xstate.xfer_sync.remote
            );

            let paths = storage.finished_incoming_files(xstate.xfer.id()).await;
            for path in paths {
                let subpath = FileSubPath::from(path.subpath);
                xstate
                    .dir_mappings
                    .register_preexisting_final_path(&subpath, &path.final_path);
            }

            anyhow::Ok(xstate)
        };

        match restore_transfer.await {
            Ok(xstate) => {
                xfers.insert(xstate.xfer.id(), xstate);
            }
            Err(err) => {
                error!(
                    logger,
                    "Failed to restore transfer {}: {err:?}", transfer.uuid
                );
            }
        }
    }

    xfers
}

async fn restore_outgoing(
    state: &Arc<State>,
    logger: &Logger,
    guard: &AliveGuard,
    stop: &CancellationToken,
) -> HashMap<Uuid, OutgoingState> {
    let transfers = state.storage.outgoing_transfers_to_resume().await;

    let mut xfers = HashMap::new();
    for transfer in transfers {
        let restore_transfer = || async move {
            let files = transfer
                .files
                .into_iter()
                .map(|dbfile| restore_outgoing_file(state, dbfile))
                .collect::<Result<_, _>>()?;

            let xfer = OutgoingTransfer::new_with_uuid(
                transfer.peer.parse().context("Failed to parse peer IP")?,
                files,
                transfer.uuid,
                &state.config,
            )
            .context("Failed to create transfer")?;

            let sync = state
                .storage
                .transfer_sync_state(xfer.id())
                .await
                .context("Missing sync state for transfer")?;

            let mut file_sync = HashMap::new();
            for file_id in xfer.files().keys() {
                let sync = state
                    .storage
                    .outgoing_file_sync_state(xfer.id(), file_id.as_ref())
                    .await
                    .context("Missing sync state for file")?;

                file_sync.insert(
                    file_id.clone(),
                    OutgoingFileSync {
                        local: sync.local_state,
                        remote: sync.remote_state,
                    },
                );
            }

            let xstate = OutgoingState {
                xfer: Arc::new(xfer),
                conn: None,
                xfer_sync: TransferSync {
                    local: sync.local_state,
                    remote: sync.remote_state,
                },
                file_sync,
                events: HashMap::new(),
            };
            anyhow::Ok(xstate)
        };

        match restore_transfer().await {
            Ok(xstate) => {
                xfers.insert(xstate.xfer.id(), xstate);
            }
            Err(err) => {
                error!(
                    logger,
                    "Failed to restore transfer {}: {err}", transfer.uuid
                );
            }
        }
    }

    for xstate in xfers.values() {
        ws::client::spawn(
            state.clone(),
            xstate.xfer.clone(),
            logger.clone(),
            guard.clone(),
            stop.clone(),
        );
    }

    xfers
}

#[allow(unused_variables)]
fn restore_outgoing_file(state: &State, dbfile: OutgoingFileToRetry) -> anyhow::Result<FileToSend> {
    let file_id: FileId = dbfile.file_id.into();
    let subpath: FileSubPath = dbfile.subpath.into();
    let uri = dbfile.uri;
    let size = dbfile.size as u64;

    let file = match uri.scheme() {
        "file" => {
            let fullpath = uri
                .to_file_path()
                .ok()
                .context("Failed to extract file path")?;

            FileToSend::new(subpath, fullpath, size, file_id)
        }
        #[cfg(unix)]
        "content" => {
            let callback = state
                .fdresolv
                .clone()
                .context("Encountered content uri but FD resovler callback is missing")?;

            FileToSend::new_from_content_uri(callback, subpath, uri, size, file_id)
        }
        unknown => anyhow::bail!("Unknon URI schema: {unknown}"),
    };

    anyhow::Ok(file)
}

fn extract_directory_mapping(
    file_subpath: &FileSubPath,
    full_path: &Path,
) -> Option<(PathBuf, String)> {
    let mut iter = file_subpath.iter();
    let first = iter.next()?;

    let count = iter.count();

    // Insert only directories
    if count > 0 {
        let ancestor = full_path.ancestors().nth(count)?;

        let filename = ancestor.file_name()?.to_str()?.to_string();

        let path = ancestor.with_file_name(first);
        Some((path, filename))
    } else {
        None
    }
}

fn ensure_resume_matches_existing_transfer<T: Transfer>(
    current: &T,
    existing: &T,
) -> anyhow::Result<()> {
    // Check if the transfer matches
    anyhow::ensure!(current.peer() == existing.peer(), "Peers do not match",);
    anyhow::ensure!(
        current.files().len() == existing.files().len(),
        "File count does not match"
    );

    anyhow::ensure!(
        current
            .files()
            .iter()
            .all(|(key, val)| existing.files().get(key).map_or(false, |v| {
                val.id() == v.id()
                    && val.subpath() == v.subpath()
                    && val.size() == v.size()
                    && val.mime_type() == v.mime_type()
            })),
        "Files do not match"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracting_dir_mapping() {
        let (path, name) = extract_directory_mapping(
            &FileSubPath::from_path("a/b/c.txt").unwrap(),
            "/home/xyz/foo/bar/a(2)/b/c.txt".as_ref(),
        )
        .expect("Failed to read mapping");

        assert_eq!(path, Path::new("/home/xyz/foo/bar/a"));
        assert_eq!(name, "a(2)");
    }
}
