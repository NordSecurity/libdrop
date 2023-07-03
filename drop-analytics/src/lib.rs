#[cfg(feature = "moose")]
mod moose_impl;

mod mock_impl;

use std::sync::{Arc, Mutex, Weak};

use slog::Logger;

static INSTANCE: Mutex<Option<Weak<dyn Moose>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Start,
    End,
}

pub struct TransferInfo {
    pub mime_type_list: String,
    pub extension_list: String,
    pub file_size_list: String,
    pub transfer_size_kb: i32,
    pub file_count: i32,
}

#[derive(Default)]
pub struct FileInfo {
    pub mime_type: String,
    pub extension: String,
    pub size_kb: i32,
}

pub trait Moose: Send + Sync {
    fn service_quality_initialization_init(&self, res: Result<(), i32>, phase: Phase);
    fn service_quality_transfer_batch(&self, phase: Phase, transfer_id: String, info: TransferInfo);
    fn service_quality_transfer_file(
        &self,
        res: Result<(), i32>,
        phase: Phase,
        transfer_id: String,
        transfer_time: i32,
        info: Option<FileInfo>,
    );

    /// Generic function for logging exceptions not related to specific transfers
    ///
    /// arbitrary_value - arbitrary value, if unavailable, use -1
    /// code - error code, if unavailable, use -1
    /// note - custom additional information
    /// message - error message
    /// name - name of the error
    fn developer_exception(
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

    #[cfg(not(feature = "moose"))]
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
