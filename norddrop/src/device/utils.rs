use std::time::SystemTime;

/// Get the current system timestamp in milliseconds, used for event timestamps
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
