//! # File download flow
//!
//! * client (sender)   -> server (receiver): `TransferRequest`
//!
//! * server (receiver) ->   client (sender): `Start (file)`
//! * client (sender)   -> server (receiver): `Chunk (file)`
//! * server (receiver) ->   client (sender): `Progress (file)`
//!
//! * server (receiver) ->   client (sender): `Done (file)`

use std::net::IpAddr;

use drop_config::DropConfig;
use serde::{Deserialize, Serialize};

pub use super::v3::{Chunk, Error, File, Progress};
use crate::FileId;

#[derive(Serialize, Deserialize, Clone)]
pub struct TransferRequest {
    pub files: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Download {
    pub file: FileId,
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

impl TryFrom<(TransferRequest, IpAddr, DropConfig)> for crate::Transfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files }, peer, config): (TransferRequest, IpAddr, DropConfig),
    ) -> Result<Self, Self::Error> {
        Self::new(
            peer,
            files
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, crate::Error>>()?,
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
        })
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
