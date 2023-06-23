use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    mem::ManuallyDrop,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::{
    service::State,
    ws::{client::ClientReq, server::ServerReq},
    Error, FileId, Transfer,
};

#[derive(Clone)]
pub enum TransferConnection {
    Client(UnboundedSender<ClientReq>),
    Server(UnboundedSender<ServerReq>),
}

pub struct TransferState {
    pub(crate) xfer: Transfer,
    pub(crate) connection: TransferConnection,
    // Used for mapping directories inside the destination
    dir_mappings: HashMap<PathBuf, String>,

    rejected: HashSet<FileId>,
}

/// Transfer manager is responsible for keeping track of all ongoing or pending
/// transfers and their status
#[derive(Default)]
pub(crate) struct TransferManager {
    transfers: HashMap<Uuid, TransferState>,
}

impl TransferState {
    fn new(xfer: Transfer, connection: TransferConnection) -> Self {
        Self {
            xfer,
            connection,
            dir_mappings: HashMap::new(),
            rejected: HashSet::new(),
        }
    }
}

impl TransferManager {
    /// Cancel ALL of the ongoing file transfers for a given transfer ID    
    pub(crate) fn cancel_transfer(&mut self, transfer_id: Uuid) -> Result<(), Error> {
        self.transfers
            .remove(&transfer_id)
            .ok_or(Error::BadTransfer)?;

        Ok(())
    }

    pub(crate) fn insert_transfer(
        &mut self,
        xfer: Transfer,
        connection: TransferConnection,
    ) -> crate::Result<()> {
        match self.transfers.entry(xfer.id()) {
            Entry::Occupied(_) => Err(Error::BadTransferState("Transfer already exists".into())),
            Entry::Vacant(entry) => {
                entry.insert(TransferState::new(xfer, connection));
                Ok(())
            }
        }
    }

    /// Returns `true` if file was sucesfully marked as rejected and `false` if
    /// it was already marked as such
    pub(crate) fn reject_file(&mut self, id: Uuid, file: FileId) -> crate::Result<bool> {
        let xstate = self
            .transfers
            .get_mut(&id)
            .ok_or(crate::Error::BadTransfer)?;
        if !xstate.xfer.files().contains_key(&file) {
            return Err(crate::Error::BadFileId);
        }

        Ok(xstate.rejected.insert(file))
    }

    pub(crate) fn ensure_file_not_rejected(
        &self,
        transfer_id: Uuid,
        file: &FileId,
    ) -> crate::Result<()> {
        let xstate = self
            .transfers
            .get(&transfer_id)
            .ok_or(crate::Error::BadTransfer)?;

        if !xstate.xfer.files().contains_key(file) {
            return Err(crate::Error::BadFileId);
        }

        if xstate.rejected.contains(file) {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    pub(crate) fn transfer(&self, id: &Uuid) -> Option<&Transfer> {
        self.transfers.get(id).map(|state| &state.xfer)
    }

    pub(crate) fn connection(&self, id: Uuid) -> Option<&TransferConnection> {
        self.transfers.get(&id).map(|state| &state.connection)
    }

    pub(crate) fn apply_dir_mapping(
        &mut self,
        id: Uuid,
        dest_dir: &Path,
        file_id: &FileId,
    ) -> crate::Result<PathBuf> {
        let state = self
            .transfers
            .get_mut(&id)
            .ok_or(crate::Error::BadTransfer)?;

        let file = state
            .xfer
            .files()
            .get(file_id)
            .ok_or(crate::Error::BadFileId)?;

        let mut iter = file.subpath().iter().map(crate::utils::normalize_filename);

        let probe = iter.next().ok_or_else(|| {
            crate::Error::BadPath("Path should contain at least one component".into())
        })?;
        let next = iter.next();

        let mapped = match next {
            Some(next) => {
                // Check if dir exists and is known to us
                let name = match state.dir_mappings.entry(dest_dir.join(probe)) {
                    // Dir is known, reuse
                    Entry::Occupied(occ) => occ.get().clone(),
                    // Dir in new, check if there is name conflict and add to known
                    Entry::Vacant(vacc) => {
                        let mapped = crate::utils::map_path_if_exists(vacc.key())?;
                        vacc.insert(
                            mapped
                                .file_name()
                                .ok_or_else(|| crate::Error::BadPath("Missing file name".into()))?
                                .to_string_lossy()
                                .to_string(),
                        )
                        .clone()
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
}

pub(crate) struct TransferGuard {
    state: ManuallyDrop<Arc<State>>,
    id: Uuid,
}

impl TransferGuard {
    pub(crate) fn new(state: Arc<State>, id: Uuid) -> Self {
        Self {
            state: ManuallyDrop::new(state),
            id,
        }
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let state = unsafe { ManuallyDrop::take(&mut self.state) };
        let id = self.id;

        tokio::spawn(async move {
            let mut lock = state.transfer_manager.lock().await;
            let _ = lock.cancel_transfer(id);
        });
    }
}
