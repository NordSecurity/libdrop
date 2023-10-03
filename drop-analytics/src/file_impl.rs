use std::{fs::File, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use slog::Logger;
use uuid;

use crate::{FileInfo, TransferDirection, TransferInfo, MOOSE_STATUS_SUCCESS, MOOSE_VALUE_NONE};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum MooseEventType {
    #[serde(rename = "init")]
    Init(InitEvent),
    #[serde(rename = "batch")]
    Batch(BatchEvent),
    #[serde(rename = "file")]
    File(FileEvent),
    #[serde(rename = "exception")]
    Exception(ExceptionEvent),
}

#[derive(Serialize, Deserialize)]
struct InitEvent {
    result: i32,
    app_version: String,
    prod: bool,
}

#[derive(Serialize, Deserialize)]
struct BatchEvent {
    transfer_id: String,
    info: TransferInfo,
    protocol_version: i32,
}

#[derive(Serialize, Deserialize)]
struct FileEvent {
    result: i32,
    transfer_id: String,
    transfer_time: i32,
    direction: TransferDirection,
    info: FileInfo,
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

        let payload = serde_json::to_string(&events)?;

        // create unique temp path
        let temp_path = format!("{}.tmp.{}", self.event_path, uuid::Uuid::new_v4());

        std::fs::write(&temp_path, payload.as_bytes())?;
        std::fs::rename(temp_path, &self.event_path)?;

        Ok(())
    }
}

impl super::Moose for FileImpl {
    fn service_quality_initialization_init(&self, res: Result<(), i32>) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::Init(InitEvent {
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
    fn service_quality_transfer_batch(
        &self,
        transfer_id: String,
        info: TransferInfo,
        protocol_version: i32,
    ) {
        let event = self.write_event(MooseEventType::Batch(BatchEvent {
            transfer_id,
            info,
            protocol_version,
        }));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write batch event: {:?}",
                event.err()
            );
        };
    }
    fn service_quality_transfer_file(
        &self,
        res: Result<(), i32>,
        transfer_id: String,
        transfer_time: i32,
        direction: TransferDirection,
        info: Option<FileInfo>,
    ) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::File(FileEvent {
            result,
            transfer_id,
            transfer_time,
            direction,
            info: info.unwrap_or_default(),
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
