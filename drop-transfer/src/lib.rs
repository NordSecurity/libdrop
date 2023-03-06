mod error;
pub mod event;
pub mod file;
mod manager;
mod protocol;
mod quarantine;
pub mod service;
pub mod transfer;
pub mod utils;
mod ws;

pub(crate) use crate::manager::TransferManager;
pub use crate::{error::Error, event::Event, file::File, service::Service, transfer::Transfer};

pub type Result<T> = std::result::Result<T, Error>;
