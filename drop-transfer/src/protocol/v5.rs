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
//!
//! There is also a posibility to delete file from the transfer (reject)
//! * server (receiver) ->   client (sender): `Reject (file)`
//! This can also be send by the client
//! * client (receiver) ->   server (sender): `Reject (file)`
//! The operation cannot be undone and subsequest downloads of this file
//! will result in error

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::{
    file::{File as _, FileSubPath},
    transfer::Transfer,
    FileId, OutgoingTransfer,
};

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct File {
    pub path: FileSubPath,
    pub id: FileId,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TransferRequest {
    pub files: Vec<File>,
    pub id: uuid::Uuid,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ReqChsum {
    pub file: FileId,
    // Up to which point calculate checksum
    pub limit: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ReportChsum {
    pub file: FileId,
    pub limit: u64,
    #[serde(serialize_with = "hex::serialize")]
    #[serde(deserialize_with = "hex::deserialize")]
    pub checksum: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Progress<T = FileId> {
    pub file: T,
    pub bytes_transfered: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Done {
    pub file: FileId,
    pub bytes_transfered: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Error<T = FileId> {
    pub file: Option<T>,
    pub msg: String,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Start {
    pub file: FileId,
    pub offset: u64,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Cancel {
    pub file: FileId,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Reject {
    pub file: FileId,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Progress(Progress<FileId>),
    Done(Done),
    Error(Error<FileId>),
    ReqChsum(ReqChsum),
    Start(Start),
    Cancel(Cancel),
    Reject(Reject),
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ClientMsg {
    ReportChsum(ReportChsum),
    Error(Error<FileId>),
    Cancel(Cancel),
    Reject(Reject),
}

#[derive(Clone)]
pub struct Chunk<T = FileId> {
    pub file: T,
    pub data: Vec<u8>,
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

impl<T> Chunk<T>
where
    T: From<String> + ToString,
{
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

impl<T> From<Chunk<T>> for tokio_tungstenite::tungstenite::Message
where
    T: From<String> + ToString,
{
    fn from(value: Chunk<T>) -> Self {
        Self::Binary(value.encode())
    }
}

impl From<&OutgoingTransfer> for TransferRequest {
    fn from(value: &OutgoingTransfer) -> Self {
        Self {
            files: value
                .files()
                .values()
                .map(|f| File {
                    path: f.subpath().clone(),
                    id: f.id().clone(),
                    size: f.size(),
                })
                .collect(),
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

    #[test]
    fn binary_serialization() {
        const FILE_CONTNET: &[u8] = b"test file content";
        const FILE_ID: &str = "ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzU";

        const CHUNK_MSG: &[u8] =
            b"\x2B\x00\x00\x00ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzUtest file content";

        let msg = Chunk {
            file: FileId::from(FILE_ID),
            data: FILE_CONTNET.to_vec(),
        }
        .encode();

        assert_eq!(msg, CHUNK_MSG);

        let Chunk { file, data } =
            Chunk::<FileId>::decode(CHUNK_MSG.to_vec()).expect("Failed to decode chunk");

        assert_eq!(file, FileId::from(FILE_ID));
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
                files: vec![
                    File {
                        path: "dir/a.txt".into(),
                        id: "ID1".into(),
                        size: 41,
                    },
                    File {
                        path: "dir/b.txt".into(),
                        id: "ID2".into(),
                        size: 4141,
                    },
                ],
                id: uuid::uuid!("1b0397eb-66e9-4252-b7cf-71782698ee3d"),
            },
            r#"
            {
              "files": [
                {
                  "path": "dir/a.txt",
                  "id": "ID1",
                  "size": 41
                },
                {
                  "path": "dir/b.txt",
                  "id": "ID2",
                  "size": 4141
                }
              ],
              "id": "1b0397eb-66e9-4252-b7cf-71782698ee3d"
            }"#,
        );

        test_json(
            ClientMsg::ReportChsum(ReportChsum {
                file: FileId::from("TESTID"),
                limit: 41,
                checksum: [
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                    22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
                ],
            }),
            r#"
            {
              "type": "ReportChsum",
              "file": "TESTID",
              "limit": 41,
              "checksum": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
            }
            "#,
        );

        test_json(
            ClientMsg::Error(Error {
                file: Some(FileId::from("TESTID")),
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": "TESTID",
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
                file: FileId::from("TESTID"),
            }),
            r#"
            {
              "type": "Cancel",
              "file": "TESTID"
            }
            "#,
        );

        test_json(
            ClientMsg::Reject(Reject {
                file: FileId::from("TESTID"),
            }),
            r#"
            {
              "type": "Reject",
              "file": "TESTID"
            }
            "#,
        );
    }

    #[test]
    fn server_json_messages() {
        test_json(
            ServerMsg::Progress(Progress {
                file: FileId::from("TESTID"),
                bytes_transfered: 41,
            }),
            r#"
            {
              "type": "Progress",
              "file": "TESTID",
              "bytes_transfered": 41
            }"#,
        );

        test_json(
            ServerMsg::Done(Done {
                file: FileId::from("TESTID"),
                bytes_transfered: 41,
            }),
            r#"
            {
              "type": "Done",
              "file": "TESTID",
              "bytes_transfered": 41
            }"#,
        );

        test_json(
            ServerMsg::Error(Error {
                file: Some(FileId::from("TESTID")),
                msg: "test message".to_string(),
            }),
            r#"
            {
              "type": "Error",
              "file": "TESTID",
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
                file: FileId::from("TESTID"),
                limit: 41,
            }),
            r#"
            {
              "type": "ReqChsum",
              "file": "TESTID",
              "limit": 41
            }"#,
        );

        test_json(
            ServerMsg::Start(Start {
                file: FileId::from("TESTID"),
                offset: 41,
            }),
            r#"
            {
              "type": "Start",
              "file": "TESTID",
              "offset": 41
            }"#,
        );

        test_json(
            ServerMsg::Cancel(Cancel {
                file: FileId::from("TESTID"),
            }),
            r#"
            {
              "type": "Cancel",
              "file": "TESTID"
            }"#,
        );

        test_json(
            ServerMsg::Reject(Reject {
                file: FileId::from("TESTID"),
            }),
            r#"
            {
              "type": "Reject",
              "file": "TESTID"
            }
            "#,
        );
    }
}
