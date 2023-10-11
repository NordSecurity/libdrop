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

use serde::{Deserialize, Serialize};

pub use super::v5::{
    Cancel, Chunk, Done, Error, File, Progress, ReportChsum, ReqChsum, Start, TransferRequest,
};
use crate::FileId;

#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Progress(Progress<FileId>),
    Done(Done),
    Error(Error<FileId>),
    ReqChsum(ReqChsum),
    Start(Start),
    Cancel(Cancel),
}

#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum ClientMsg {
    ReportChsum(ReportChsum),
    Error(Error<FileId>),
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

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn test_json<T: Serialize + DeserializeOwned + Eq>(message: T, expected: &str) {
        let json_msg = serde_json::to_value(&message).expect("Failed to serialize");
        let json_exp: serde_json::Value =
            serde_json::from_str(expected).expect("Failed to convert expected json to value");
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
    }
}
