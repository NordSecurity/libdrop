#[cfg(feature = "moose")]
mod moose_impl;

#[cfg(feature = "moose_file")]
mod file_impl;
mod mock_impl;

use std::sync::{Arc, Mutex, Weak};

use serde::{Deserialize, Serialize};
use slog::Logger;

static INSTANCE: Mutex<Option<Weak<dyn Moose>>> = Mutex::new(None);

pub const MOOSE_STATUS_SUCCESS: i32 = 0;

#[allow(dead_code)]
const MOOSE_VALUE_NONE: i32 = -1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TransferDirection {
    #[serde(rename = "upload")]
    Upload,
    #[serde(rename = "download")]
    Download,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TransferFilePhase {
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "finished")]
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InitEventData {
    pub init_duration: i32,
    pub result: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferIntentEventData {
    pub transfer_id: String,
    pub file_count: i32,
    pub transfer_size: i32,
    pub path_ids: String,
    pub file_sizes: String,
    pub extensions: String,
    pub mime_types: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferStateEventData {
    pub protocol_version: i32,
    pub transfer_id: String,
    pub result: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferFileEventData {
    pub phase: TransferFilePhase,
    pub transfer_id: String,
    pub transfer_time: i32,
    pub path_id: String,
    pub direction: TransferDirection,
    pub transferred: i32,
    pub result: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeveloperExceptionEventData {
    pub code: i32,
    pub note: String,
    pub message: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeveloperExceptionWithValueEventData {
    pub arbitrary_value: i32,
    pub code: i32,
    pub note: String,
    pub message: String,
    pub name: String,
}

pub trait Moose: Send + Sync {
    fn event_init(&self, data: InitEventData);
    fn event_transfer_intent(&self, data: TransferIntentEventData);
    fn event_transfer_state(&self, data: TransferStateEventData);
    fn event_transfer_file(&self, data: TransferFileEventData);

    /// Generic function for logging exceptions not related to specific
    /// transfers
    ///
    /// code - error code, if unavailable, use -1
    /// note - custom additional information
    /// message - error message
    /// name - name of the error
    fn developer_exception(&self, data: DeveloperExceptionEventData);

    /// Generic function for logging exceptions not related to specific
    /// transfers, with an arbitrary value
    ///
    /// arbitrary_value - arbitrary value, if unavailable, use -1
    /// code - error code, if unavailable, use -1
    /// note - custom additional information
    /// message - error message
    /// name - name of the error
    fn developer_exception_with_value(&self, data: DeveloperExceptionWithValueEventData);
}

#[allow(unused_variables)]
fn create(
    logger: Logger,
    event_path: String,
    lib_version: String,
    prod: bool,
) -> anyhow::Result<Arc<dyn Moose>> {
    #[cfg(feature = "moose")]
    {
        let moose = moose_impl::MooseImpl::new(logger, event_path, lib_version, prod)?;
        Ok(Arc::new(moose))
    }
    #[cfg(feature = "moose_file")]
    {
        Ok(Arc::new(file_impl::FileImpl::new(
            logger,
            event_path,
            lib_version,
            prod,
        )))
    }

    #[cfg(not(any(feature = "moose", feature = "moose_file")))]
    {
        Ok(moose_mock())
    }
}

pub fn init_moose(
    logger: Logger,
    event_path: String,
    lib_version: String,
    prod: bool,
) -> anyhow::Result<Arc<dyn Moose>> {
    let mut lock = INSTANCE.lock().expect("Moose lock is poisoned");

    if let Some(arc) = lock.as_ref().and_then(Weak::upgrade) {
        Ok(arc)
    } else {
        let arc = create(logger, event_path, lib_version, prod)?;

        *lock = Some(Arc::downgrade(&arc));

        Ok(arc)
    }
}

pub fn moose_mock() -> Arc<dyn Moose> {
    Arc::new(mock_impl::MockImpl)
}
