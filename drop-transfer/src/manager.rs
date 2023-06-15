use std::{
    collections::{hash_map::Entry, HashMap},
    io,
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
    dir_maker_file_name: String,
}

/// Transfer manager is responsible for keeping track of all ongoing or pending
/// transfers and their status
#[derive(Default)]
pub(crate) struct TransferManager {
    transfers: HashMap<Uuid, TransferState>,
}

impl TransferState {
    fn new(xfer: Transfer, connection: TransferConnection) -> Self {
        let dir_maker_file_name = format!(".{}.drop-dir", xfer.id());

        Self {
            xfer,
            connection,
            dir_mappings: HashMap::new(),
            dir_maker_file_name,
        }
    }

    pub fn rm_all_marker_files(&self) {
        for (full_original, mapping) in &self.dir_mappings {
            let marker_file = full_original
                .with_file_name(mapping)
                .join(&self.dir_maker_file_name);

            let _ = std::fs::remove_file(&marker_file);
        }
    }

    pub(crate) fn apply_dir_mapping(
        &mut self,
        dest_dir: &Path,
        file_id: &FileId,
    ) -> crate::Result<PathBuf> {
        let file = self
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
                let name = match self.dir_mappings.entry(dest_dir.join(probe)) {
                    // Dir is known, reuse
                    Entry::Occupied(occ) => occ.get().clone(),
                    // Dir in new, check if there is name conflict and add to known
                    Entry::Vacant(vacc) => {
                        let mapped = crate::utils::filepath_variants(vacc.key())?.find(|dst_location| {
                                // Skip if there is already a file with the same name.
                                // Additionaly there could be a dangling symlink with the same name,
                                // the `symlink_metadata()` ensures we can catch that.
                                matches!(dst_location.symlink_metadata() , Err(err) if err.kind() == io::ErrorKind::NotFound) ||
                                // Or it might be our dir created by the same transfer (as this can be a resume)
                                dst_location.join(&self.dir_maker_file_name).symlink_metadata().is_ok()
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

                        // Create marker file
                        std::fs::create_dir_all(&mapped)?;
                        let marker_path = mapped.join(&self.dir_maker_file_name);
                        std::fs::File::create(marker_path)?;

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
}

impl TransferManager {
    /// Cancel ALL of the ongoing file transfers for a given transfer ID    
    pub(crate) fn cancel_transfer(&mut self, transfer_id: Uuid) -> Option<TransferState> {
        self.transfers.remove(&transfer_id)
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

    pub(crate) fn state(&self, id: Uuid) -> Option<&TransferState> {
        self.transfers.get(&id)
    }

    pub(crate) fn state_mut(&mut self, id: Uuid) -> Option<&mut TransferState> {
        self.transfers.get_mut(&id)
    }
}

pub(crate) struct TransferGuard {
    state: Option<Arc<State>>,
    id: Uuid,
}

impl TransferGuard {
    pub(crate) fn new(state: Arc<State>, id: Uuid) -> Self {
        Self {
            state: Some(state),
            id,
        }
    }

    pub(crate) async fn gracefull_close(mut self) {
        let state = self
            .state
            .take()
            .expect("State should be present in method calls");

        let mut lock = state.transfer_manager.lock().await;
        if let Some(state) = lock.cancel_transfer(self.id) {
            state.rm_all_marker_files();
        }
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            let id = self.id;
            tokio::spawn(async move {
                let mut lock = state.transfer_manager.lock().await;
                lock.cancel_transfer(id);
            });
        }
    }
}
