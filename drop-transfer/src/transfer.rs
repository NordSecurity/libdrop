use std::{collections::HashMap, net::IpAddr};

use drop_analytics::TransferInfo;
use drop_config::DropConfig;
use uuid::Uuid;

use crate::{file::FileId, utils::Hidden, Error, File};

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub struct Transfer {
    peer: IpAddr,
    uuid: Uuid,
    files: HashMap<Hidden<String>, File>,
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
            .map(|file| (Hidden(file.name().to_string()), file))
            .collect();

        Ok(Self { peer, uuid, files })
    }

    pub(crate) fn file(&self, file_id: &FileId) -> Option<&File> {
        let mut components = file_id.iter();

        let first = components.next()?;
        let mut file = self.files.get(first)?;

        for name in components {
            file = file.child(name)?;
        }

        Some(file)
    }

    pub fn files(&self) -> &HashMap<Hidden<String>, File> {
        &self.files
    }

    // Gathers all files into a flat list
    pub fn flat_file_list(&self) -> Vec<(FileId, &File)> {
        fn push_children<'a>(file: &'a File, file_id: &FileId, out: &mut Vec<(FileId, &'a File)>) {
            if file.is_dir() {
                for file in file.children() {
                    let mut file_id = file_id.clone();
                    file_id.append(file.name().to_string());
                    push_children(file, &file_id, out);
                }
            } else {
                out.push((file_id.clone(), file));
            }
        }

        let mut out = Vec::new();

        for file in self.files().values() {
            let file_id = FileId::from_name(file.name().to_string());
            push_children(file, &file_id, &mut out);
        }

        out
    }

    pub fn info(&self) -> TransferInfo {
        let info_list = self
            .flat_file_list()
            .iter()
            .map(|(_, f)| f.info().unwrap_or_default())
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

    pub fn id(&self) -> Uuid {
        self.uuid
    }

    pub fn peer(&self) -> IpAddr {
        self.peer
    }
}
