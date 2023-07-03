use crate::{FileInfo, TransferInfo};

pub struct MockImpl;

impl super::Moose for MockImpl {
    fn service_quality_initialization_init(&self, _res: Result<(), i32>, _phase: crate::Phase) {}

    fn service_quality_transfer_batch(
        &self,
        _phase: crate::Phase,
        _transfer_id: String,
        _info: TransferInfo,
    ) {
    }

    fn service_quality_transfer_file(
        &self,
        _res: Result<(), i32>,
        _phase: crate::Phase,
        _transfer_id: String,
        _transfer_time: i32,
        _info: Option<FileInfo>,
    ) {
    }

    fn developer_exception(
        &self,
        _arbitrary_value: i32,
        _code: i32,
        _note: String,
        _message: String,
        _name: String,
    ) {
    }
}
