//! # File download flow
//!
//! * client (sender)   -> server (receiver): `TransferRequest`
//!
//! * server (receiver) ->   client (sender): `Start (file)`
//! * client (sender)   -> server (receiver): `Chunk (file)`
//! * server (receiver) ->   client (sender): `Progress (file)`
//!
//! * server (receiver) ->   client (sender): `Done (file)`

use std::{net::IpAddr, sync::Arc};

use drop_config::DropConfig;
use serde::{Deserialize, Serialize};

pub use super::v3::{Chunk, Error, File, Progress};
use crate::{
    file::{FileId, FileKind},
    FileSubPath,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct TransferRequest {
    pub files: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Download {
    pub file: FileSubPath,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Progress(Progress),
    Done(Progress),
    Error(Error),
    Start(Download),
    Cancel(Download),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ClientMsg {
    Error(Error),
    Cancel(Download),
}

impl TryFrom<(TransferRequest, IpAddr, Arc<DropConfig>)> for crate::Transfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files }, peer, config): (TransferRequest, IpAddr, Arc<DropConfig>),
    ) -> Result<Self, Self::Error> {
        fn process_file(
            in_files: &mut Vec<crate::File>,
            subpath: FileSubPath,
            File { size, children, .. }: File,
        ) -> crate::Result<()> {
            match size {
                Some(size) => {
                    in_files.push(crate::File {
                        file_id: FileId::from(&subpath),
                        subpath,
                        kind: FileKind::FileToRecv { size },
                    });
                }
                None => {
                    for file in children {
                        process_file(
                            in_files,
                            subpath.clone().append_file_name(&file.name)?,
                            file,
                        )?;
                    }
                }
            }

            Ok(())
        }

        let mut in_files = Vec::new();
        for file in files {
            process_file(
                &mut in_files,
                FileSubPath::from_file_name(&file.name)?,
                file,
            )?;
        }

        Self::new(peer, in_files, &config)
    }
}

impl TryFrom<&crate::Transfer> for TransferRequest {
    type Error = crate::Error;

    fn try_from(value: &crate::Transfer) -> Result<Self, Self::Error> {
        let mut files: Vec<File> = Vec::new();

        for file in value.files().values() {
            let mut parents = file.subpath.iter();
            let mut files = &mut files;
            let name = if let Some(name) = parents.next_back() {
                name.clone()
            } else {
                continue;
            };

            for parent_name in parents {
                if files.iter().all(|file| file.name != *parent_name) {
                    files.push(File {
                        name: parent_name.clone(),
                        size: None,
                        children: Vec::new(),
                    });
                }

                files = &mut files
                    .iter_mut()
                    .find(|file| file.name == *parent_name)
                    .expect("The parent was just inserted")
                    .children;
            }

            files.push(File {
                name,
                size: Some(file.size()),
                children: Vec::new(),
            });
        }

        Ok(Self { files })
    }
}

impl From<&ServerMsg> for warp::ws::Message {
    fn from(value: &ServerMsg) -> Self {
        let msg = serde_json::to_string(value).expect("Failed to serialize server message");
        Self::text(msg)
    }
}

impl From<&ClientMsg> for tokio_tungstenite::tungstenite::Message {
    fn from(value: &ClientMsg) -> Self {
        let msg = serde_json::to_string(value).expect("Failed to serialize client message");
        Self::Text(msg)
    }
}

impl From<&TransferRequest> for tokio_tungstenite::tungstenite::Message {
    fn from(value: &TransferRequest) -> Self {
        let msg = serde_json::to_string(value).expect("Failed to serialize client message");
        Self::Text(msg)
    }
}
