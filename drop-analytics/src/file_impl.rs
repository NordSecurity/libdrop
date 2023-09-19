use std::{fs::File, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use slog::Logger;

use crate::{FileInfo, TransferFilePhase, TransferInfo, MOOSE_STATUS_SUCCESS, MOOSE_VALUE_NONE};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum MooseEventType {
    #[serde(rename = "init")]
    Init(InitEvent),
    #[serde(rename = "transfer")]
    Transfer(TransferEvent),
    #[serde(rename = "transfer_start")]
    TransferStart(TransferStartEvent),
    #[serde(rename = "transfer_end")]
    TransferEnd(TransferEndEvent),
    #[serde(rename = "file")]
    File(FileEvent),
    #[serde(rename = "exception")]
    Exception(ExceptionEvent),
}

#[derive(Serialize, Deserialize)]
struct InitEvent {
    init_duration: i32,
    result: i32,
    app_version: String,
    prod: bool,
}

#[derive(Serialize, Deserialize)]
struct TransferEvent {
    transfer_id: String,
    #[serde(flatten)]
    info: TransferInfo,
}

#[derive(Serialize, Deserialize)]
struct TransferStartEvent {
    protocol_version: i32,
    transfer_id: String,
    retry_count: i32,
}

#[derive(Serialize, Deserialize)]
struct TransferEndEvent {
    transfer_id: String,
    result: i32,
}

#[derive(Serialize, Deserialize)]
struct FileEvent {
    phase: TransferFilePhase,
    transfer_id: String,
    transfer_time: i32,
    #[serde(flatten)]
    info: FileInfo,
    transferred: i32,
    result: i32,
}

#[derive(Serialize, Deserialize)]
struct ExceptionEvent {
    arbitrary_value: i32,
    code: i32,
    note: String,
    message: String,
    name: String,
}

pub struct FileImpl {
    event_path: String,
    logger: Logger,
    app_version: String,
    prod: bool,
}

impl FileImpl {
    pub fn new(logger: Logger, event_path: String, app_version: String, prod: bool) -> Self {
        Self {
            event_path,
            logger,
            app_version,
            prod,
        }
    }

    fn write_event(&self, event: MooseEventType) -> Result<(), std::io::Error> {
        let mut events: Vec<MooseEventType> = {
            if !Path::new(&self.event_path).exists() {
                vec![]
            } else {
                let data = std::fs::read_to_string(&self.event_path)?;
                serde_json::from_str(&data)?
            }
        };
        events.push(event);

        File::create(&self.event_path)?.write_all(serde_json::to_string(&events)?.as_bytes())?;

        Ok(())
    }
}

impl super::Moose for FileImpl {
    fn event_init(&self, init_duration: i32, res: Result<(), i32>) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::Init(InitEvent {
            init_duration,
            result,
            app_version: self.app_version.clone(),
            prod: self.prod,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write init event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer(&self, transfer_id: String, transfer_info: TransferInfo) {
        let event = self.write_event(MooseEventType::Transfer(TransferEvent {
            transfer_id,
            info: transfer_info,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_start(&self, protocol_version: i32, transfer_id: String, retry_count: i32) {
        let event = self.write_event(MooseEventType::TransferStart(TransferStartEvent {
            protocol_version,
            transfer_id,
            retry_count,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer start event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_end(&self, transfer_id: String, res: Result<(), i32>) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::TransferEnd(TransferEndEvent {
            transfer_id,
            result,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer end event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_file(
        &self,
        phase: TransferFilePhase,
        transfer_id: String,
        transfer_time: i32,
        file_info: FileInfo,
        transferred: i32,
        res: Result<(), i32>,
    ) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::File(FileEvent {
            phase,
            transfer_id,
            transfer_time,
            info: file_info,
            transferred,
            result,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write file event: {:?}",
                event.err()
            );
        };
    }

    fn developer_exception(&self, code: i32, note: String, message: String, name: String) {
        let event = self.write_event(MooseEventType::Exception(ExceptionEvent {
            arbitrary_value: MOOSE_VALUE_NONE,
            code,
            note,
            message,
            name,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write exception event: {:?}",
                event.err()
            );
        };
    }

    fn developer_exception_with_value(
        &self,
        arbitrary_value: i32,
        code: i32,
        note: String,
        message: String,
        name: String,
    ) {
        let event = self.write_event(MooseEventType::Exception(ExceptionEvent {
            arbitrary_value,
            code,
            note,
            message,
            name,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write exception event: {:?}",
                event.err()
            );
        };
    }
}
