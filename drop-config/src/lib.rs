use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub drop: DropConfig,
    pub moose: MooseConfig,
}

#[derive(Debug, Clone)]
pub struct DropConfig {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    pub storage_path: String,
    pub max_uploads_in_flight: usize,
    pub max_reqs_per_sec: u32,
}

impl Default for DropConfig {
    fn default() -> Self {
        Self {
            dir_depth_limit: 5,
            transfer_file_limit: 1000,
            storage_path: "libdrop.sqlite".to_string(),
            max_uploads_in_flight: 4,
            max_reqs_per_sec: 50,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MooseConfig {
    pub event_path: String,
    pub prod: bool,
}

pub const PORT: u16 = 49111;
pub const TRANFER_IDLE_LIFETIME: Duration = Duration::new(60, 0);
pub const PING_INTERVAL: Duration = Duration::new(30, 0);
pub const CONNECTION_MAX_RETRY_INTERVAL: Duration = Duration::new(10, 0);
