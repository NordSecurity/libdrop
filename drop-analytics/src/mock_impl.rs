#![allow(unused_variables)]

use crate::{FileInfo, TransferInfo};

pub struct MockImpl;

impl super::Moose for MockImpl {
    fn event_init(&self, init_duration: i32, res: Result<(), i32>) {}

    fn event_transfer(&self, transfer_id: String, transfer_info: TransferInfo) {}

    fn event_transfer_start(&self, protocol_version: i32, transfer_id: String, retry_count: i32) {}

    fn event_transfer_file(
        &self,
        phase: crate::TransferFilePhase,
        transfer_id: String,
        transfer_time: i32,
        file_info: FileInfo,
        transferred: i32,
        res: Result<(), i32>,
    ) {
    }

    fn event_transfer_end(&self, transfer_id: String, res: Result<(), i32>) {}

    fn developer_exception(&self, code: i32, note: String, message: String, name: String) {}

    fn developer_exception_with_value(
        &self,
        arbitrary_value: i32,
        code: i32,
        note: String,
        message: String,
        name: String,
    ) {
    }
}
