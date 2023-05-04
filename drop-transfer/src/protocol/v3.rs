//! # File download flow
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

use std::{collections::HashMap, net::IpAddr, sync::Arc};

use anyhow::Context;
use drop_config::DropConfig;
use serde::{Deserialize, Serialize};

use crate::{
    file::{FileKind, FileSubPath},
    utils::Hidden,
};

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

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ReqChsum {
    pub file: FileSubPath,
    // Up to which point calculate checksum
    pub limit: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ReportChsum {
    pub file: FileSubPath,
    pub limit: u64,
    #[serde(serialize_with = "hex::serialize")]
    #[serde(deserialize_with = "hex::deserialize")]
    pub checksum: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Progress {
    pub file: FileSubPath,
    pub bytes_transfered: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Done {
    pub file: FileSubPath,
    pub bytes_transfered: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Error {
    pub file: Option<FileSubPath>,
    pub msg: String,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Start {
    pub file: FileSubPath,
    pub offset: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Cancel {
    pub file: FileSubPath,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Progress(Progress),
    Done(Done),
    Error(Error),
    ReqChsum(ReqChsum),
    Start(Start),
    Cancel(Cancel),
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ClientMsg {
    ReportChsum(ReportChsum),
    Error(Error),
    Cancel(Cancel),
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

#[derive(Clone)]
pub struct Chunk {
    pub file: FileSubPath,
    pub data: Vec<u8>,
}

impl Chunk {
    // Message structure:
    // [u32 little endian file id length][file id][file chunk]

    pub fn decode(mut msg: Vec<u8>) -> anyhow::Result<Self> {
        const LEN_SIZE: usize = std::mem::size_of::<u32>();

        anyhow::ensure!(msg.len() > LEN_SIZE, "Binary message too short");

        let len =
            u32::from_le_bytes(msg[..LEN_SIZE].try_into().expect("Invalid u32 size")) as usize;
        let id_end = len + LEN_SIZE;

        anyhow::ensure!(msg.len() > id_end, "Invalid file id length");

        let drain = msg.drain(0..id_end).skip(LEN_SIZE);
        let file = String::from_utf8(drain.collect())
            .context("Invalid file id")?
            .into();

        Ok(Self { file, data: msg })
    }

    pub fn encode(self) -> Vec<u8> {
        let Self { file, data } = self;

        let file = file.to_string();

        let len = file.len() as u32;
        len.to_le_bytes()
            .into_iter()
            .chain(file.into_bytes())
            .chain(data)
            .collect()
    }
}

impl TryFrom<File> for crate::File {
    type Error = crate::Error;

    fn try_from(
        File {
            name,
            size,
            children,
        }: File,
    ) -> Result<Self, Self::Error> {
        let children: HashMap<_, _> = children
            .into_iter()
            .map(|c| {
                let name = Hidden(c.name.clone());
                let file = crate::File::try_from(c)?;

                Ok((name, file))
            })
            .collect::<Result<_, Self::Error>>()?;

        Ok(Self {
            name: Hidden(name),
            kind: if children.is_empty() {
                FileKind::FileToRecv {
                    size: size.ok_or(crate::Error::BadFile)?,
                }
            } else {
                FileKind::Dir { children }
            },
        })
    }
}

impl TryFrom<&crate::File> for File {
    type Error = crate::Error;

    fn try_from(value: &crate::File) -> Result<Self, Self::Error> {
        let name = value.name().to_string();

        Ok(Self {
            name,
            size: value.size(),
            children: value
                .children()
                .map(TryFrom::try_from)
                .collect::<Result<_, crate::Error>>()?,
        })
    }
}

impl TryFrom<(TransferRequest, IpAddr, Arc<DropConfig>)> for crate::Transfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files, id }, peer, config): (TransferRequest, IpAddr, Arc<DropConfig>),
    ) -> Result<Self, Self::Error> {
        Self::new_with_uuid(
            peer,
            files
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, crate::Error>>()?,
            id,
            &config,
        )
    }
}

impl TryFrom<&crate::Transfer> for TransferRequest {
    type Error = crate::Error;

    fn try_from(value: &crate::Transfer) -> Result<Self, Self::Error> {
        Ok(Self {
            files: value
                .files()
                .values()
                .map(TryFrom::try_from)
                .collect::<Result<_, crate::Error>>()?,
            id: value.id(),
        })
    }
}

impl From<&TransferRequest> for tokio_tungstenite::tungstenite::Message {
    fn from(value: &TransferRequest) -> Self {
        let msg = serde_json::to_string(value).expect("Failed to serialize client message");
        Self::Text(msg)
    }
}

impl From<Chunk> for tokio_tungstenite::tungstenite::Message {
    fn from(value: Chunk) -> Self {
        Self::Binary(value.encode())
    }
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    #[test]
    fn binary_serialization() {
        const FILE_CONTNET: &[u8] = b"test file content";
        const FILE_ID: &str = "test/file.ext";

        const CHUNK_MSG: &[u8] = b"\x0D\x00\x00\x00test/file.exttest file content";

        let msg = Chunk {
            file: FileSubPath::from(FILE_ID),
            data: FILE_CONTNET.to_vec(),
        }
        .encode();

        assert_eq!(msg, CHUNK_MSG);

        let Chunk { file, data } =
            Chunk::decode(CHUNK_MSG.to_vec()).expect("Failed to decode chunk");

        assert_eq!(file, FileSubPath::from(FILE_ID));
        assert_eq!(data, FILE_CONTNET);
    }

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
