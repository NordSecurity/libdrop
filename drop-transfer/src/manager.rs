use std::{
    collections::{hash_map::Entry, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use drop_config::DropConfig;
use drop_storage::{sync, Storage};
use slog::{error, info, warn, Logger};
use tokio::sync::{mpsc::UnboundedSender, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    file::FileSubPath,
    service::State,
    transfer::{IncomingTransfer, OutgoingTransfer},
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
    },
    File, FileId, FileToRecv, FileToSend, Transfer,
};

pub struct TransferSync {
    local: sync::TransferState,
    remote: sync::TransferState,
}

pub struct IncomingFileSync {
    local: sync::FileState,
    remote: sync::FileState,
    pub in_flight: Option<PathBuf>,
}

pub struct OutgoingFileSync {
    local: sync::FileState,
    remote: sync::FileState,
}

pub struct IncomingState {
    pub xfer: Arc<IncomingTransfer>,
    pub conn: Option<UnboundedSender<ServerReq>>,
    pub dir_mappings: DirMapping,
    xfer_sync: TransferSync,
    file_sync: HashMap<FileId, IncomingFileSync>,
}

pub struct OutgoingState {
    pub xfer: Arc<OutgoingTransfer>,
    pub conn: Option<UnboundedSender<ClientReq>>,
    xfer_sync: TransferSync,
    file_sync: HashMap<FileId, OutgoingFileSync>,
}

/// Transfer manager is responsible for keeping track of all ongoing or pending
/// transfers and their status
pub struct TransferManager {
    pub incoming: Mutex<HashMap<Uuid, IncomingState>>,
    pub outgoing: Mutex<HashMap<Uuid, OutgoingState>>,
    storage: Arc<Storage>,
    logger: Logger,
}

#[derive(Default)]
pub struct DirMapping {
    mappings: HashMap<PathBuf, String>,
}

impl TransferManager {
    pub fn new(storage: Arc<Storage>, logger: Logger) -> Self {
        Self {
            incoming: Default::default(),
            outgoing: Default::default(),
            storage,
            logger,
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
                    sync::TransferState::Canceled => drop(conn),
                    _ => {
                        state.reject_pending(&conn, &self.logger);
                        state.resume_pending(&conn, &self.logger);
                        state.conn = Some(conn);
                    }
                }

                Ok(false)
            }
            Entry::Vacant(vacc) => {
                self.storage.insert_transfer(&xfer.storage_info())?;
                self.storage.update_transfer_sync_states(
                    xfer.id(),
                    Some(sync::TransferState::Active),
                    Some(sync::TransferState::Active),
                )?;

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
                self.storage.update_transfer_sync_states(
                    transfer_id,
                    Some(sync::TransferState::Active),
                    Some(sync::TransferState::Active),
                )?;

                state.xfer_sync.local = sync::TransferState::Active;
                state.xfer_sync.remote = sync::TransferState::Active;
            }
            (_, sync::TransferState::New) => {
                self.storage.update_transfer_sync_states(
                    transfer_id,
                    Some(sync::TransferState::Active),
                    None,
                )?;
                state.xfer_sync.remote = sync::TransferState::Active;
            }
            _ => (),
        }

        match state.xfer_sync.local {
            sync::TransferState::Canceled => {
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
                self.storage.insert_transfer(&xfer.storage_info())?;

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
                });
            }
        };

        Ok(())
    }

    pub async fn outgoing_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
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
                self.storage.update_outgoing_file_sync_states(
                    state.xfer.id(),
                    file_id.as_ref(),
                    None,
                    Some(sync::FileState::Rejected),
                )?;
                sync.local = sync::FileState::Rejected;

                if let Some(conn) = &state.conn {
                    let _ = conn.send(ClientReq::Reject {
                        file: file_id.clone(),
                    });
                }
            }
        }

        Ok(())
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

        self.storage.update_outgoing_file_sync_states(
            transfer_id,
            file_id.as_ref(),
            Some(sync::FileState::Rejected),
            None,
        )?;
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

        self.storage.update_outgoing_file_sync_states(
            transfer_id,
            file_id.as_ref(),
            Some(sync::FileState::Rejected),
            Some(sync::FileState::Rejected),
        )?;
        sync.remote = sync::FileState::Rejected;
        sync.local = sync::FileState::Rejected;

        Ok(())
    }

    pub async fn incoming_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<()> {
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
                self.storage.update_incoming_file_sync_states(
                    state.xfer.id(),
                    file_id.as_ref(),
                    None,
                    Some(sync::FileState::Rejected),
                )?;
                self.storage
                    .stop_incoming_file(state.xfer.id(), file_id.as_ref())?;

                sync.local = sync::FileState::Rejected;
                let _ = sync.in_flight.take();

                if let Some(conn) = &state.conn {
                    let _ = conn.send(ServerReq::Reject {
                        file: file_id.clone(),
                    });
                }
            }
        }

        Ok(())
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

        self.storage.update_incoming_file_sync_states(
            transfer_id,
            file_id.as_ref(),
            Some(sync::FileState::Rejected),
            None,
        )?;
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

        self.storage.update_incoming_file_sync_states(
            transfer_id,
            file_id.as_ref(),
            Some(sync::FileState::Rejected),
            Some(sync::FileState::Rejected),
        )?;
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
        self.storage.trasnfer_sync_clear(transfer_id)?;
        lock.remove(&transfer_id);

        Ok(())
    }

    pub async fn incoming_issue_close(
        &self,
        transfer_id: Uuid,
    ) -> crate::Result<Arc<IncomingTransfer>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if matches!(state.xfer_sync.local, sync::TransferState::Canceled) {
            return Err(crate::Error::BadTransfer);
        }

        self.storage.update_transfer_sync_states(
            transfer_id,
            None,
            Some(drop_storage::sync::TransferState::Canceled),
        )?;
        state.xfer_sync.local = sync::TransferState::Canceled;
        let _ = state.conn.take();
        for val in state.file_sync.values_mut() {
            val.in_flight.take();
        }

        Ok(state.xfer.clone())
    }

    pub async fn outgoing_issue_close(
        &self,
        transfer_id: Uuid,
    ) -> crate::Result<Arc<OutgoingTransfer>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if matches!(state.xfer_sync.local, sync::TransferState::Canceled) {
            return Err(crate::Error::BadTransfer);
        }

        self.storage.update_transfer_sync_states(
            transfer_id,
            None,
            Some(drop_storage::sync::TransferState::Canceled),
        )?;
        state.xfer_sync.local = sync::TransferState::Canceled;
        let _ = state.conn.take();

        Ok(state.xfer.clone())
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
        self.storage.trasnfer_sync_clear(transfer_id)?;
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

    pub fn start_download(
        &mut self,
        storage: &Storage,
        file_id: &FileId,
        parent_dir: &Path,
    ) -> crate::Result<()> {
        storage.start_incoming_file(
            self.xfer.id(),
            file_id.as_ref(),
            &parent_dir.to_string_lossy(),
        )?;

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

    /// Returs `true` when cancel was issued and `false` in case its already
    /// canceled
    pub fn cancel_file(&mut self, storage: &Storage, file_id: &FileId) -> crate::Result<bool> {
        if matches!(self.xfer_sync.local, sync::TransferState::Canceled) {
            return Err(crate::Error::BadTransfer);
        }

        let state = self
            .file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)?;

        if matches!(state.local, sync::FileState::Rejected) {
            return Err(crate::Error::Rejected);
        }

        let canceled = if state.in_flight.is_some() {
            storage.stop_incoming_file(self.xfer.id(), file_id.as_ref())?;
            let _ = state.in_flight.take();

            if let Some(conn) = &self.conn {
                let _ = conn.send(ServerReq::Cancel {
                    file: file_id.clone(),
                });
            }

            true
        } else {
            false
        };

        Ok(canceled)
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

pub(crate) async fn resume(state: &Arc<State>, stop: CancellationToken, logger: &Logger) {
    let incoming = {
        let state = state.clone();
        let logger = logger.clone();
        tokio::task::spawn_blocking(move || {
            restore_incoming(&state.storage, &state.config, &logger)
        })
    };

    let outgoing = {
        let state = state.clone();
        let logger = logger.clone();
        tokio::task::spawn_blocking(move || restore_outgoing(&state, &stop, &logger))
    };

    match incoming.await {
        Ok(incoming) => *state.transfer_manager.incoming.lock().await = incoming,
        Err(err) => error!(logger, "Failed to join incoming resume task: {err}"),
    }

    match outgoing.await {
        Ok(outgoing) => *state.transfer_manager.outgoing.lock().await = outgoing,
        Err(err) => error!(logger, "Failed to join outgoing resume task: {err}"),
    }
}

fn restore_incoming(
    storage: &Storage,
    config: &DropConfig,
    logger: &Logger,
) -> HashMap<Uuid, IncomingState> {
    let transfers = match storage.incoming_transfers_to_resume() {
        Ok(transfers) => transfers,
        Err(err) => {
            error!(
                logger,
                "Failed to restore incoming pedning transfers form DB: {err}"
            );
            return HashMap::new();
        }
    };

    transfers
        .into_iter()
        .filter_map(|transfer| {
            let restore_transfer = || {
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
                    .context("Failed to fetch transfer sync state")?
                    .context("Missing sync state for transfer")?;

                let mut file_sync = HashMap::new();

                for file_id in xfer.files().keys() {
                    let sync = storage
                        .incoming_file_sync_state(xfer.id(), file_id.as_ref())
                        .context("Failed to fetch file sync state")?
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

                let in_flights = storage
                    .incoming_files_to_resume(xfer.id())
                    .context("Failed to fetch file to resume")?;

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
                };

                let paths = storage
                    .finished_incoming_files(xstate.xfer.id())
                    .context("Failed to fetch finished paths")?;
                for path in paths {
                    let subpath = FileSubPath::from(path.subpath);
                    xstate
                        .dir_mappings
                        .register_preexisting_final_path(&subpath, &path.final_path);
                }

                anyhow::Ok(xstate)
            };

            match restore_transfer() {
                Ok(xstate) => Some((xstate.xfer.id(), xstate)),
                Err(err) => {
                    error!(
                        logger,
                        "Failed to restore transfer {}: {err:?}", transfer.uuid
                    );

                    None
                }
            }
        })
        .collect()
}

fn restore_outgoing(
    state: &Arc<State>,
    stop: &CancellationToken,
    logger: &Logger,
) -> HashMap<Uuid, OutgoingState> {
    let transfers = match state.storage.outgoing_transfers_to_resume() {
        Ok(transfers) => transfers,
        Err(err) => {
            error!(
                logger,
                "Failed to restore pedning outgoing transfers form DB: {err}"
            );

            return HashMap::new();
        }
    };

    let xfers: HashMap<_, _> = transfers
        .into_iter()
        .filter_map(|transfer| {
            let restore_transfer = || {
                let files = transfer
                    .files
                    .into_iter()
                    .map(|dbfile| {
                        let file_id: FileId = dbfile.file_id.into();
                        let subpath: FileSubPath = dbfile.subpath.into();
                        let uri = dbfile.uri;

                        let file = match uri.scheme() {
                            "file" => file_to_resume_from_path_uri(&uri, subpath, file_id)?,
                            #[cfg(unix)]
                            "content" => {
                                file_to_resume_from_content_uri(state, uri, subpath, file_id)?
                            }
                            unknown => anyhow::bail!("Unknon URI schema: {unknown}"),
                        };

                        anyhow::Ok(file)
                    })
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
                    .context("Failed to fetch transfer sync state")?
                    .context("Missing sync state for transfer")?;

                let mut file_sync = HashMap::new();
                for file_id in xfer.files().keys() {
                    let sync = state
                        .storage
                        .outgoing_file_sync_state(xfer.id(), file_id.as_ref())
                        .context("Failed to fetch file sync state")?
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
                };
                anyhow::Ok(xstate)
            };

            match restore_transfer() {
                Ok(xstate) => Some((xstate.xfer.id(), xstate)),
                Err(err) => {
                    error!(
                        logger,
                        "Failed to restore transfer {}: {err}", transfer.uuid
                    );

                    None
                }
            }
        })
        .collect();

    for xstate in xfers.values() {
        let logger = logger.clone();
        let stop = stop.clone();

        let task = {
            let logger = logger.clone();
            let state = state.clone();
            let xfer = xstate.xfer.clone();

            ws::client::run(state, xfer, logger)
        };

        tokio::spawn(async move {
            tokio::select! {
                biased;

                _ = stop.cancelled() => {
                    warn!(logger, "Aborting transfer resume");
                },
                _ = task => (),
            }
        });
    }

    xfers
}

fn file_to_resume_from_path_uri(
    uri: &url::Url,
    subpath: FileSubPath,
    file_id: FileId,
) -> anyhow::Result<FileToSend> {
    let fullpath = uri
        .to_file_path()
        .ok()
        .context("Failed to extract file path")?;

    let meta = fs::symlink_metadata(&fullpath)
        .with_context(|| format!("Failed to load file: {}", file_id))?;

    anyhow::ensure!(!meta.is_dir(), "Invalid file type");

    FileToSend::new(subpath, fullpath, meta, file_id.clone())
        .with_context(|| format!("Failed to restore file {file_id} from DB"))
}

#[cfg(unix)]
fn file_to_resume_from_content_uri(
    state: &State,
    uri: url::Url,
    subpath: FileSubPath,
    file_id: FileId,
) -> anyhow::Result<FileToSend> {
    let callback = state
        .fdresolv
        .as_ref()
        .context("Encountered content uri but FD resovler callback is missing")?;

    let fd = callback(uri.as_str()).with_context(|| format!("Failed to fetch FD for: {uri}"))?;

    FileToSend::new_from_fd(subpath, uri, fd, file_id.clone())
        .with_context(|| format!("Failed to restore file {file_id} from DB"))
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
