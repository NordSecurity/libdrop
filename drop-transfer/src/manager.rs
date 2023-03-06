use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::{
    service::State,
    ws::{client::ClientReq, server::ServerReq},
    Error, File, Transfer,
};

#[derive(Clone)]
pub enum TransferConnection {
    Client(UnboundedSender<ClientReq>),
    Server(UnboundedSender<ServerReq>),
}

pub struct TransferState {
    pub(crate) xfer: Transfer,
    files: HashSet<PathBuf>,
    pub(crate) connection: TransferConnection,
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
            files: HashSet::new(),
            connection,
        }
    }
}

impl TransferManager {
    /// Get ALL of the ongoing file transfers for a given transfer ID
    /// returns None if a transfer does not exist
    pub(crate) fn get_transfer_files(&self, transfer_id: Uuid) -> Option<Vec<PathBuf>> {
        self.transfers
            .get(&transfer_id)
            .map(|state| state.files.iter().cloned().collect())
    }

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
            Entry::Occupied(_) => Err(Error::BadTransferState),
            Entry::Vacant(entry) => {
                let state = entry.insert(TransferState::new(xfer.clone(), connection));

                for file in xfer.files() {
                    state.files.insert(file.0.to_path_buf());

                    let children = file.1.iter().collect::<Vec<&File>>();

                    for child in children {
                        let path = if let Ok(path) = child
                            .path()
                            .strip_prefix(file.1.path().parent().unwrap_or_else(|| Path::new("")))
                        {
                            path
                        } else {
                            child.path()
                        };

                        state.files.insert(path.to_path_buf());
                    }
                }

                Ok(())
            }
        }
    }

    pub(crate) fn transfer(&self, id: &Uuid) -> Option<&Transfer> {
        self.transfers.get(id).map(|state| &state.xfer)
    }

    pub(crate) fn connection(&self, id: Uuid) -> Option<&TransferConnection> {
        self.transfers.get(&id).map(|state| &state.connection)
    }
}

pub(crate) struct TransferGuard {
    state: Arc<State>,
    id: Uuid,
}

impl TransferGuard {
    pub(crate) fn new(state: Arc<State>, id: Uuid) -> Self {
        Self { state, id }
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let state = self.state.clone();
        let id = self.id;

        tokio::spawn(async move {
            let mut lock = state.transfer_manager.lock().await;
            let _ = lock.cancel_transfer(id);
        });
    }
}
