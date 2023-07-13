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

use super::v4;
use crate::{
    file::{File as _, FileId, FileSubPath, FileToRecv},
    transfer::IncomingTransfer,
    OutgoingTransfer, Transfer,
};

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct File {
    pub name: String,
    pub size: Option<u64>,
    pub children: Vec<File>,
}

pub type Chunk = v4::Chunk<FileSubPath>;
pub type Progress = v4::Progress<FileSubPath>;
pub type Error = v4::Error<FileSubPath>;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TransferRequest {
    pub files: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Download {
    pub file: FileSubPath,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Progress(Progress),
    Done(Progress),
    Error(Error),
    Start(Download),
    Cancel(Download),
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ClientMsg {
    Error(Error),
    Cancel(Download),
}

impl TryFrom<(TransferRequest, IpAddr, Arc<DropConfig>)> for IncomingTransfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files }, peer, config): (TransferRequest, IpAddr, Arc<DropConfig>),
    ) -> Result<Self, Self::Error> {
        fn process_file(
            in_files: &mut Vec<FileToRecv>,
            subpath: FileSubPath,
            File { size, children, .. }: File,
        ) -> crate::Result<()> {
            match size {
                Some(size) => {
                    in_files.push(FileToRecv::new(FileId::from(&subpath), subpath, size));
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

impl From<&OutgoingTransfer> for TransferRequest {
    fn from(value: &OutgoingTransfer) -> Self {
        let mut files: Vec<File> = Vec::new();

        for file in value.files().values() {
            let mut parents = file.subpath().iter();
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

        Self { files }
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

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn test_json<T: Serialize + DeserializeOwned + Eq>(message: T, expected: &str) {
        let json_msg = serde_json::to_value(&message).expect("Failed to serialize");
        let json_exp: serde_json::Value =
            serde_json::from_str(expected).expect("Failed to convert expected josn to value");
        assert_eq!(json_msg, json_exp);

        let deserialized: T = serde_json::from_str(expected).expect("Failed to serialize");
        assert!(deserialized == message);
    }

    #[test]
    fn client_json_messages() {
        test_json(
            TransferRequest {
                files: vec![File {
                    name: "dir".into(),
                    size: None,
                    children: vec![
                        File {
                            name: "a.txt".into(),
                            size: Some(41),
                            children: vec![],
                        },
                        File {
                            name: "b.txt".into(),
                            size: Some(4141),
                            children: vec![],
                        },
                    ],
                }],
            },
            r#"
            {
              "files": [
                {
                  "name": "dir",
                  "size": null,
                  "children": [
                    {
                      "name": "a.txt",
                      "size": 41,
                      "children": []
                    },
                    {
                      "name": "b.txt",
                      "size": 4141,
                      "children": []
                    }
                  ]
                }
              ]
            }"#,
        );

        test_json(
            ClientMsg::Error(Error {
                file: Some(FileSubPath::from("test/file.ext")),
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": "test/file.ext",
              "msg": "test message"
            }
            "#,
        );

        test_json(
            ClientMsg::Error(Error {
                file: None,
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": null,
              "msg": "test message"
            }
            "#,
        );

        test_json(
            ClientMsg::Cancel(Download {
                file: FileSubPath::from("test/file.ext"),
            }),
            r#"
            {
              "type": "Cancel",
              "file": "test/file.ext"
            }
            "#,
        );
    }

    #[test]
    fn server_json_messages() {
        test_json(
            ServerMsg::Progress(Progress {
                file: FileSubPath::from("test/file.ext"),
                bytes_transfered: 41,
            }),
            r#"
            {
              "type": "Progress",
              "file": "test/file.ext",
              "bytes_transfered": 41
            }"#,
        );

        test_json(
            ServerMsg::Done(Progress {
                file: FileSubPath::from("test/file.ext"),
                bytes_transfered: 41,
            }),
            r#"
            {
              "type": "Done",
              "file": "test/file.ext",
              "bytes_transfered": 41
            }"#,
        );

        test_json(
            ServerMsg::Error(Error {
                file: Some(FileSubPath::from("test/file.ext")),
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": "test/file.ext",
              "msg": "test message"
            }"#,
        );

        test_json(
            ServerMsg::Error(Error {
                file: None,
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": null,
              "msg": "test message"
            }"#,
        );

        test_json(
            ServerMsg::Start(Download {
                file: FileSubPath::from("test/file.ext"),
            }),
            r#"
            {
              "type": "Start",
              "file": "test/file.ext"
            }"#,
        );

        test_json(
            ServerMsg::Cancel(Download {
                file: FileSubPath::from("test/file.ext"),
            }),
            r#"
            {
              "type": "Cancel",
              "file": "test/file.ext"
            }"#,
        );
    }
}
