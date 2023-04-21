use std::{
    ffi::{c_int, c_void},
    panic,
};

use libc::c_char;
use serde::Serialize;
use slog::Level;

use crate::device::Result as DevResult;

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum norddrop_result {
    /// Operation was success
    NORDDROP_RES_OK = 0,

    /// Operation resulted to unknown error.
    NORDDROP_RES_ERROR = 1,

    /// Failed to parse C string, meaning the string provided is not valid UTF8
    /// or is a null pointer
    NORDDROP_RES_INVALID_STRING = 2,

    /// One of the arguments provided is invalid
    NORDDROP_RES_BAD_INPUT = 3,

    /// Failed to parse JSON argument
    NORDDROP_RES_JSON_PARSE = 4,

    /// Failed to create transfer based on arguments provided
    NORDDROP_RES_TRANSFER_CREATE = 5,

    /// The libdrop instance is not started yet
    NORDDROP_RES_NOT_STARTED = 6,

    /// Address already in use
    NORDDROP_RES_ADDR_IN_USE = 7,

    /// Failed to start the libdrop instance
    NORDDROP_RES_INSTANCE_START = 8,

    /// Failed to stop the libdrop instance
    NORDDROP_RES_INSTANCE_STOP = 9,

    /// Invalid private key provided
    NORDDROP_RES_INVALID_PRIVKEY = 10,
}

pub use norddrop_result::*;

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[repr(C)]
#[derive(Copy, Clone)]
/// Posible log levels.
pub enum norddrop_log_level {
    NORDDROP_LOG_CRITICAL = 1,
    NORDDROP_LOG_ERROR = 2,
    NORDDROP_LOG_WARNING = 3,
    NORDDROP_LOG_INFO = 4,
    NORDDROP_LOG_DEBUG = 5,
    NORDDROP_LOG_TRACE = 6,
}

#[allow(non_camel_case_types)]
pub type norddrop_event_fn = unsafe extern "C" fn(*mut c_void, *const c_char);

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Copy, Clone)]
/// Event callback
pub struct norddrop_event_cb {
    /// Context to pass to callback.
    /// User must ensure safe access of this var from multitheaded context.
    ctx: *mut c_void,
    /// Function to be called
    cb: norddrop_event_fn,
}

impl norddrop_event_cb {
    pub(crate) fn callback(&self) -> norddrop_event_fn {
        self.cb
    }

    pub(crate) fn callback_data(&self) -> *mut c_void {
        self.ctx
    }
}

#[allow(non_camel_case_types)]
pub type norddrop_logger_fn = unsafe extern "C" fn(*mut c_void, norddrop_log_level, *const c_char);

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Copy, Clone)]
/// Logging callback
pub struct norddrop_logger_cb {
    /// Context to pass to callback.
    /// User must ensure safe access of this var from multitheaded context.
    pub ctx: *mut c_void,
    /// Function to be called
    pub cb: norddrop_logger_fn,
}

#[allow(non_camel_case_types)]
/// Writes the peer's public key into the buffer of length 32.
/// The peer is identifed by IP address passed as string,
/// If IP is null that means we're requesting the public key
/// of the caller itself.
/// Returns 0 on success and 1 on failure or missing key
pub type norddrop_pubkey_fn =
    unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_char) -> c_int;

unsafe impl Send for norddrop_pubkey_cb {}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Copy, Clone)]
/// Fetch peer public key callback
pub struct norddrop_pubkey_cb {
    /// Context to pass to callback.
    /// User must ensure safe access of this var from multitheaded context.
    pub ctx: *mut c_void,
    /// Function to be called
    pub cb: norddrop_pubkey_fn,
}

#[no_mangle]
pub extern "C" fn __norddrop_force_export(
    _: norddrop_result,
    _: norddrop_event_cb,
    _: norddrop_logger_cb,
    _: norddrop_pubkey_cb,
) {
}

impl From<std::str::Utf8Error> for norddrop_result {
    fn from(_: std::str::Utf8Error) -> Self {
        NORDDROP_RES_INVALID_STRING
    }
}

impl From<DevResult> for norddrop_result {
    fn from(res: DevResult) -> Self {
        use norddrop_result::*;
        match res {
            Ok(_) => NORDDROP_RES_OK,
            Err(error) => error,
        }
    }
}

macro_rules! map_enum {
    ($from:tt <=> $to:tt, $($f:tt = $t:tt),+ $(,)?) => {
        impl From<$from> for $to {
            fn from(f: $from) -> $to {
                match f {
                    $($from::$f => $to::$t),+
                }
            }
        }

        impl From<$to> for $from {
            fn from(t: $to) -> $from {
                match t {
                    $($to::$t => $from::$f),+
                }
            }
        }
    };
}

map_enum! {
    Level <=> norddrop_log_level,
    Critical = NORDDROP_LOG_CRITICAL,
    Error = NORDDROP_LOG_ERROR,
    Warning = NORDDROP_LOG_WARNING,
    Info = NORDDROP_LOG_INFO,
    Debug = NORDDROP_LOG_DEBUG,
    Trace = NORDDROP_LOG_TRACE,
}

unsafe impl Sync for norddrop_event_cb {}
unsafe impl Send for norddrop_event_cb {}

unsafe impl Sync for norddrop_logger_cb {}
unsafe impl Send for norddrop_logger_cb {}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum PanicError {
    Panic(String),
}

impl From<&panic::PanicInfo<'_>> for PanicError {
    fn from(p: &panic::PanicInfo) -> PanicError {
        PanicError::Panic(p.to_string())
    }
}
