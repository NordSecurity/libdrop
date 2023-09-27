pub mod types;
mod version;

use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    fmt, panic,
    sync::Mutex,
};

use libc::c_char;
use slog::{error, o, Drain, Logger, KV};

use self::types::{
    norddrop_event_cb, norddrop_log_level, norddrop_logger_cb, norddrop_pubkey_cb, norddrop_result,
};
use crate::device::{NordDropFFI, Result as DevResult};

/// Check if res is ok, else return early by converting Error into
/// norddrop_result
macro_rules! ffi_try {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Into::<norddrop_result>::into(e),
        }
    };
}

extern "C" {
    fn fortify_source();
}

#[allow(non_camel_case_types)]
pub struct norddrop(Mutex<NordDropFFI>);

/// Initialize a new transfer with the provided peer and descriptors
///
/// # Arguments
///
/// * `dev` - A pointer to the instance.
/// * `peer` - Peer address.
/// * `descriptors` - JSON descriptors.
///
/// # Returns
///
/// A String containing the transfer ID.
///
/// # Descriptors format
///
/// Descriptors are provided as an array of JSON objects, with each object
/// containing a "path" and optionally a file descriptor "fd":
///
/// ```json
/// [
///   {
///     "path": "/path/to/file",
///   },
///   {
///     "path": "/path/to/dir",
///   }
/// ]
/// ```
///
/// # On Android, due to limitations, we must also accept a file descriptor
///
/// ```json
/// [
///   {
///    "path": "/path/to/file",
///    "fd": 1234
///   }
/// ]
/// ```
///
/// # Safety
/// The pointers provided must be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_new_transfer(
    dev: &norddrop,
    peer: *const c_char,
    descriptors: *const c_char,
) -> *mut c_char {
    let res = panic::catch_unwind(|| {
        let mut dev = dev.0.lock().expect("lock instance");

        if peer.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let peer = CStr::from_ptr(peer).to_str()?;

        if descriptors.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let descriptors = CStr::from_ptr(descriptors).to_str()?;

        let xfid = dev.new_transfer(peer, descriptors)?;

        Ok(xfid.to_string().into_bytes())
    });

    // catch_unwind catches panics as errors and everything else goes to Ok
    match res {
        Ok(Ok(xfid)) => new_unmanaged_str(&xfid),
        _ => std::ptr::null_mut(),
    }
}

/// Destroy the libdrop instance.
///
/// # Arguments
///
/// * `dev` - Pointer to the instance.
///
/// # Safety
/// This function creates a box with instance pointer and immediately drops it.
#[no_mangle]
pub unsafe extern "C" fn norddrop_destroy(dev: *mut norddrop) {
    if !dev.is_null() {
        #[allow(clippy::let_underscore_must_use)]
        let _ = Box::from_raw(dev);
    }
}

/// # Download a file from the peer
///
/// # Arguments
///
/// * `dev` - Pointer to the instance
/// * `xfid` - Transfer ID
/// * `fid` - File ID
/// * `dst` - Destination path
///
/// # Safety
/// The pointers provided must be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_download(
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
            let cstr_xfid = CStr::from_ptr(xfid);
            ffi_try!(cstr_xfid.to_str())
        };

        let str_fid = {
            if fid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_fid = CStr::from_ptr(fid);
            ffi_try!(cstr_fid.to_str())
        };

        let str_dst = {
            if dst.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_dst = CStr::from_ptr(dst);
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

/// # Cancel a transfer from either side
///
/// # Arguments
///
/// * `dev`: Pointer to the instance
/// * `xfid`: Transfer ID
///
/// # Safety
/// The pointers provided must be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_cancel_transfer(
    dev: &norddrop,
    xfid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = CStr::from_ptr(xfid);
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

/// Reject a file from either side
///
/// # Arguments
///
/// * `dev`: Pointer to the instance
/// * `xfid`: Transfer ID
/// * `fid`: File ID
///
/// # Safety
/// The pointers provided should be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_reject_file(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(|| {
        let xfid = {
            if xfid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }

            CStr::from_ptr(xfid)
                .to_str()?
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)?
        };

        let fid = {
            if fid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }

            CStr::from_ptr(fid).to_str()?.to_owned()
        };

        let dev = dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR)?;

        dev.reject_file(xfid, fid)?;

        Ok(())
    });

    match result {
        Ok(Ok(())) => norddrop_result::NORDDROP_RES_OK,
        Ok(Err(err)) => err,
        Err(_) => norddrop_result::NORDDROP_RES_ERROR,
    }
}

/// Set FD resolver callback.
/// The callback provides FDs based on URI.
/// This function should be called before `norddrop_start()`, otherwise it will
/// return an error.
///
/// # Arguments
///
/// * `dev`: Pointer to the instance
/// * `callback`: Callback structure
#[cfg(unix)]
#[no_mangle]
pub extern "C" fn norddrop_set_fd_resolver_callback(
    dev: &norddrop,
    callback: types::norddrop_fd_cb,
) -> norddrop_result {
    let result = panic::catch_unwind(|| {
        let mut dev = dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR)?;

        dev.set_fd_resolver_callback(callback)?;
        Ok(())
    });

    match result {
        Ok(Ok(())) => norddrop_result::NORDDROP_RES_OK,
        Ok(Err(err)) => err,
        Err(_) => norddrop_result::NORDDROP_RES_ERROR,
    }
}

/// Start libdrop
///
/// # Arguments
///
/// * `dev` - Pointer to the instance
/// * `listen_addr` - Address to listen on
/// * `config` - JSON configuration
///
/// # Configuration Parameters
///
/// * `dir_depth_limit` - if the tree contains more levels then the error is
/// returned.
///
/// * `transfer_file_limit` - when aggregating files from the path, if this
/// limit is reached, an error is returned.
///
/// * `moose_event_path` - moose database path.
///
/// * `storage_path` - storage path for persistence engine.
///
/// # Safety
/// The pointers provided must be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_start(
    dev: &norddrop,
    listen_addr: *const c_char,
    config: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let addr = {
            if listen_addr.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(CStr::from_ptr(listen_addr).to_str())
        };

        let config = {
            if config.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(CStr::from_ptr(config).to_str())
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

/// Stop norddrop instance
///
/// # Arguments
///
/// * `dev` - Pointer to the instance
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

/// Purge transfers from the database
///
/// # Arguments
///
/// * `dev` - Pointer to the instance
/// * `txids` - JSON array of transfer IDs
///
/// # Safety
/// The pointers provided must be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_purge_transfers(
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

            ffi_try!(CStr::from_ptr(txids).to_str())
        };

        dev.purge_transfers(txids)
            .norddrop_log_result(&dev.logger, "norddrop_purge_transfers")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Purge transfers from the database until the given timestamp
///
///  # Arguments
///
/// * `dev` - Pointer to the instance
/// * `until_timestamp` - Unix timestamp in seconds
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

/// Get transfers from the database
///
/// # Arguments
///
/// * `dev` - Pointer to the instance
/// * `since_timestamp` - Timestamp in seconds
///
/// # Returns
///
/// JSON formatted transfers
///
/// # JSON example from the sender side
///  ```json
/// {
///      "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///      "peer_id": "192.168.0.0",
///      "created_at": 1686651025988,
///      "states": [
///          {
///              "created_at": 1686651026008,
///              "state": "cancel",
///              "by_peer": true
///          }
///      ],
///      "type": "outgoing",
///      "paths": [
///          {
///              "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///              "base_path": "/home/user/Pictures",
///              "relative_path": "doggo.jpg",
///              "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
///              "bytes": 29852,
///              "created_at": 1686651025988,
///              "states": [
///                  {
///                      "created_at": 1686651025991,
///                      "state": "pending"
///                  },
///                  {
///                      "created_at": 1686651025997,
///                      "state": "started",
///                      "bytes_sent": 0
///                  },
///                  {
///                      "created_at": 1686651026002,
///                      "state": "completed"
///                  }
///              ]
///          }
///      ]
///  }
/// ```
/// 
/// # JSON example from the receiver side
/// ```json
/// {
///     "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///     "peer_id": "172.17.0.1",
///     "created_at": 1686651025988,
///     "states": [
///         {
///             "created_at": 1686651026007,
///             "state": "cancel",
///             "by_peer": false
///         }
///     ],
///     "type": "outgoing",
///     "paths": [
///         {
///             "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///             "relative_path": "doggo.jpg",
///             "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
///             "bytes": 29852,
///             "created_at": 1686651025988,
///             "states": [
///                 {
///                     "created_at": 1686651025992,
///                     "state": "pending"
///                 },
///                 {
///                     "created_at": 1686651026000,
///                     "state": "started",
///                     "base_dir": "/root",
///                     "bytes_received": 0
///                 },
///                 {
///                     "created_at": 1686651026003,
///                     "state": "completed",
///                     "final_path": "/root/doggo.jpg"
///                 }
///             ]
///         }
///     ]
/// }
/// ```
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

/// Removes a single transfer file from the database. The file must be rejected
/// beforehand, otherwise the error is returned.
///
///  # Arguments
///
/// * `dev`: Pointer to the instance
/// * `xfid`: Transfer ID
/// * `fid`: File ID
///
/// # Safety
/// The pointers provided should be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_remove_transfer_file(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(|| {
        let dev = dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR)?;

        let xfid = {
            if xfid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }

            CStr::from_ptr(xfid)
                .to_str()?
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)?
        };

        let fid = {
            if fid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }
            CStr::from_ptr(fid).to_str()?
        };

        dev.remove_transfer_file(xfid, fid)?;
        Ok(())
    });

    match result {
        Ok(Ok(())) => norddrop_result::NORDDROP_RES_OK,
        Ok(Err(err)) => err,
        Err(_) => norddrop_result::NORDDROP_RES_ERROR,
    }
}

/// Create a new instance of norddrop. This is a required step to work
/// with API further
///
/// # Arguments
///
/// * `dev` - Pointer to the pointer to the instance. The pointer will be
///   allocated by the function and should be freed by the caller using
///   norddrop_destroy()
/// * `event_cb` - Event callback
/// * `log_level` - Log level
/// * `logger_cb` - Logger callback
/// * `pubkey_cb` - Fetch peer public key callback. It is used to request
/// the app to provide the peer’s public key or the node itself. The callback
/// provides two parameters, `const char *ip` which is a string
/// representation of the peer’s IP address, and `char *pubkey` which is
/// preallocated buffer of size 32 into which the app should write the public
/// key as bytes. The app returns the status of the callback call, 0 on
/// success and a non-zero value to indicate that the key could not be
/// provided. Note that it’s not BASE64, it must be decoded if it is beforehand.
/// * `privkey` - 32bytes private key. Note that it’s not BASE64, it must
/// be decoded if it is beforehand.
///
/// # Safety
/// The pointers provided must be valid as well as callback functions
#[no_mangle]
pub unsafe extern "C" fn norddrop_new(
    dev: *mut *mut norddrop,
    event_cb: norddrop_event_cb,
    log_level: norddrop_log_level,
    logger_cb: norddrop_logger_cb,
    pubkey_cb: norddrop_pubkey_cb,
    privkey: *const c_char,
) -> norddrop_result {
    fortify_source();

    let logger = Logger::root(logger_cb.filter_level(log_level.into()).fuse(), o!());

    let result = panic::catch_unwind(move || {
        if privkey.is_null() {
            return norddrop_result::NORDDROP_RES_INVALID_PRIVKEY;
        }
        let mut privkey_bytes = [0u8; drop_auth::SECRET_KEY_LENGTH];
        privkey_bytes
            .as_mut_ptr()
            .copy_from_nonoverlapping(privkey as *const _, drop_auth::SECRET_KEY_LENGTH);

        let privkey = drop_auth::SecretKey::from(privkey_bytes);

        let drive = ffi_try!(NordDropFFI::new(event_cb, pubkey_cb, privkey, logger));
        *dev = Box::into_raw(Box::new(norddrop(Mutex::new(drive))));

        norddrop_result::NORDDROP_RES_OK
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// Set connectivity state of a peer
///
/// # Arguments
///
/// * `dev` - A pointer to the instance.
/// * `peer` - peer address
/// * `is_online` - 0 if offline, 1 if online
///
/// # Safety
/// The pointers provided should be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_set_peer_state(
    dev: &norddrop,
    peer: *const c_char,
    is_online: i64,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        let peer = {
            if peer.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(CStr::from_ptr(peer).to_str())
        };

        let is_online = match is_online {
            0 => false,
            1 => true,
            _ => return norddrop_result::NORDDROP_RES_BAD_INPUT,
        };

        dev.set_peer_state(peer, is_online)
            .norddrop_log_result(&dev.logger, "norddrop_set_peer_state")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

struct KeyValueSerializer<'a> {
    rec: &'a slog::Record<'a>,
    kv: HashMap<slog::Key, String>,
}

impl<'a> slog::Serializer for KeyValueSerializer<'a> {
    fn emit_arguments(&mut self, key: slog::Key, val: &fmt::Arguments) -> slog::Result {
        self.kv.insert(key, val.to_string());
        Ok(())
    }
}

impl<'a> KeyValueSerializer<'a> {
    fn new(rec: &'a slog::Record) -> Self {
        KeyValueSerializer {
            rec,
            kv: HashMap::new(),
        }
    }

    fn msg(self) -> String {
        let file = self.rec.file();
        let line = self.rec.line();
        let msg = self.rec.msg();
        let kv = self.kv;

        if kv.is_empty() {
            return format!("{file}:{line} {msg}");
        }

        let kv_str = kv.iter().fold(String::new(), |mut acc, (k, v)| {
            acc.push_str(&format!("{} => {}, ", k, v));
            acc
        });

        format!("{file}:{line} {msg} @ [{kv_str}]")
    }
}

impl slog::Drain for norddrop_logger_cb {
    type Ok = ();
    type Err = ();

    /// Log a record.
    /// record.kv() contains key:value pairs inside of macro calls for logging
    /// with a `a => b` syntax
    /// the key:val pair inside of arguments is of the parent logger
    fn log(&self, record: &slog::Record, _: &slog::OwnedKVList) -> Result<Self::Ok, Self::Err> {
        if !self.is_enabled(record.level()) {
            return Ok(());
        }

        let kv = record.kv();

        let mut serializer = KeyValueSerializer::new(record);

        #[allow(clippy::let_underscore_must_use)]
        let _ = kv.serialize(record, &mut serializer);

        if let Ok(cstr) = CString::new(serializer.msg()) {
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
