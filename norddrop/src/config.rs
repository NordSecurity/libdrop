#[derive(Debug)]
pub struct Config {
    pub dir_depth_limit: u64,
    pub transfer_file_limit: u64,
    pub moose_event_path: String,
    pub moose_prod: bool,
    pub storage_path: String,
    pub moose_app_version: String,
    pub checksum_events_size_threshold: Option<u64>,
    pub checksum_events_granularity: Option<u64>,
    pub connection_retries: Option<u32>,
}

impl Config {
    const fn default_connection_retries() -> u32 {
        5
    }

    const fn default_checksum_granularity() -> u32 {
        256 * 1024
    }
}

impl From<Config> for drop_config::Config {
    fn from(val: Config) -> Self {
        let Config {
            dir_depth_limit,
            transfer_file_limit,
            moose_event_path,
            moose_prod,
            storage_path,
            moose_app_version,
            checksum_events_size_threshold,
            checksum_events_granularity,
            connection_retries,
        } = val;

        drop_config::Config {
            drop: drop_config::DropConfig {
                dir_depth_limit: dir_depth_limit as _,
                transfer_file_limit: transfer_file_limit as _,
                storage_path,
                checksum_events_size_threshold: checksum_events_size_threshold.map(|x| x as _),
                checksum_events_granularity: checksum_events_granularity
                    .unwrap_or(Config::default_checksum_granularity() as _),
                connection_retries: connection_retries
                    .unwrap_or(Config::default_connection_retries()),
            },
            moose: drop_config::MooseConfig {
                app_version: moose_app_version,
                event_path: moose_event_path,
                prod: moose_prod,
            },
        }
    }
}
