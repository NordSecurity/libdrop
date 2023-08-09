use crate::{FileInfo, TransferDirection, TransferInfo};

pub struct MockImpl;

impl super::Moose for MockImpl {
    fn service_quality_initialization_init(&self, _res: Result<(), i32>) {}

    fn service_quality_transfer_batch(
        &self,
        _transfer_id: String,
        _info: TransferInfo,
        _protocol_version: i32,
    ) {
    }

    fn service_quality_transfer_file(
        &self,
        _res: Result<(), i32>,
        _transfer_id: String,
        _transfer_time: i32,
        _direction: TransferDirection,
        _info: Option<FileInfo>,
    ) {
    }

    fn developer_exception(&self, _code: i32, _note: String, _message: String, _name: String) {}

    fn developer_exception_with_value(
        &self,
        _arbitrary_value: i32,
        _code: i32,
        _note: String,
        _message: String,
        _name: String,
    ) {
    }
}
