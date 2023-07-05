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
    pub connection_max_retry_interval: Duration,
    pub transfer_idle_lifetime: Duration,
    pub storage_path: String,
    pub max_uploads_in_flight: usize,
    pub max_reqs_per_sec: u32,
}

impl Default for DropConfig {
    fn default() -> Self {
        Self {
            dir_depth_limit: 5,
            transfer_file_limit: 1000,
            connection_max_retry_interval: Duration::from_secs(10),
            transfer_idle_lifetime: Duration::from_secs(60),
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

impl DropConfig {
    pub fn ping_interval(&self) -> Duration {
        self.transfer_idle_lifetime / 2
    }
}
