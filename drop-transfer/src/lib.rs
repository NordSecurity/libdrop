pub mod auth;
mod error;
pub mod event;
pub mod file;
mod manager;
mod protocol;
mod quarantine;
pub mod service;
mod storage_dispatch;
pub mod transfer;
pub mod utils;
mod ws;

pub(crate) use crate::manager::TransferManager;
pub use crate::{
    error::Error,
    event::Event,
    file::{File, FileId},
    service::Service,
    storage_dispatch::StorageDispatch,
    transfer::Transfer,
};

pub type Result<T> = std::result::Result<T, Error>;
