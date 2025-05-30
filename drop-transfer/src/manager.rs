use std::{
    collections::{hash_map::Entry, HashMap},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use drop_config::DropConfig;
use drop_storage::{sync, types::OutgoingFileToRetry, Storage};
use slog::{debug, error, info, trace, warn, Logger};
use tokio::sync::{mpsc::UnboundedSender, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    check,
    file::FileSubPath,
    service::State,
    tasks::AliveGuard,
    transfer::{IncomingTransfer, OutgoingTransfer},
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
        EventTxFactory, FileEventTx, IncomingFileEventTx, IncomingTransferEventTx,
        OutgoingFileEventTx, OutgoingTransferEventTx, TransferEventTx,
    },
    File, FileId, FileToRecv, FileToSend, Transfer,
};

pub struct CloseResult<T: Transfer> {
    pub file_events: Vec<Arc<FileEventTx<T>>>,
    pub xfer_events: Arc<TransferEventTx<T>>,
}

pub struct FinishResult<T: Transfer> {
    pub xfer_state: FinishTransferState<T>,
    pub file_events: Arc<FileEventTx<T>>,
}

pub enum FinishTransferState<T: Transfer> {
    Canceled { events: Arc<TransferEventTx<T>> },
    Alive,
}

pub enum OutgoingConnected {
    JustCancelled {
        events: Arc<OutgoingTransferEventTx>,
    },
    Continue,
}

pub enum IncomingRegistered {
    IsNew {
        events: Arc<IncomingTransferEventTx>,
    },
    Continue,
    JustCancelled {
        events: Arc<IncomingTransferEventTx>,
    },
}

#[derive(Debug, Clone, Copy, strum::FromRepr)]
pub enum FileTerminalState {
    Rejected,
    Completed,
    Failed,
}

enum IncomingLocalFileState {
    Idle,
    InFlight { path: PathBuf },
    Terminal(FileTerminalState),
}

enum OutgoingLocalFileState {
    Alive,
    Terminal(FileTerminalState),
}

pub struct IncomingState {
    pub xfer: Arc<IncomingTransfer>,
    conn: Option<UnboundedSender<ServerReq>>,
    pub dir_mappings: DirMapping,
    xfer_sync: sync::TransferState,
    file_sync: HashMap<FileId, IncomingLocalFileState>,
    file_events: HashMap<FileId, Arc<IncomingFileEventTx>>,
    pub xfer_events: Arc<IncomingTransferEventTx>,
}

pub struct OutgoingState {
    pub xfer: Arc<OutgoingTransfer>,
    conn: Option<UnboundedSender<ClientReq>>,
    xfer_sync: sync::TransferState,
    file_sync: HashMap<FileId, OutgoingLocalFileState>,
    file_events: HashMap<FileId, Arc<OutgoingFileEventTx>>,
    pub xfer_events: Arc<OutgoingTransferEventTx>,
}

/// Transfer manager is responsible for keeping track of all ongoing or pending
/// transfers and their status
pub struct TransferManager {
    pub incoming: Mutex<HashMap<Uuid, IncomingState>>,
    pub outgoing: Mutex<HashMap<Uuid, OutgoingState>>,
    storage: Arc<Storage>,
    logger: Logger,
    event_factory: EventTxFactory,
}

#[derive(Default)]
pub struct DirMapping {
    mappings: HashMap<PathBuf, String>,
}

impl TransferManager {
    pub fn new(storage: Arc<Storage>, event_factory: EventTxFactory, logger: Logger) -> Self {
        Self {
            incoming: Default::default(),
            outgoing: Default::default(),
            storage,
            logger,
            event_factory,
        }
    }

    /// Returns `Some()` if the transfer is new one
    pub async fn register_incoming(
        &self,
        xfer: Arc<IncomingTransfer>,
        conn: UnboundedSender<ServerReq>,
    ) -> anyhow::Result<IncomingRegistered> {
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

                if let Some(conn) = &state.conn {
                    anyhow::ensure!(
                        !conn.is_closed(),
                        "The transfer connection is in progress already"
                    );
                }

                info!(self.logger, "Issuing pending requests for: {}", xfer.id());
                state.issue_pending_requests(&conn, &self.logger);

                match state.xfer_sync {
                    sync::TransferState::Canceled => {
                        debug!(self.logger, "Incoming transfer is locally cancelled");
                        if let Err(e) = conn.send(ServerReq::Close) {
                            warn!(self.logger, "Failed to send close request: {}", e);
                        }
                        drop(conn)
                    }
                    _ => {
                        state.conn = Some(conn);

                        let was_cancelled = state
                            .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
                            .await;

                        match was_cancelled {
                            FinishTransferState::Canceled { events } => {
                                return Ok(IncomingRegistered::JustCancelled { events })
                            }
                            FinishTransferState::Alive => (),
                        }
                    }
                }

                Ok(IncomingRegistered::Continue)
            }
            Entry::Vacant(vacc) => {
                if self
                    .storage
                    .insert_transfer(&xfer.storage_info())
                    .await
                    .is_none()
                {
                    warn!(self.logger, "Transfer was closed already");
                    if let Err(e) = conn.send(ServerReq::Close) {
                        warn!(self.logger, "Failed to send close request: {}", e);
                    }
                    return Ok(IncomingRegistered::Continue);
                }

                self.storage
                    .update_transfer_sync_states(xfer.id(), sync::TransferState::Active)
                    .await;

                let state = vacc.insert(IncomingState {
                    xfer: xfer.clone(),
                    conn: Some(conn),
                    dir_mappings: Default::default(),
                    xfer_sync: sync::TransferState::Active,
                    file_sync: xfer
                        .files()
                        .keys()
                        .map(|file_id| (file_id.clone(), IncomingLocalFileState::Idle))
                        .collect(),
                    file_events: xfer
                        .files()
                        .keys()
                        .map(|file_id| {
                            (
                                file_id.clone(),
                                Arc::new(self.event_factory.file(xfer.clone(), file_id.clone())),
                            )
                        })
                        .collect(),
                    xfer_events: Arc::new(self.event_factory.transfer(xfer, false)),
                });

                Ok(IncomingRegistered::IsNew {
                    events: state.xfer_events.clone(),
                })
            }
        }
    }

    pub async fn is_outgoing_alive(&self, transfer_id: Uuid) -> bool {
        let lock = self.outgoing.lock().await;
        lock.get(&transfer_id).is_some()
    }

    pub async fn outgoing_connected(
        &self,
        transfer_id: Uuid,
        conn: UnboundedSender<ClientReq>,
    ) -> crate::Result<OutgoingConnected> {
        let mut lock = self.outgoing.lock().await;
        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if let sync::TransferState::New = state.xfer_sync {
            self.storage
                .update_transfer_sync_states(transfer_id, sync::TransferState::Active)
                .await;

            state.xfer_sync = sync::TransferState::Active;
        }

        match state.xfer_sync {
            sync::TransferState::Canceled => {
                debug!(self.logger, "Outgoing transfer is locally cancelled");
                if let Err(e) = conn.send(ClientReq::Close) {
                    warn!(self.logger, "Failed to send close request: {}", e);
                }
                drop(conn);
            }
            _ => {
                state.issue_pending_requests(&conn, &self.logger);
                state.conn = Some(conn);

                let was_cancelled = state
                    .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
                    .await;

                match was_cancelled {
                    FinishTransferState::Canceled { events } => {
                        return Ok(OutgoingConnected::JustCancelled { events })
                    }
                    FinishTransferState::Alive => (),
                }
            }
        }

        Ok(OutgoingConnected::Continue)
    }

    pub async fn insert_outgoing(
        &self,
        xfer: Arc<OutgoingTransfer>,
    ) -> crate::Result<Arc<OutgoingTransferEventTx>> {
        let mut lock = self.outgoing.lock().await;

        let state = match lock.entry(xfer.id()) {
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
                    xfer: xfer.clone(),
                    conn: None,
                    xfer_sync: sync::TransferState::New,
                    file_sync: xfer
                        .files()
                        .keys()
                        .map(|file_id| (file_id.clone(), OutgoingLocalFileState::Alive))
                        .collect(),
                    file_events: xfer
                        .files()
                        .keys()
                        .map(|file_id| {
                            (
                                file_id.clone(),
                                Arc::new(self.event_factory.file(xfer.clone(), file_id.clone())),
                            )
                        })
                        .collect(),
                    xfer_events: Arc::new(self.event_factory.transfer(xfer, false)),
                })
            }
        };

        Ok(state.xfer_events.clone())
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

        Ok(state.file_events(file_id)?.clone())
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

        Ok(state.file_events(file_id)?.clone())
    }

    pub async fn outgoing_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<FinishResult<OutgoingTransfer>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        state
            .file_sync_mut(file_id)?
            .try_terminate(FileTerminalState::Rejected)?;

        self.storage
            .update_outgoing_file_sync_states(
                state.xfer.id(),
                file_id.as_ref(),
                sync::FileState::Terminal,
            )
            .await;

        if let Some(conn) = &state.conn {
            debug!(
                self.logger,
                "Pushing outgoing rejection request: file_id {file_id}"
            );

            if let Err(e) = conn.send(ClientReq::Reject {
                file: file_id.clone(),
            }) {
                warn!(self.logger, "Failed to send reject request: {}", e);
            };
        }

        Ok(FinishResult {
            xfer_state: state
                .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
                .await,
            file_events: state.file_events(file_id)?.clone(),
        })
    }

    pub async fn outgoing_terminal_recv(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
        file_state: FileTerminalState,
    ) -> crate::Result<Option<FinishResult<OutgoingTransfer>>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state.file_sync_mut(file_id)?;

        let res = if sync.try_terminate(file_state).is_ok() {
            self.storage
                .update_outgoing_file_sync_states(
                    transfer_id,
                    file_id.as_ref(),
                    sync::FileState::Terminal,
                )
                .await;

            let xfer_state = state
                .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
                .await;

            Some(FinishResult {
                xfer_state,
                file_events: state.file_events(file_id)?.clone(),
            })
        } else {
            None
        };

        Ok(res)
    }

    pub async fn incoming_rejection_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
    ) -> crate::Result<FinishResult<IncomingTransfer>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        let sync = state.file_sync_mut(file_id)?;
        sync.try_terminate_local(FileTerminalState::Rejected)?;

        self.storage
            .stop_incoming_file(state.xfer.id(), file_id.as_ref())
            .await;

        if let Some(conn) = &state.conn {
            debug!(
                self.logger,
                "Pushing incoming rejection request: file_id {file_id}"
            );

            if let Err(e) = conn.send(ServerReq::Reject {
                file: file_id.clone(),
            }) {
                warn!(self.logger, "Failed to send reject request: {}", e);
            };
        }

        let xfer_state = state
            .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
            .await;

        Ok(FinishResult {
            xfer_state,
            file_events: state.file_events(file_id)?.clone(),
        })
    }

    pub async fn incoming_remove(&self, transfer_id: Uuid) -> Option<IncomingState> {
        debug!(self.logger, "Removing incoming transfer: {transfer_id}");
        let mut lock = self.incoming.lock().await;

        let state = lock.remove(&transfer_id)?;
        self.storage.transfer_sync_clear(transfer_id).await;
        Some(state)
    }

    pub async fn is_incoming_alive(&self, transfer_id: Uuid) -> bool {
        let lock = self.incoming.lock().await;

        match lock.get(&transfer_id) {
            Some(state) => !matches!(state.xfer_sync, sync::TransferState::Canceled),
            None => false,
        }
    }

    pub async fn incoming_finish_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
        success: Result<(), String>,
    ) -> crate::Result<FinishTransferState<IncomingTransfer>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        let fstate = state.file_sync_mut(file_id)?;
        fstate.try_terminate_local(if success.is_ok() {
            FileTerminalState::Completed
        } else {
            FileTerminalState::Failed
        })?;

        self.storage
            .stop_incoming_file(transfer_id, file_id.as_ref())
            .await;

        if let Some(conn) = &state.conn {
            let file = file_id.clone();

            let (name, serv_req) = match success {
                Ok(()) => ("DONE", ServerReq::Done { file }),
                Err(msg) => ("FAIL", ServerReq::Fail { file, msg }),
            };

            debug!(self.logger, "Pushing file {name} message");
            if let Err(e) = conn.send(serv_req) {
                warn!(self.logger, "Failed to send {name} message: {e}");
            };
        }

        let xfer_state = state
            .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
            .await;

        Ok(xfer_state)
    }

    pub async fn incoming_terminal_recv(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
        file_state: FileTerminalState,
    ) -> crate::Result<Option<FinishResult<IncomingTransfer>>> {
        let mut lock = self.incoming.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        let sync = state.file_sync_mut(file_id)?;

        let res = if sync.try_terminate_local(file_state).is_ok() {
            self.storage
                .stop_incoming_file(transfer_id, file_id.as_ref())
                .await;

            let xfer_state = state
                .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
                .await;

            Some(FinishResult {
                xfer_state,
                file_events: state.file_events(file_id)?.clone(),
            })
        } else {
            None
        };

        Ok(res)
    }

    pub async fn outgoing_failure_post(
        &self,
        transfer_id: Uuid,
        file_id: &FileId,
        msg: String,
    ) -> crate::Result<FinishResult<OutgoingTransfer>> {
        let mut lock = self.outgoing.lock().await;

        let state = lock
            .get_mut(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        state.ensure_not_cancelled()?;

        let sync = state.file_sync_mut(file_id)?;
        sync.try_terminate(FileTerminalState::Failed)?;

        self.storage
            .update_outgoing_file_sync_states(
                transfer_id,
                file_id.as_ref(),
                sync::FileState::Terminal,
            )
            .await;

        if let Some(conn) = &state.conn {
            debug!(self.logger, "Pushing file FAIL message");
            if let Err(e) = conn.send(ClientReq::Fail {
                file: file_id.clone(),
                msg: msg.to_string(),
            }) {
                warn!(self.logger, "Failed to send FAIL message: {e}");
            };
        }

        let xfer_state = state
            .cancel_transfer_if_all_files_terminated(&self.logger, &self.storage)
            .await;

        Ok(FinishResult {
            xfer_state,
            file_events: state.file_events(file_id)?.clone(),
        })
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
        state.cancel_transfer(&self.logger, &self.storage).await;

        for val in state.file_sync.values_mut() {
            if let IncomingLocalFileState::InFlight { .. } = &*val {
                *val = IncomingLocalFileState::Idle;
            }
        }

        let res = CloseResult {
            file_events: state.file_events.values().cloned().collect(),
            xfer_events: state.xfer_events.clone(),
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

        match state.xfer_sync {
            // If it's new, then suppress transfer synchronization by deleting the state
            sync::TransferState::New => {
                self.storage.transfer_sync_clear(transfer_id).await;

                let res = CloseResult {
                    file_events: state.file_events.values().cloned().collect(),
                    xfer_events: state.xfer_events.clone(),
                };

                lock.remove(&transfer_id);

                Ok(res)
            }
            sync::TransferState::Active => {
                state.cancel_transfer(&self.logger, &self.storage).await;

                Ok(CloseResult {
                    file_events: state.file_events.values().cloned().collect(),
                    xfer_events: state.xfer_events.clone(),
                })
            }
            sync::TransferState::Canceled => Err(crate::Error::BadTransfer),
        }
    }

    pub async fn outgoing_ensure_file_not_terminated(
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

        state.ensure_not_terminated()
    }

    pub async fn outgoing_remove(&self, transfer_id: Uuid) -> Option<OutgoingState> {
        debug!(self.logger, "Removing outgoing transfer: {transfer_id}");
        let mut lock = self.outgoing.lock().await;

        let state = lock.remove(&transfer_id)?;
        self.storage.transfer_sync_clear(transfer_id).await;
        Some(state)
    }

    pub async fn outgoing_event_tx(
        &self,
        transfer_id: Uuid,
    ) -> Option<Arc<OutgoingTransferEventTx>> {
        let lock = self.outgoing.lock().await;
        lock.get(&transfer_id)
            .map(|state| state.xfer_events.clone())
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
        trace!(self.logger, "outgoing_disconnect: {}", transfer_id);
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
    fn issue_pending_requests(&self, conn: &UnboundedSender<ClientReq>, logger: &Logger) {
        let iter = self
            .file_sync
            .iter()
            .filter_map(|(file_id, state)| match state {
                OutgoingLocalFileState::Terminal(FileTerminalState::Rejected) => {
                    info!(logger, "Rejecting file: {file_id}",);

                    Some(ClientReq::Reject {
                        file: file_id.clone(),
                    })
                }
                OutgoingLocalFileState::Terminal(FileTerminalState::Failed) => {
                    info!(logger, "Failing file: {file_id}",);

                    Some(ClientReq::Fail {
                        file: file_id.clone(),
                        msg: String::from("File failed elsewhere"),
                    })
                }
                _ => None,
            });

        for req in iter {
            if let Err(e) = conn.send(req) {
                warn!(logger, "Failed to send request: {}", e);
            }
        }
    }

    fn ensure_not_cancelled(&self) -> crate::Result<()> {
        if let sync::TransferState::Canceled = self.xfer_sync {
            return Err(crate::Error::BadTransfer);
        }
        Ok(())
    }

    fn file_events(&self, file_id: &FileId) -> crate::Result<&Arc<OutgoingFileEventTx>> {
        self.file_events.get(file_id).ok_or(crate::Error::BadFileId)
    }

    fn file_sync_mut(&mut self, file_id: &FileId) -> crate::Result<&mut OutgoingLocalFileState> {
        self.file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)
    }

    async fn cancel_transfer_if_all_files_terminated(
        &mut self,
        logger: &Logger,
        storage: &Storage,
    ) -> FinishTransferState<OutgoingTransfer> {
        let all_terminated = self
            .file_sync
            .values()
            .all(|file_state| matches!(file_state, OutgoingLocalFileState::Terminal(_)));

        if all_terminated {
            debug!(
                logger,
                "All outgoing files terminated, cancelling transfer: {}",
                self.xfer.id()
            );

            self.cancel_transfer(logger, storage).await;
            FinishTransferState::Canceled {
                events: self.xfer_events.clone(),
            }
        } else {
            FinishTransferState::Alive
        }
    }

    async fn cancel_transfer(&mut self, logger: &Logger, storage: &Storage) {
        storage
            .update_transfer_sync_states(
                self.xfer.id(),
                drop_storage::sync::TransferState::Canceled,
            )
            .await;
        self.xfer_sync = sync::TransferState::Canceled;

        if let Some(conn) = self.conn.take() {
            debug!(logger, "Pushing outgoing  close request");

            if let Err(e) = conn.send(ClientReq::Close) {
                warn!(logger, "Failed to send close request: {}", e);
            }
        }
    }
}

impl IncomingState {
    /// Returs `true` when the new download can be started and `false` in case
    /// the downaload is already happening
    pub fn validate_for_download(&self, file_id: &FileId) -> crate::Result<bool> {
        self.ensure_not_cancelled()?;

        let state = self.file_sync.get(file_id).ok_or(crate::Error::BadFileId)?;
        let start = match state {
            IncomingLocalFileState::Idle => true,
            IncomingLocalFileState::InFlight { .. } => false,
            IncomingLocalFileState::Terminal(term) => {
                return Err(crate::Error::FileStateMismatch(*term));
            }
        };

        Ok(start)
    }

    pub async fn start_download(
        &mut self,
        storage: &Storage,
        file_id: &FileId,
        parent_dir: &Path,
        logger: &Logger,
    ) -> crate::Result<()> {
        let state = self.file_sync_mut(file_id)?;

        state.ensure_not_terminated()?;
        *state = IncomingLocalFileState::InFlight {
            path: parent_dir.to_path_buf(),
        };

        storage
            .start_incoming_file(
                self.xfer.id(),
                file_id.as_ref(),
                &parent_dir.to_string_lossy(),
            )
            .await;

        let file = &self.xfer.files()[file_id];

        if let Some(conn) = &self.conn {
            let task = FileXferTask::new(file.clone(), self.xfer.clone(), parent_dir.into());

            debug!(logger, "Pushing download request: file_id {file_id}");

            if let Err(e) = conn.send(ServerReq::Download {
                task: Box::new(task),
            }) {
                warn!(logger, "Failed to send download request: {}", e);
            };
        }

        Ok(())
    }

    pub fn file_events(&self, file_id: &FileId) -> crate::Result<&Arc<IncomingFileEventTx>> {
        self.file_events.get(file_id).ok_or(crate::Error::BadFileId)
    }

    fn ensure_not_cancelled(&self) -> crate::Result<()> {
        if let sync::TransferState::Canceled = self.xfer_sync {
            return Err(crate::Error::BadTransfer);
        }
        Ok(())
    }

    fn issue_pending_requests(&self, conn: &UnboundedSender<ServerReq>, logger: &Logger) {
        let iter = self
            .file_sync
            .iter()
            .filter_map(|(file_id, state)| match state {
                IncomingLocalFileState::InFlight { path } => {
                    info!(logger, "Resuming file: {file_id}",);

                    let xfile = &self.xfer.files()[file_id];
                    let task = FileXferTask::new(xfile.clone(), self.xfer.clone(), path.into());
                    Some(ServerReq::Download {
                        task: Box::new(task),
                    })
                }
                IncomingLocalFileState::Terminal(FileTerminalState::Rejected) => {
                    info!(logger, "Rejecting file: {file_id}",);

                    Some(ServerReq::Reject {
                        file: file_id.clone(),
                    })
                }
                IncomingLocalFileState::Terminal(FileTerminalState::Completed) => {
                    info!(logger, "Finishing file: {file_id}",);

                    Some(ServerReq::Done {
                        file: file_id.clone(),
                    })
                }
                IncomingLocalFileState::Terminal(FileTerminalState::Failed) => {
                    info!(logger, "Failing file: {file_id}",);

                    Some(ServerReq::Fail {
                        file: file_id.clone(),
                        msg: String::from(
                            "File failed. The failed state was retrieved from the database.",
                        ),
                    })
                }
                _ => None,
            });

        for req in iter {
            if let Err(e) = conn.send(req) {
                warn!(logger, "Failed to send request: {}", e);
            }
        }
    }

    fn file_sync_mut(&mut self, file_id: &FileId) -> crate::Result<&mut IncomingLocalFileState> {
        self.file_sync
            .get_mut(file_id)
            .ok_or(crate::Error::BadFileId)
    }

    async fn cancel_transfer_if_all_files_terminated(
        &mut self,
        logger: &Logger,
        storage: &Storage,
    ) -> FinishTransferState<IncomingTransfer> {
        let all_terminated = self
            .file_sync
            .values()
            .all(|file_state| matches!(file_state, IncomingLocalFileState::Terminal(_)));

        if all_terminated {
            debug!(
                logger,
                "All incoming files terminated, cancelling transfer: {}",
                self.xfer.id()
            );

            self.cancel_transfer(logger, storage).await;
            FinishTransferState::Canceled {
                events: self.xfer_events.clone(),
            }
        } else {
            FinishTransferState::Alive
        }
    }

    async fn cancel_transfer(&mut self, logger: &Logger, storage: &Storage) {
        storage
            .update_transfer_sync_states(self.xfer.id(), sync::TransferState::Canceled)
            .await;

        self.xfer_sync = sync::TransferState::Canceled;

        if let Some(conn) = self.conn.take() {
            debug!(logger, "Pushing incoming close request");

            if let Err(e) = conn.send(ServerReq::Close) {
                warn!(logger, "Failed to send close request: {}", e);
            }
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

    fn register_preexisting_final_path(
        &mut self,
        file_subpath: &FileSubPath,
        full_path: impl AsRef<Path>,
    ) {
        self.mappings
            .extend(extract_directory_mapping(file_subpath, full_path.as_ref()));
    }
}

impl IncomingLocalFileState {
    fn ensure_not_terminated(&self) -> crate::Result<()> {
        match self {
            Self::Terminal(term) => Err(crate::Error::FileStateMismatch(*term)),
            _ => Ok(()),
        }
    }

    fn try_terminate_local(&mut self, to_set: FileTerminalState) -> crate::Result<()> {
        match self {
            IncomingLocalFileState::Idle | IncomingLocalFileState::InFlight { .. } => {
                *self = IncomingLocalFileState::Terminal(to_set);
                Ok(())
            }
            IncomingLocalFileState::Terminal(state) => Err(crate::Error::FileStateMismatch(*state)),
        }
    }
}

impl OutgoingLocalFileState {
    fn ensure_not_terminated(&self) -> crate::Result<()> {
        match self {
            Self::Terminal(term) => Err(crate::Error::FileStateMismatch(*term)),
            _ => Ok(()),
        }
    }

    fn try_terminate(&mut self, to_set: FileTerminalState) -> crate::Result<()> {
        match self {
            OutgoingLocalFileState::Alive => {
                *self = OutgoingLocalFileState::Terminal(to_set);
                Ok(())
            }
            OutgoingLocalFileState::Terminal(state) => Err(crate::Error::FileStateMismatch(*state)),
        }
    }
}

pub(crate) async fn restore_transfers_state(state: &Arc<State>, logger: &Logger) {
    let incoming = restore_incoming(
        &state.transfer_manager.event_factory,
        &state.storage,
        &state.config,
        logger,
    )
    .await;
    *state.transfer_manager.incoming.lock().await = incoming;

    let outgoing = restore_outgoing(state, logger).await;
    *state.transfer_manager.outgoing.lock().await = outgoing;
}

pub(crate) async fn resume(
    refresh_trigger: &tokio::sync::watch::Receiver<()>,
    state: &Arc<State>,
    logger: &Logger,
    guard: &AliveGuard,
    stop: &CancellationToken,
) {
    {
        let xfers = state.transfer_manager.outgoing.lock().await;

        for xstate in xfers.values() {
            let trig = refresh_trigger.clone();
            ws::client::spawn(
                trig,
                state.clone(),
                xstate.xfer.clone(),
                logger.clone(),
                guard.clone(),
                stop.clone(),
            );
        }
    }

    {
        let xfers = state.transfer_manager.incoming.lock().await;

        for xstate in xfers.values() {
            let trig = refresh_trigger.clone();
            check::spawn(
                trig,
                state.clone(),
                xstate.xfer.clone(),
                logger.clone(),
                guard.clone(),
                stop.clone(),
            );
        }
    }
}

async fn restore_incoming(
    factory: &EventTxFactory,
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
                let state = storage
                    .incoming_file_sync_state(xfer.id(), file_id.as_ref())
                    .await
                    .context("Missing sync state for file")?;

                let local = if state.is_rejected {
                    IncomingLocalFileState::Terminal(FileTerminalState::Rejected)
                } else if state.is_success {
                    IncomingLocalFileState::Terminal(FileTerminalState::Completed)
                } else if state.is_failed {
                    IncomingLocalFileState::Terminal(FileTerminalState::Failed)
                } else {
                    match state.sync {
                        sync::FileState::Alive => IncomingLocalFileState::Idle,
                        sync::FileState::Terminal => {
                            IncomingLocalFileState::Terminal(FileTerminalState::Failed)
                        } // Assume it's failed
                    }
                };

                file_sync.insert(file_id.clone(), local);
            }

            let in_flights = storage.incoming_files_to_resume(xfer.id()).await;

            for file in in_flights {
                if let Some(state) = file_sync.get_mut(&file.file_id) {
                    if state.ensure_not_terminated().is_ok() {
                        *state = IncomingLocalFileState::InFlight {
                            path: file.base_dir.into(),
                        };
                    }
                }
            }

            let xfer = Arc::new(xfer);
            let mut xstate = IncomingState {
                xfer: xfer.clone(),
                conn: None,
                dir_mappings: Default::default(),
                xfer_sync: sync.local_state,
                file_sync,
                file_events: xfer
                    .files()
                    .keys()
                    .map(|file_id| {
                        (
                            file_id.clone(),
                            Arc::new(factory.file(xfer.clone(), file_id.clone())),
                        )
                    })
                    .collect(),
                xfer_events: Arc::new(factory.transfer(
                    xfer,
                    matches!(sync.local_state, sync::TransferState::Canceled),
                )),
            };

            debug!(
                logger,
                "Restoring transfer: {}, state: {:?}",
                xstate.xfer.id(),
                xstate.xfer_sync,
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

async fn restore_outgoing(state: &Arc<State>, logger: &Logger) -> HashMap<Uuid, OutgoingState> {
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
                let state = state
                    .storage
                    .outgoing_file_sync_state(xfer.id(), file_id.as_ref())
                    .await
                    .context("Missing sync state for file")?;

                let local = if state.is_rejected {
                    OutgoingLocalFileState::Terminal(FileTerminalState::Rejected)
                } else if state.is_success {
                    OutgoingLocalFileState::Terminal(FileTerminalState::Completed)
                } else if state.is_failed {
                    OutgoingLocalFileState::Terminal(FileTerminalState::Failed)
                } else {
                    match state.sync {
                        sync::FileState::Alive => OutgoingLocalFileState::Alive,
                        sync::FileState::Terminal => {
                            OutgoingLocalFileState::Terminal(FileTerminalState::Failed)
                        } // Assume it's failed
                    }
                };

                file_sync.insert(file_id.clone(), local);
            }

            let xfer = Arc::new(xfer);
            let xstate = OutgoingState {
                xfer: xfer.clone(),
                conn: None,
                xfer_sync: sync.local_state,
                file_sync,
                file_events: xfer
                    .files()
                    .keys()
                    .map(|file_id| {
                        (
                            file_id.clone(),
                            Arc::new(
                                state
                                    .transfer_manager
                                    .event_factory
                                    .file(xfer.clone(), file_id.clone()),
                            ),
                        )
                    })
                    .collect(),
                xfer_events: Arc::new(state.transfer_manager.event_factory.transfer(
                    xfer,
                    matches!(sync.local_state, sync::TransferState::Canceled),
                )),
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
            .all(|(key, val)| existing.files().get(key).is_some_and(|v| {
                val.id() == v.id() && val.size() == v.size() && val.mime_type() == v.mime_type()
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
