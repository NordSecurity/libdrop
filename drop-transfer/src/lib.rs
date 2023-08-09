pub mod auth;
mod error;
pub mod event;
pub mod file;
mod manager;
mod protocol;
mod quarantine;
pub mod service;
mod storage_dispatch;
mod tasks;
pub mod transfer;
pub mod utils;
mod ws;

#[cfg(unix)]
pub use crate::file::FdResolver;
pub(crate) use crate::manager::TransferManager;
pub use crate::{
    error::Error,
    event::Event,
    file::{File, FileId, FileToRecv, FileToSend},
    service::Service,
    storage_dispatch::StorageDispatch,
    transfer::{IncomingTransfer, OutgoingTransfer, Transfer, TransferData},
};

pub type Result<T> = std::result::Result<T, Error>;
