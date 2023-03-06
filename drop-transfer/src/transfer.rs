use std::{collections::HashMap, net::IpAddr, path::Path};

use drop_config::DropConfig;
use uuid::Uuid;

use crate::{utils::Hidden, Error, File};

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub struct Transfer {
    peer: IpAddr,
    uuid: Uuid,
    files: HashMap<Hidden<Box<Path>>, File>,
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
            .map(|file| {
                Ok((
                    Hidden(
                        AsRef::<Path>::as_ref(&file.path().file_name().ok_or(Error::BadPath)?)
                            .into(),
                    ),
                    file,
                ))
            })
            .collect::<Result<_>>()?;

        Ok(Self { peer, uuid, files })
    }

    pub(crate) fn file(&self, path: &Path) -> Option<&File> {
        let mut components = path.components();

        if let Some(root) = components.next() {
            if let Some(file) = self
                .files
                .get(&AsRef::<Path>::as_ref(&root).to_path_buf().into_boxed_path())
            {
                if let Some(child) = file.child(components.as_path()) {
                    return Some(child);
                }

                return Some(file);
            }
        }

        None
    }

    pub fn files(&self) -> &HashMap<Hidden<Box<Path>>, File> {
        &self.files
    }

    // Gathers all files into a flat list
    pub fn flat_file_list(&self) -> Vec<&File> {
        self.files()
            .values()
            .into_iter()
            .flat_map(|f| {
                if f.is_dir() {
                    f.iter().filter(|c| !c.is_dir()).collect::<Vec<_>>()
                } else {
                    vec![f]
                }
            })
            .collect()
    }

    // Used for moose only
    pub fn sizes_kb(&self) -> Vec<i32> {
        self.flat_file_list()
            .iter()
            .map(|f| f.size_kb().unwrap_or_default())
            .collect()
    }

    pub fn id(&self) -> Uuid {
        self.uuid
    }

    pub fn peer(&self) -> IpAddr {
        self.peer
    }
}
