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

pub trait Moose: Send + Sync {
    fn service_quality_initialization_init(&self, res: Result<(), i32>, phase: Phase);
    fn service_quality_transfer_batch(
        &self,
        phase: Phase,
        files_count: i32,
        size_of_files_list: String,
        transfer_id: String,
        transfer_size: i32,
    );
    fn service_quality_transfer_file(
        &self,
        res: Result<(), i32>,
        phase: Phase,
        transfer_id: String,
        transfer_size: Option<i32>,
        transfer_time: i32,
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
