use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    io,
    mem::ManuallyDrop,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::{
    file::FileSubPath,
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

    /// Returns `true` if file was sucesfully marked as rejected and `false` if
    /// it was already marked as such
    pub(crate) fn reject_file(&mut self, file: FileId) -> crate::Result<bool> {
        if !self.xfer.files().contains_key(&file) {
            return Err(crate::Error::BadFileId);
        }

        Ok(self.rejected.insert(file))
    }

    pub(crate) fn ensure_file_not_rejected(&self, file: &FileId) -> crate::Result<()> {
        if !self.xfer.files().contains_key(file) {
            return Err(crate::Error::BadFileId);
        }

        if self.rejected.contains(file) {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

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
    pub(crate) fn compose_final_path(
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
                let name = match self.dir_mappings.entry(dest_dir.join(probe)) {
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
