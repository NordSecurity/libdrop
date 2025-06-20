use std::path::Path;

use serde::{Deserialize, Serialize};
use slog::Logger;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum MooseEventType {
    #[serde(rename = "init")]
    Init(InitEvent),
    #[serde(rename = "transfer_intent")]
    TransferIntent(crate::TransferIntentEventData),
    #[serde(rename = "transfer_intent_received")]
    TransferIntentReceived(crate::TransferIntentReceivedEventData),
    #[serde(rename = "transfer_state")]
    TransferState(crate::TransferStateEventData),
    #[serde(rename = "file")]
    File(crate::TransferFileEventData),
    #[serde(rename = "exception")]
    Exception(crate::DeveloperExceptionEventData),
}

#[derive(Serialize, Deserialize)]
struct InitEvent {
    #[serde(flatten)]
    event: crate::InitEventData,
    lib_version: String,
    prod: bool,
}

pub struct FileImpl {
    event_path: String,
    logger: Logger,
    lib_version: String,
    prod: bool,
}

impl FileImpl {
    pub fn new(logger: Logger, event_path: String, lib_version: String, prod: bool) -> Self {
        Self {
            event_path,
            logger,
            lib_version,
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
    fn event_init(&self, data: crate::InitEventData) {
        let event = self.write_event(MooseEventType::Init(InitEvent {
            event: data,
            lib_version: self.lib_version.clone(),
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

    fn event_transfer_intent(&self, data: crate::TransferIntentEventData) {
        let event = self.write_event(MooseEventType::TransferIntent(data));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer intent event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_intent_received(&self, data: crate::TransferIntentReceivedEventData) {
        let event = self.write_event(MooseEventType::TransferIntentReceived(data));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer intent received event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_state(&self, data: crate::TransferStateEventData) {
        let event = self.write_event(MooseEventType::TransferState(data));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write transfer state event: {:?}",
                event.err()
            );
        };
    }

    fn event_transfer_file(&self, data: crate::TransferFileEventData) {
        let event = self.write_event(MooseEventType::File(data));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write file event: {:?}",
                event.err()
            );
        };
    }

    fn developer_exception(&self, data: crate::DeveloperExceptionEventData) {
        let event = self.write_event(MooseEventType::Exception(data));

        if event.is_err() {
            slog::error!(
                self.logger,
                "[Moose] Failed to write exception event: {:?}",
                event.err()
            );
        };
    }
}
