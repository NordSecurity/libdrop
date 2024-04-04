#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod ffi;

/// cbindgen:ignore
pub mod device;

uniffi::include_scaffolding!("norddrop");

pub use device::types::{Config, Event, File, FinishEvent, Status};
pub use ffi::{
    types::{norddrop_log_level as LogLevel, norddrop_result as Error},
    uni::*,
};
