use std::{collections::HashMap, net::IpAddr};

use drop_analytics::TransferInfo;
use drop_config::DropConfig;
use drop_storage::TransferInfo as StorageInfo;
use uuid::Uuid;

use crate::{
    file::{FileId, FileKind, FileSource, FileSubPath},
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
        // TODO(msz): this insane check wouldn't be needed if we had two different
        // `Transfer` types
        let is_incoming = match self.files.values().next() {
            Some(files) => matches!(files.kind, FileKind::FileToRecv { .. }),
            None => true, // TODO(msz): Arbitrarily chosen, there is no way to differentiate here
        };

        let files = if is_incoming {
            let files = self
                .files
                .values()
                .map(|f| drop_storage::types::TransferIncomingPath {
                    file_id: f.file_id.to_string(),
                    relative_path: f.subpath.to_string(),
                    size: f.size() as _,
                })
                .collect();

            drop_storage::types::TransferFiles::Incoming(files)
        } else {
            let files = self
                .files
                .values()
                .filter_map(|f| {
                    let base_path = match &f.kind {
                        FileKind::FileToSend { source, .. } => match source {
                            FileSource::Path(fullpath) => fullpath
                                .ancestors()
                                .nth(f.subpath.iter().count())?
                                .to_str()?,
                            #[cfg(unix)]
                            FileSource::Fd(_) => ".", /* Let's pretend the files are in the
                                                       * working dir. The FDs are only used on
                                                       * Android, */
                        },
                        _ => return None,
                    };

                    Some(drop_storage::types::TransferOutgoingPath {
                        file_id: f.file_id.to_string(),
                        relative_path: f.subpath.to_string(),
                        base_path: base_path.to_string(),
                        size: f.size() as _,
                    })
                })
                .collect();

            drop_storage::types::TransferFiles::Outgoing(files)
        };

        StorageInfo {
            id: self.id(),
            peer: self.peer().to_string(),
            files,
        }
    }

    pub fn id(&self) -> Uuid {
        self.uuid
    }

    pub fn peer(&self) -> IpAddr {
        self.peer
    }
}
