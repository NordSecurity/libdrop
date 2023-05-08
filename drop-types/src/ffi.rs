use std::time::Duration;

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub dir_depth_limit: usize,
    pub transfer_file_limit: usize,
    pub req_connection_timeout_ms: u64,
    #[serde(default = "default_connection_max_retry_interval_ms")]
    pub connection_max_retry_interval_ms: u64,
    pub transfer_idle_lifetime_ms: u64,
    pub moose_event_path: String,
    pub moose_prod: bool,
    pub storage_path: String,
}

const fn default_connection_max_retry_interval_ms() -> u64 {
    10000
}

impl From<Config> for crate::config::Config {
    fn from(val: Config) -> Self {
        let Config {
            dir_depth_limit,
            transfer_file_limit,
            req_connection_timeout_ms,
            connection_max_retry_interval_ms,
            transfer_idle_lifetime_ms,
            moose_event_path,
            moose_prod,
            storage_path,
        } = val;

        crate::config::Config {
            drop: crate::config::DropConfig {
                dir_depth_limit,
                transfer_file_limit,
                req_connection_timeout: Duration::from_millis(req_connection_timeout_ms),
                connection_max_retry_interval: Duration::from_millis(
                    connection_max_retry_interval_ms,
                ),
                transfer_idle_lifetime: Duration::from_millis(transfer_idle_lifetime_ms),
                storage_path,
            },
            moose: crate::config::MooseConfig {
                event_path: moose_event_path,
                prod: moose_prod,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_config() {
        // Without `connection_max_retry_interval_ms`
        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "req_connection_timeout_ms": 1000,
          "transfer_idle_lifetime_ms": 2000,
          "moose_event_path": "test/path",
          "moose_prod": true,
          "storage_path": ":memory:"
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");
        assert_eq!(cfg.connection_max_retry_interval_ms, 10000);

        let json = r#"
        {
          "dir_depth_limit": 10,
          "transfer_file_limit": 100,
          "req_connection_timeout_ms": 1000,
          "transfer_idle_lifetime_ms": 2000,
          "connection_max_retry_interval_ms": 500,
          "moose_event_path": "test/path",
          "moose_prod": true,
          "storage_path": ":memory:"
        }
        "#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to deserialize config");

        let crate::config::Config {
            drop:
                crate::config::DropConfig {
                    dir_depth_limit,
                    transfer_file_limit,
                    req_connection_timeout,
                    transfer_idle_lifetime,
                    connection_max_retry_interval,
                    storage_path,
                },
            moose: crate::config::MooseConfig { event_path, prod },
        } = cfg.into();

        assert_eq!(dir_depth_limit, 10);
        assert_eq!(transfer_file_limit, 100);
        assert_eq!(req_connection_timeout, Duration::from_millis(1000));
        assert_eq!(connection_max_retry_interval, Duration::from_millis(500));
        assert_eq!(transfer_idle_lifetime, Duration::from_millis(2000));
        assert_eq!(event_path, "test/path");
        assert_eq!(storage_path, ":memory:");
        assert!(prod);
    }
}
