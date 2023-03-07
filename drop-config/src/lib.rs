use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub drop: DropConfig,
    pub moose: MooseConfig,
}

#[derive(Debug, Clone, Copy)]
pub struct DropConfig {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    pub req_connection_timeout: Duration,
    pub connection_max_retry_interval: Duration,
    pub transfer_idle_lifetime: Duration,
}

impl Default for DropConfig {
    fn default() -> Self {
        Self {
            dir_depth_limit: 5,
            transfer_file_limit: 1000,
            req_connection_timeout: Duration::from_secs(5),
            connection_max_retry_interval: Duration::from_secs(10),
            transfer_idle_lifetime: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MooseConfig {
    pub event_path: String,
    pub prod: bool,
}

pub const PORT: u16 = 49111;
