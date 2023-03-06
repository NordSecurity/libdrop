use std::{collections::HashMap, net::IpAddr, path::PathBuf};

use anyhow::Context;
use drop_config::DropConfig;
use serde::{Deserialize, Serialize};

use crate::{file::FileKind, utils::Hidden};

#[derive(Serialize, Deserialize, Clone)]
pub struct File {
    pub name: String,
    pub size: Option<u64>,
    pub children: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransferRequest {
    pub files: Vec<File>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Download {
    pub file: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Progress {
    pub file: String,
    pub bytes_transfered: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Error {
    pub file: Option<String>,
    pub msg: String,
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

#[derive(Clone)]
pub struct Chunk {
    pub file: String,
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
        let file = String::from_utf8(drain.collect()).context("Invalid file id")?;

        Ok(Self { file, data: msg })
    }

    pub fn encode(self) -> Vec<u8> {
        let Self { file, data } = self;

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
        let path = PathBuf::from(name);

        let children: HashMap<_, _> = children
            .into_iter()
            .map(|mut c| {
                let key = c.name.clone();
                c.name = path.join(&c.name).to_string_lossy().to_string();

                Ok((Hidden(PathBuf::from(key).into_boxed_path()), c.try_into()?))
            })
            .collect::<Result<_, Self::Error>>()?;

        Ok(Self {
            path: Hidden(path.into_boxed_path()),
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
        let name = value.name().ok_or(crate::Error::BadPath)?;

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

impl TryFrom<(TransferRequest, IpAddr, &DropConfig)> for crate::Transfer {
    type Error = crate::Error;

    fn try_from(
        (TransferRequest { files }, peer, config): (TransferRequest, IpAddr, &DropConfig),
    ) -> Result<Self, Self::Error> {
        Self::new(
            peer,
            files
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, crate::Error>>()?,
            config,
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

impl From<Chunk> for tokio_tungstenite::tungstenite::Message {
    fn from(value: Chunk) -> Self {
        Self::Binary(value.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_serialization() {
        const FILE_CONTNET: &[u8] = b"test file content";
        const FILE_PATH: &str = "test/file.ext";

        const CHUNK_MSG: &[u8] = b"\x0D\x00\x00\x00test/file.exttest file content";

        let msg = Chunk {
            file: String::from(FILE_PATH),
            data: FILE_CONTNET.to_vec(),
        }
        .encode();

        assert_eq!(msg, CHUNK_MSG);

        let Chunk { file, data } =
            Chunk::decode(CHUNK_MSG.to_vec()).expect("Failed to decode chunk");

        assert_eq!(file, FILE_PATH);
        assert_eq!(data, FILE_CONTNET);
    }
}
