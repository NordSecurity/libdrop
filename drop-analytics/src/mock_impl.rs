pub struct MockImpl;

impl super::Moose for MockImpl {
    fn service_quality_initialization_init(&self, _res: Result<(), i32>, _phase: crate::Phase) {}

    fn service_quality_transfer_batch(
        &self,
        _phase: crate::Phase,
        _files_count: i32,
        _size_of_files_list: String,
        _transfer_id: String,
        _transfer_size: i32,
    ) {
    }

    fn service_quality_transfer_file(
        &self,
        _res: Result<(), i32>,
        _phase: crate::Phase,
        _transfer_id: String,
        _transfer_size: Option<i32>,
        _transfer_time: i32,
    ) {
    }
}
