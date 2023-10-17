use std::sync::Arc;

use slog::{error, info};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

use crate::{service::State, ws::OutgoingFileEventTx};

pub struct PermitInit(PermitInitRepr);

enum PermitInitRepr {
    Acquired(OwnedSemaphorePermit),
    WillWait {
        logger: slog::Logger,
        throttle: Arc<Semaphore>,
        events: Arc<OutgoingFileEventTx>,
        transfered: u64,
    },
}

pub(crate) async fn init(
    logger: &slog::Logger,
    state: &State,
    events: &Arc<OutgoingFileEventTx>,
    transfered: u64,
) -> Option<PermitInit> {
    let repr = match state.throttle.clone().try_acquire_owned() {
        Err(TryAcquireError::NoPermits) => {
            let file_id = events.file_id();
            info!(logger, "Throttling file: {file_id}");
            events.throttled(transfered).await;

            PermitInitRepr::WillWait {
                logger: logger.clone(),
                throttle: state.throttle.clone(),
                events: events.clone(),
                transfered,
            }
        }
        Err(TryAcquireError::Closed) => {
            error!(logger, "Throttle semaphore is closed");
            return None;
        }
        Ok(permit) => {
            events.start(transfered).await;
            PermitInitRepr::Acquired(permit)
        }
    };

    Some(PermitInit(repr))
}

impl PermitInit {
    pub async fn acquire(self) -> Option<OwnedSemaphorePermit> {
        match self.0 {
            PermitInitRepr::Acquired(permit) => Some(permit),
            PermitInitRepr::WillWait {
                logger,
                throttle,
                events,
                transfered,
            } => match throttle.acquire_owned().await {
                Ok(permit) => {
                    let file_id = events.file_id();
                    info!(logger, "Throttle permited file: {file_id}");
                    events.start_with_progress(transfered).await;

                    Some(permit)
                }
                Err(err) => {
                    error!(logger, "Throttle semaphore failed: {err}");
                    None
                }
            },
        }
    }
}
