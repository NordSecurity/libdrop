use std::{collections::HashMap, net::IpAddr};

use drop_analytics::{TransferDirection, TransferIntentEventData};
use drop_config::DropConfig;
use drop_storage::TransferInfo as StorageInfo;
use uuid::Uuid;

use crate::{
    file::{File, FileId, FileSource, FileSubPath, FileToRecv, FileToSend},
    utils, Error,
};

pub type IncomingTransfer = TransferData<FileToRecv>;
pub type OutgoingTransfer = TransferData<FileToSend>;

pub trait Transfer {
    type File: File;

    fn id(&self) -> Uuid;
    fn peer(&self) -> IpAddr;
    fn files(&self) -> &HashMap<FileId, Self::File>;

    fn direction() -> TransferDirection;

    fn file_by_subpath(&self, file_subpath: &FileSubPath) -> Option<&Self::File> {
        self.files()
            .values()
            .find(|file| file.subpath() == file_subpath)
    }

    fn info(&self) -> TransferIntentEventData {
        let size_list = self
            .files()
            .values()
            .map(|f| utils::to_kb(f.size()))
            .collect::<Vec<i32>>();

        TransferIntentEventData {
            transfer_id: self.id().to_string(),
            path_ids: self
                .files()
                .values()
                .map(|f| f.id().to_string())
                .collect::<Vec<_>>()
                .join(","),
            mime_types: self
                .files()
                .values()
                .map(|f| f.mime_type())
                .collect::<Vec<_>>()
                .join(","),
            extensions: self
                .files()
                .values()
                .map(|f| f.subpath().extension().unwrap_or("none"))
                .collect::<Vec<_>>()
                .join(","),
            file_sizes: size_list
                .iter()
                .map(i32::to_string)
                .collect::<Vec<String>>()
                .join(","),
            transfer_size: size_list.iter().sum(),
            file_count: self.files().len() as i32,
        }
    }
}

#[derive(Debug)]
pub struct TransferData<F: File> {
    peer: IpAddr,
    uuid: Uuid,

    // all the files
    files: HashMap<FileId, F>,
}

impl<F: File> TransferData<F> {
    pub fn new(peer: IpAddr, files: Vec<F>, config: &DropConfig) -> crate::Result<Self> {
        Self::new_with_uuid(peer, files, Uuid::new_v4(), config)
    }

    pub(crate) fn new_with_uuid(
        peer: IpAddr,
        files: Vec<F>,
        uuid: Uuid,
        config: &DropConfig,
    ) -> crate::Result<Self> {
        if files.is_empty() {
            return Err(Error::EmptyTransfer);
        }

        if files.len() > config.transfer_file_limit {
            return Err(Error::TransferLimitsExceeded);
        }

        let files = files
            .into_iter()
            .map(|file| (file.id().clone(), file))
            .collect();

        Ok(Self { peer, uuid, files })
    }
}

impl<F: File> Transfer for TransferData<F> {
    type File = F;

    fn id(&self) -> Uuid {
        self.uuid
    }

    fn peer(&self) -> IpAddr {
        self.peer
    }

    fn files(&self) -> &HashMap<FileId, Self::File> {
        &self.files
    }

    fn direction() -> TransferDirection {
        Self::File::direction()
    }
}

impl IncomingTransfer {
    pub(crate) fn storage_info(&self) -> StorageInfo {
        let files = self
            .files
            .values()
            .map(|f| drop_storage::types::TransferIncomingPath {
                file_id: f.id().to_string(),
                relative_path: f.subpath().to_string(),
                size: f.size() as _,
            })
            .collect();

        StorageInfo {
            id: self.id(),
            peer: self.peer().to_string(),
            files: drop_storage::types::TransferFiles::Incoming(files),
        }
    }
}

impl OutgoingTransfer {
    pub(crate) fn storage_info(&self) -> StorageInfo {
        let files = self
            .files
            .values()
            .filter_map(|f| {
                let uri = match &f.source {
                    FileSource::Path(fullpath) => url::Url::from_file_path(&fullpath.0).ok()?,
                    #[cfg(unix)]
                    FileSource::Fd { content_uri, .. } => content_uri.clone(),
                };

                Some(drop_storage::types::TransferOutgoingPath {
                    file_id: f.id().to_string(),
                    relative_path: f.subpath().to_string(),
                    uri,
                    size: f.size() as _,
                })
            })
            .collect();

        let files = drop_storage::types::TransferFiles::Outgoing(files);
        StorageInfo {
            id: self.id(),
            peer: self.peer().to_string(),
            files,
        }
    }
}
