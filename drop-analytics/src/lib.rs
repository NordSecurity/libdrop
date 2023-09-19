#[cfg(feature = "moose")]
mod moose_impl;

#[cfg(feature = "moose_file")]
mod file_impl;
mod mock_impl;

use std::sync::{Arc, Mutex, Weak};

use serde::{Deserialize, Serialize};
use slog::Logger;

static INSTANCE: Mutex<Option<Weak<dyn Moose>>> = Mutex::new(None);

#[allow(dead_code)]
const MOOSE_STATUS_SUCCESS: i32 = 0;

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

#[derive(Serialize, Deserialize)]
pub struct TransferInfo {
    pub file_count: i32,
    pub transfer_size: i32,
    pub path_ids: String,
    pub file_sizes: String,
    pub extensions: String,
    pub mime_types: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub path_id: String,
    pub direction: TransferDirection,
}

pub trait Moose: Send + Sync {
    fn event_init(&self, init_duration: i32, res: Result<(), i32>);
    fn event_transfer(&self, transfer_id: String, transfer_info: TransferInfo);
    fn event_transfer_start(&self, protocol_version: i32, transfer_id: String, retry_count: i32);
    fn event_transfer_end(&self, transfer_id: String, res: Result<(), i32>);
    fn event_transfer_file(
        &self,
        phase: TransferFilePhase,
        transfer_id: String,
        transfer_time: i32,
        file_info: FileInfo,
        transferred: i32,
        res: Result<(), i32>,
    );

    /// Generic function for logging exceptions not related to specific
    /// transfers
    ///
    /// code - error code, if unavailable, use -1
    /// note - custom additional information
    /// message - error message
    /// name - name of the error
    fn developer_exception(&self, code: i32, note: String, message: String, name: String);

    /// Generic function for logging exceptions not related to specific
    /// transfers, with an arbitrary value
    ///
    /// arbitrary_value - arbitrary value, if unavailable, use -1
    /// code - error code, if unavailable, use -1
    /// note - custom additional information
    /// message - error message
    /// name - name of the error
    fn developer_exception_with_value(
        &self,
        arbitrary_value: i32,
        code: i32,
        note: String,
        message: String,
        name: String,
    );
}

#[allow(unused_variables)]
fn create(
    logger: Logger,
    event_path: String,
    app_version: String,
    prod: bool,
) -> anyhow::Result<Arc<dyn Moose>> {
    #[cfg(feature = "moose")]
    {
        let moose = moose_impl::MooseImpl::new(logger, event_path, app_version, prod)?;
        Ok(Arc::new(moose))
    }
    #[cfg(feature = "moose_file")]
    {
        Ok(Arc::new(file_impl::FileImpl::new(
            logger,
            event_path,
            app_version,
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
    app_version: String,
    prod: bool,
) -> anyhow::Result<Arc<dyn Moose>> {
    let mut lock = INSTANCE.lock().expect("Moose lock is poisoned");

    if let Some(arc) = lock.as_ref().and_then(Weak::upgrade) {
        Ok(arc)
    } else {
        let arc = create(logger, event_path, app_version, prod)?;

        *lock = Some(Arc::downgrade(&arc));

        Ok(arc)
    }
}

pub fn moose_mock() -> Arc<dyn Moose> {
    Arc::new(mock_impl::MockImpl)
}
