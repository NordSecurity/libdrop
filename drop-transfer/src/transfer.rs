use std::{collections::HashMap, net::IpAddr};

use drop_analytics::TransferInfo;
use drop_config::DropConfig;
use drop_storage::{TransferInfo as StorageInfo, TransferPath};
use uuid::Uuid;

use crate::{
    file::{FileId, FileSubPath},
    Error, File,
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub struct Transfer {
    peer: IpAddr,
    uuid: Uuid,

    // all the files
    files: HashMap<FileId, File>,
}

impl Transfer {
    pub fn new(peer: IpAddr, files: Vec<File>, config: &DropConfig) -> Result<Self> {
        Self::new_with_uuid(peer, files, Uuid::new_v4(), config)
    }

    pub(crate) fn new_with_uuid(
        peer: IpAddr,
        files: Vec<File>,
        uuid: Uuid,
        config: &DropConfig,
    ) -> Result<Self> {
        if files.len() > config.transfer_file_limit {
            return Err(Error::TransferLimitsExceeded);
        }

        let files = files
            .into_iter()
            .map(|file| (file.file_id.clone(), file))
            .collect();

        Ok(Self { peer, uuid, files })
    }

    pub(crate) fn file_by_subpath(&self, file_subpath: &FileSubPath) -> Option<&File> {
        self.files
            .values()
            .find(|file| file.subpath == *file_subpath)
    }

    pub fn files(&self) -> &HashMap<FileId, File> {
        &self.files
    }

    pub fn info(&self) -> TransferInfo {
        let info_list = self
            .files
            .values()
            .map(|f| f.info().unwrap_or_default())
            .collect::<Vec<_>>();

        let size_list = info_list
            .iter()
            .map(|info| info.size_kb)
            .collect::<Vec<i32>>();

        TransferInfo {
            mime_type_list: info_list
                .iter()
                .map(|info| info.mime_type.as_str())
                .collect::<Vec<_>>()
                .join(","),
            extension_list: info_list
                .iter()
                .map(|info| info.extension.as_str())
                .collect::<Vec<_>>()
                .join(","),
            file_size_list: size_list
                .iter()
                .map(i32::to_string)
                .collect::<Vec<String>>()
                .join(","),
            transfer_size_kb: size_list.iter().sum(),
            file_count: info_list.len() as i32,
        }
    }

    pub fn storage_info(&self) -> StorageInfo {
        StorageInfo {
            id: self.id().to_string(),
            peer: self.peer().to_string(),
            files: self
                .files()
                .iter()
                .map(|(_, file)| TransferPath {
                    id: file.id().to_string(),
                    path: file.subpath().to_string(),
                    size: file.size() as i64,
                })
                .collect(),
        }
    }

    pub fn id(&self) -> Uuid {
        self.uuid
    }

    pub fn peer(&self) -> IpAddr {
        self.peer
    }
}
