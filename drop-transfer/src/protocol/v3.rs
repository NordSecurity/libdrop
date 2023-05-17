//! # File download flow (same as for v4)
//!
//! * client (sender)   -> server (receiver): `TransferRequest`
//!
//! If the server has the file or a part of it, the server can request checksum
//! from the client. In that case sender must report the checksum. The request
//! can be repeated
//! * server (receiver) ->   client (sender): `ReqChsum (file)`
//! * client (sender)   -> server (receiver): `ReportChsum (file)`
//!
//! If the server needs to download something:
//! * server (receiver) ->   client (sender): `Start (file)`
//! * client (sender)   -> server (receiver): `Chunk (file)`
//! * server (receiver) ->   client (sender): `Progress (file)`
//!
//! This message indicate that the file is downloaded. Can be sent without
//! `Start` in case the downloaded file is already there
//! * server (receiver) ->   client (sender): `Done (file)`

use std::{net::IpAddr, sync::Arc};

use drop_config::DropConfig;
use serde::{Deserialize, Serialize};

use super::v4;
use crate::file::{FileId, FileKind, FileSubPath};

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct File {
    pub name: String,
    pub size: Option<u64>,
    pub children: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TransferRequest {
    pub files: Vec<File>,
    pub id: uuid::Uuid,
}

pub type ReqChsum = v4::ReqChsum<FileSubPath>;
pub type ReportChsum = v4::ReportChsum<FileSubPath>;
pub type Progress = v4::Progress<FileSubPath>;
pub type Done = v4::Done<FileSubPath>;
pub type Error = v4::Error<FileSubPath>;
pub type Start = v4::Start<FileSubPath>;
pub type Cancel = v4::Cancel<FileSubPath>;
pub type ServerMsg = v4::ServerMsg<FileSubPath>;
pub type ClientMsg = v4::ClientMsg<FileSubPath>;
pub type Chunk = v4::Chunk<FileSubPath>;

impl TryFrom<(TransferRequest, IpAddr, Arc<DropConfig>)> for crate::Transfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files, id }, peer, config): (TransferRequest, IpAddr, Arc<DropConfig>),
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

        Self::new_with_uuid(peer, in_files, id, &config)
    }
}

impl From<&crate::Transfer> for TransferRequest {
    fn from(value: &crate::Transfer) -> Self {
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

        Self {
            files,
            id: value.id(),
        }
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
                id: uuid::uuid!("1b0397eb-66e9-4252-b7cf-71782698ee3d"),
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
              ],
              "id": "1b0397eb-66e9-4252-b7cf-71782698ee3d"
            }"#,
        );

        test_json(
            ClientMsg::ReportChsum(ReportChsum {
                file: FileSubPath::from("test/file.ext"),
                limit: 41,
                checksum: [
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                    22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
                ],
            }),
            r#"
            {
              "type": "ReportChsum",
              "file": "test/file.ext",
              "limit": 41,
              "checksum": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
            }
            "#,
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
            ClientMsg::Cancel(Cancel {
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
            ServerMsg::Done(Done {
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
            ServerMsg::ReqChsum(ReqChsum {
                file: FileSubPath::from("test/file.ext"),
                limit: 41,
            }),
            r#"
            {
              "type": "ReqChsum",
              "file": "test/file.ext",
              "limit": 41
            }"#,
        );

        test_json(
            ServerMsg::Start(Start {
                file: FileSubPath::from("test/file.ext"),
                offset: 41,
            }),
            r#"
            {
              "type": "Start",
              "file": "test/file.ext",
              "offset": 41
            }"#,
        );

        test_json(
            ServerMsg::Cancel(Cancel {
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
