use crate::{FileInfo, Phase, TransferInfo, MOOSE_STATUS_SUCCESS, MOOSE_VALUE_NONE};

use serde::{Deserialize, Serialize};

use slog::Logger;
use std::{fs::File, io::Write, path::Path};

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
    phase: Phase,
    result: i32,
    app_version: String,
    prod: bool,
}

#[derive(Serialize, Deserialize)]
struct BatchEvent {
    phase: Phase,
    transfer_id: String,
    info: TransferInfo,
}

#[derive(Serialize, Deserialize)]
struct FileEvent {
    phase: Phase,
    result: i32,
    transfer_id: String,
    transfer_time: i32,
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
        let data = if !Path::new(&self.event_path).exists() {
            "[]".to_string()
        } else {
            std::fs::read_to_string(&self.event_path)?
        };

        let mut events: Vec<MooseEventType> = serde_json::from_str(&data)?;
        events.push(event);

        File::create(&self.event_path)?.write_all(serde_json::to_string(&events)?.as_bytes())?;

        Ok(())
    }
}

impl super::Moose for FileImpl {
    fn service_quality_initialization_init(&self, res: Result<(), i32>, phase: crate::Phase) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::Init(InitEvent {
            phase,
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
        phase: crate::Phase,
        transfer_id: String,
        info: TransferInfo,
    ) {
        let event = self.write_event(MooseEventType::Batch(BatchEvent {
            phase,
            transfer_id,
            info,
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
        phase: crate::Phase,
        transfer_id: String,
        transfer_time: i32,
        info: Option<FileInfo>,
    ) {
        let result = match res {
            Ok(_) => MOOSE_STATUS_SUCCESS,
            Err(e) => e,
        };

        let event = self.write_event(MooseEventType::File(FileEvent {
            phase,
            result,
            transfer_id,
            transfer_time,
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
