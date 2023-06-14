pub mod types;
mod version;

use std::{
    ffi::{CStr, CString},
    panic,
    sync::{Mutex, Once},
};

use libc::c_char;
use slog::{error, o, warn, Drain, Logger};

use self::types::{
    norddrop_event_cb, norddrop_log_level, norddrop_logger_cb, norddrop_pubkey_cb, norddrop_result,
};
use crate::{
    device::{NordDropFFI, Result as DevResult},
    ffi::types::PanicError,
};

/// Cehck if res is ok, else return early by converting Error into
/// norddrop_result
macro_rules! ffi_try {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Into::<norddrop_result>::into(e),
        }
    };
}

/// cbindgen:ignore
static PANIC_HOOK: Once = Once::new();

extern "C" {
    fn fortify_source();
}

#[allow(non_camel_case_types)]
pub struct norddrop(Mutex<NordDropFFI>);

#[no_mangle]
pub extern "C" fn norddrop_new_transfer(
    dev: &norddrop,
    peer: *const c_char,
    descriptors: *const c_char,
) -> *mut c_char {
    let res = panic::catch_unwind(|| {
        let mut dev = dev.0.lock().expect("lock instance");

        if peer.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let peer = unsafe { CStr::from_ptr(peer) }.to_str()?;

        if descriptors.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let descriptors = unsafe { CStr::from_ptr(descriptors) }.to_str()?;

        let xfid = dev.new_transfer(peer, descriptors)?;

        Ok(xfid.to_string().into_bytes())
    });

    // catch_unwind catches panics as errors and everything else goes to Ok
    match res {
        Ok(Ok(xfid)) => new_unmanaged_str(&xfid),
        _ => std::ptr::null_mut(),
    }
}

/// Destroy libdrop instance
#[no_mangle]
pub extern "C" fn norddrop_destroy(dev: *mut norddrop) {
    if !dev.is_null() {
        unsafe { Box::from_raw(dev) };
    }
}

/// Download a file from the peer
#[no_mangle]
pub extern "C" fn norddrop_download(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
    dst: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let str_fid = {
            if fid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_fid = unsafe { CStr::from_ptr(fid) };
            ffi_try!(cstr_fid.to_str())
        };

        let str_dst = {
            if dst.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_dst = unsafe { CStr::from_ptr(dst) };
            ffi_try!(cstr_dst.to_str())
        };

        dev.download(
            ffi_try!(str_xfid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            ffi_try!(str_fid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            str_dst.to_string(),
        )
        .norddrop_log_result(&dev.logger, "norddrop_download")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}
/// Cancel a transfer from the sender side
#[no_mangle]
pub extern "C" fn norddrop_cancel_transfer(dev: &norddrop, xfid: *const c_char) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.cancel_transfer(ffi_try!(str_xfid
            .to_string()
            .parse()
            .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)))
            .norddrop_log_result(
                &dev.logger,
                &format!("norddrop_cancel_transfer, xfid: {xfid:?}"),
            )
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Cancel a transfer from the sender side
#[no_mangle]
pub extern "C" fn norddrop_cancel_file(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let str_fid = {
            if fid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            let cstr_fid = unsafe { CStr::from_ptr(fid) };
            ffi_try!(cstr_fid.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.cancel_file(
            ffi_try!(str_xfid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            ffi_try!(str_fid
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
        )
        .norddrop_log_result(&dev.logger, "norddrop_cancel_file")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Start norddrop instance.
#[no_mangle]
pub extern "C" fn norddrop_start(
    dev: &norddrop,
    listen_addr: *const c_char,
    config: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let addr = {
            if listen_addr.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(listen_addr) }.to_str())
        };

        let config = {
            if config.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(config) }.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.start(addr, config)
            .norddrop_log_result(&dev.logger, "norddrop_start")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Stop norddrop instance and all related activities
#[no_mangle]
pub extern "C" fn norddrop_stop(dev: &norddrop) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        dev.stop().norddrop_log_result(&dev.logger, "norddrop_stop")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Purge transfers with the given id(s) from the database, accepts a JSON array
/// of strings
#[no_mangle]
pub extern "C" fn norddrop_purge_transfers(
    dev: &norddrop,
    txids: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        let txids = {
            if txids.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(txids) }.to_str())
        };

        dev.purge_transfers(txids)
            .norddrop_log_result(&dev.logger, "norddrop_purge_transfers")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Purge all transfers that are older than the given timestamp from the
/// database. Accepts a UNIX timestamp in seconds
#[no_mangle]
pub extern "C" fn norddrop_purge_transfers_until(
    dev: &norddrop,
    until_timestamp: std::ffi::c_longlong,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        dev.purge_transfers_until(until_timestamp)
            .norddrop_log_result(&dev.logger, "norddrop_purge_transfers_until")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Get all transfers since the given timestamp from the database. Accepts a
/// UNIX timestamp in seconds
#[no_mangle]
pub extern "C" fn norddrop_get_transfers_since(
    dev: &norddrop,
    since_timestamp: std::ffi::c_longlong,
) -> *mut c_char {
    let res = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        let transfers = dev.transfers_since(since_timestamp)?;

        Ok::<Vec<u8>, norddrop_result>(transfers.into_bytes())
    });

    match res {
        Ok(Ok(transfers)) => new_unmanaged_str(&transfers),
        _ => std::ptr::null_mut(),
    }
}

/// Create a new instance of norddrop. This is a required step to work with API
/// further
#[no_mangle]
pub extern "C" fn norddrop_new(
    dev: *mut *mut norddrop,
    event_cb: norddrop_event_cb,
    log_level: norddrop_log_level,
    logger_cb: norddrop_logger_cb,
    pubkey_cb: norddrop_pubkey_cb,
    privkey: *const c_char,
) -> norddrop_result {
    unsafe {
        fortify_source();
    }

    let logger = Logger::root(logger_cb.filter_level(log_level.into()).fuse(), o!());

    PANIC_HOOK.call_once(|| {
        let logger = logger.clone();

        panic::set_hook(Box::new(move |info: &panic::PanicInfo| {
            error!(logger, "{}", info);

            let res = CString::new(
                serde_json::to_string(&PanicError::from(info))
                    .unwrap_or_else(|_| String::from("event_to_json error")),
            );

            match res {
                Ok(s) => unsafe {
                    let callback = event_cb.callback();
                    let callback_data = event_cb.callback_data();

                    (callback)(callback_data, s.as_ptr())
                },
                Err(e) => warn!(logger, "Failed to create CString: {}", e),
            }
        }));
    });

    let result = panic::catch_unwind(move || {
        {
            let logger = logger.clone();
            use log::{Level, LevelFilter, Metadata, Record};

            struct SimpleLogger {
                logger: Logger,
            }

            impl log::Log for SimpleLogger {
                fn enabled(&self, metadata: &Metadata) -> bool {
                    metadata.level() <= Level::Info
                }

                fn log(&self, record: &Record) {
                    if self.enabled(record.metadata()) {
                        match record.level() {
                            Level::Error => slog::error!(self.logger, "{}", record.args()),
                            Level::Warn => slog::warn!(self.logger, "{}", record.args()),
                            Level::Info => slog::info!(self.logger, "{}", record.args()),
                            Level::Debug => slog::debug!(self.logger, "{}", record.args()),
                            Level::Trace => slog::trace!(self.logger, "{}", record.args()),
                        }
                    }
                }
                fn flush(&self) {}
            }

            log::set_boxed_logger(Box::new(SimpleLogger {
                logger: logger.clone(),
            }))
            .expect("Failed to set logger");

            log::set_max_level(LevelFilter::Trace);
        }

        if privkey.is_null() {
            return norddrop_result::NORDDROP_RES_INVALID_PRIVKEY;
        }
        let mut privkey_bytes = [0u8; drop_auth::SECRET_KEY_LENGTH];
        unsafe {
            privkey_bytes
                .as_mut_ptr()
                .copy_from_nonoverlapping(privkey as *const _, drop_auth::SECRET_KEY_LENGTH);
        }
        let privkey = drop_auth::SecretKey::from(privkey_bytes);

        let drive = ffi_try!(NordDropFFI::new(event_cb, pubkey_cb, privkey, logger));
        unsafe { *dev = Box::into_raw(Box::new(norddrop(Mutex::new(drive)))) };

        norddrop_result::NORDDROP_RES_OK
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

impl slog::Drain for norddrop_logger_cb {
    type Ok = ();
    type Err = ();

    fn log(&self, record: &slog::Record, _: &slog::OwnedKVList) -> Result<Self::Ok, Self::Err> {
        if !self.is_enabled(record.level()) {
            return Ok(());
        }

        let file = record.location().file;
        let line = record.location().line;

        let msg = format!("{file}:{line} {}", record.msg());
        if let Ok(cstr) = CString::new(msg) {
            unsafe { (self.cb)(self.ctx, record.level().into(), cstr.as_ptr()) };
        }

        Ok(())
    }
}

trait FFILog {
    fn norddrop_log_result(self, logger: &Logger, caller: &str) -> norddrop_result;
}

impl FFILog for DevResult {
    fn norddrop_log_result(self, logger: &Logger, caller: &str) -> norddrop_result {
        let res = norddrop_result::from(self);
        match res {
            norddrop_result::NORDDROP_RES_OK => {}
            _ => error!(logger, "{:?}: {res:?}", caller),
        };
        res
    }
}

fn new_unmanaged_str(bytes: &[u8]) -> *mut c_char {
    let ptr = unsafe { libc::calloc(bytes.len() + 1, std::mem::size_of::<c_char>()) };
    if ptr.is_null() {
        return std::ptr::null_mut();
    };

    unsafe { ptr.copy_from_nonoverlapping(bytes.as_ptr() as *const _, bytes.len()) };

    ptr as *mut c_char
}
