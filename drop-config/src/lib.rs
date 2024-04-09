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
    // If set the checksum events will be emited for every file of this or bigger size
    pub checksum_events_size_threshold: Option<usize>,
    // If set the checksum events will be emited for every checksum_events_granularity bytes
    // Default value is 256KB.
    pub checksum_events_granularity: u64,
    pub connection_retries: u32,
}

impl Default for DropConfig {
    fn default() -> Self {
        Self {
            dir_depth_limit: 5,
            transfer_file_limit: 1000,
            storage_path: "libdrop.sqlite".to_string(),
            checksum_events_size_threshold: None,
            checksum_events_granularity: 256 * 1024,
            connection_retries: 5,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MooseConfig {
    pub app_version: String,
    pub event_path: String,
    pub prod: bool,
}

pub const PORT: u16 = 49111;
pub const TRANFER_IDLE_LIFETIME: Duration = Duration::new(60, 0);
pub const PING_INTERVAL: Duration = Duration::new(30, 0);
pub const MAX_UPLOADS_IN_FLIGHT: usize = 4;
pub const MAX_REQUESTS_PER_SEC: u32 = 50;
pub const WS_SEND_TIMEOUT: Duration = Duration::new(20, 0);
pub const FIRST_RETRY_AFTER: Duration = Duration::new(1, 0);
