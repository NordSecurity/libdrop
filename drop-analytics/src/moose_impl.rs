use std::{
    sync::mpsc::{sync_channel, SyncSender},
    time::Duration,
};

use anyhow::Context;
use mooselibdropapp as moose;
use slog::{error, info, warn, Logger};

use crate::{TransferDirection, TransferFilePhase, MOOSE_VALUE_NONE};

const DROP_MOOSE_APP_NAME: &str = "norddrop";

pub struct MooseImpl {
    logger: slog::Logger,
}

struct MooseInitCallback {
    logger: slog::Logger,
    init_tx: SyncSender<Result<moose::TrackerState, moose::InitError>>,
}

impl moose::InitCallback for MooseInitCallback {
    fn after_init(&self, result_code: Result<moose::TrackerState, moose::InitError>) {
        info!(self.logger, "[Moose] Init callback: {:?}", result_code);
        let _ = self.init_tx.send(result_code);
    }
}

struct MooseErrorCallback {
    logger: slog::Logger,
}

impl moose::ErrorCallback for MooseErrorCallback {
    fn on_error(
        &self,
        moose_error_code: moose::MooseError,
        error_level: moose::MooseErrorLevel,
        error_code: i32,
        msg: &str,
    ) {
        match error_level {
            mooselibdropapp::MooseErrorLevel::Warning => {
                warn!(
                    self.logger,
                    "[Moose] callback: moose_error_code({:?}) error_code({:?}): {:?}",
                    moose_error_code,
                    error_code,
                    msg
                );
            }
            mooselibdropapp::MooseErrorLevel::Error => {
                error!(
                    self.logger,
                    "[Moose] callback: moose_error_code({:?}) error_code({:?}): {:?}",
                    moose_error_code,
                    error_code,
                    msg
                );
            }
        }
    }
}

macro_rules! moose_debug {
    (
        $logger:expr,
        $result:ident,
        $func:expr
    ) => {
        if let Some(error) = $result.as_ref().err() {
            warn!($logger, "[Moose] Error: {:?} on call to `{}`", error, $func);
        }
    };
}

macro_rules! moose {
    (
        $logger:expr,
        $func:ident
        $(,$arg:expr)*
    ) => {
        {
            let result = moose::$func($($arg),*);
            moose_debug!($logger, result, stringify!($func));
        }
    };
}

impl MooseImpl {
    pub fn new(
        logger: Logger,
        event_path: String,
        lib_version: String,
        prod: bool,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = sync_channel(1);

        let res = moose::init(
            event_path,
            prod,
            Box::new(MooseInitCallback {
                logger: logger.clone(),
                init_tx: tx,
            }),
            Box::new(MooseErrorCallback {
                logger: logger.clone(),
            }),
        );

        moose_debug!(logger, res, "init");
        res.context("Failed to initialize moose")?;

        let res = rx
            .recv_timeout(Duration::from_secs(2))
            .context("Failed to receive moose init callback result, channel timed out")?;
        moose_debug!(logger, res, "init");
        anyhow::ensure!(res.is_ok(), "Failed to initialize moose: {:?}", res.err());

        moose!(
            logger,
            set_context_application_libdropapp_name,
            DROP_MOOSE_APP_NAME.to_owned()
        );
        moose!(
            logger,
            set_context_application_libdropapp_version,
            lib_version
        );

        Ok(Self { logger })
    }
}

impl Drop for MooseImpl {
    fn drop(&mut self) {
        moose!(self.logger, moose_deinit);
    }
}

impl super::Moose for MooseImpl {
    fn event_init(&self, data: crate::InitEventData) {
        moose!(
            self.logger,
            send_serviceQuality_initialization_init,
            data.result,
            data.init_duration,
            None
        );
    }

    fn event_transfer_intent(&self, data: crate::TransferIntentEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_intent,
            data.transfer_id,
            data.file_count,
            data.transfer_size,
            data.file_sizes,
            data.extensions,
            data.mime_types,
            data.path_ids,
            None
        );
    }

    fn event_transfer_intent_received(&self, data: crate::TransferIntentReceivedEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_intentReceived,
            data.transfer_id,
            None
        );
    }

    fn event_transfer_state(&self, data: crate::TransferStateEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_state,
            data.transfer_id,
            data.result,
            data.protocol_version,
            None
        );
    }

    fn event_transfer_file(&self, data: crate::TransferFileEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_file,
            data.transfer_id,
            data.transfer_time,
            data.result,
            data.direction.into(),
            data.path_id,
            data.phase.into(),
            data.transferred,
            None
        );
    }

    fn developer_exception(&self, data: crate::DeveloperExceptionEventData) {
        moose!(
            self.logger,
            send_developer_exceptionHandling_catchException,
            MOOSE_VALUE_NONE,
            data.code,
            data.name,
            data.message,
            data.note,
            None
        );
    }

    fn developer_exception_with_value(&self, data: crate::DeveloperExceptionWithValueEventData) {
        moose!(
            self.logger,
            send_developer_exceptionHandling_catchException,
            data.arbitrary_value,
            data.code,
            data.name,
            data.message,
            data.note,
            None
        );
    }
}

impl From<TransferDirection> for moose::LibdropappTransferDirection {
    fn from(direction: TransferDirection) -> Self {
        match direction {
            TransferDirection::Upload => moose::LibdropappTransferDirection::Upload,
            TransferDirection::Download => moose::LibdropappTransferDirection::Download,
        }
    }
}

impl From<TransferFilePhase> for moose::LibdropappFilePhase {
    fn from(phase: TransferFilePhase) -> Self {
        match phase {
            TransferFilePhase::Finished => moose::LibdropappFilePhase::Finished,
            TransferFilePhase::Paused => moose::LibdropappFilePhase::Paused,
        }
    }
}
