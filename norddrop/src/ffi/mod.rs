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
/// Each transfer and files in it contain history of states that can be used to
/// replay what happened and the last state denotes the current state of the
/// transfer.
///
/// # Transfer States(same for both incoming and outgoing transfers)
/// - `canceled` - The transfer was successfully canceled by either peer.
///   Contains indicator of who canceled the transfer.
/// - `failed` - Contains status code of failure. Regular error table should be
///   consulted for errors.

/// # Incoming File States
/// - `completed` - The file was successfully received and saved to the disk.
///   Contains the final path of the file.
/// - `failed` - contains status code of failure. Regular error table should be
///   consulted for errors.
/// - `paused` - The file was paused due to recoverable errors. Most probably
///   due to network availability.
/// - `pending` - The download was issued for this file and it will proceed when
///   possible.
/// - `reject` - The file was rejected by the receiver. Contains indicator of
///   who rejected the file.
/// - `started` - The file was started to be received. Contains the base
///   directory of the file.
///
/// Terminal states: `failed`, `completed`, `reject`. Terminal states appear
/// once and it is the final state. Other states might appear multiple times.
///
/// # Outgoing File States
/// - `completed` - The file was successfully received and saved to the disk.
///   Contains the final path of the file.
/// - `failed` - contains status code of failure. Regular error table should be
///   consulted for errors.
/// - `paused` - The file was paused due to recoverable errors. Most probably
///   due to network availability.
/// - `reject` - The file was rejected by the receiver. Contains indicator of
///   who rejected the file.
/// - `started` - The file was started to be received. Contains the base
///   directory of the file.
///
/// Terminal states: `failed`, `completed`, `reject`. Terminal states appear
/// once and it is the final state. Other states might appear multiple times.
///
/// # Same transfer from both sides
/// ## Sender
///  ```json
/// {
///     "id": "0352847a-dfd5-40de-b214-edc1d06e469e",
///     "created_at": 1698240954430,
///     "peer_id": "192.168.1.3",
///     "states": [
///         {
///             "created_at": 1698240956418,
///             "state": "cancel",
///             "by_peer": true
///         }
///     ],
///     "type": "outgoing",
///     "paths": [
///         {
///             "created_at": 1698240954430,
///             "transfer_id": "0352847a-dfd5-40de-b214-edc1d06e469e",
///             "base_path": "/tmp",
///             "relative_path": "testfile-big",
///             "file_id": "ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzU",
///             "bytes": 10485760,
///             "bytes_sent": 10485760,
///             "states": [
///                 {
///                     "created_at": 1698240955416,
///                     "state": "started",
///                     "bytes_sent": 0
///                 },
///                 {
///                     "created_at": 1698240955856,
///                     "state": "completed"
///                 }
///             ]
///         }
///     ]
/// }
/// ```
/// 
/// ## Receiver
/// ```json
///     {
///         "id": "0352847a-dfd5-40de-b214-edc1d06e469e",
///         "created_at": 1698240954437,
///         "peer_id": "192.168.1.2",
///         "states": [
///             {
///                 "created_at": 1698240956417,
///                 "state": "cancel",
///                 "by_peer": false
///             }
///         ],
///         "type": "incoming",
///         "paths": [
///             {
///                 "created_at": 1698240954437,
///                 "transfer_id": "0352847a-dfd5-40de-b214-edc1d06e469e",
///                 "relative_path": "testfile-big",
///                 "file_id": "ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzU",
///                 "bytes": 10485760,
///                 "bytes_received": 10485760,
///                 "states": [
///                     {
///                         "created_at": 1698240955415,
///                         "state": "pending",
///                         "base_dir": "/tmp/received"
///                     },
///                     {
///                         "created_at": 1698240955415,
///                         "state": "started",
///                         "bytes_received": 0
///                     },
///                     {
///                         "created_at": 1698240955855,
///                         "state": "completed",
///                         "final_path": "/tmp/received/testfile-big"
///                     }
///                 ]
///             }
///         ]
///     }
/// ```
/// 
/// The whole database schema:
/// ```sql
/// CREATE TABLE transfers (
///     id TEXT PRIMARY KEY UNIQUE NOT NULL,
///     peer TEXT NOT NULL,
///     is_outgoing INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW'))     CHECK(is_outgoing = 0 OR is_outgoing = 1)
///   , is_deleted INTEGER NOT NULL DEFAULT FALSE CHECK (is_deleted IN (FALSE,
/// TRUE)));   CREATE TABLE sqlite_sequence(name,seq);
///   CREATE TABLE transfer_cancel_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     transfer_id TEXT NOT NULL,
///     by_peer INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE
/// CASCADE ON UPDATE CASCADE     CHECK(by_peer = 0 OR by_peer = 1)
///   );
///   CREATE TABLE transfer_failed_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     transfer_id TEXT NOT NULL,
///     status_code INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE
/// CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE outgoing_paths (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     transfer_id TEXT NOT NULL,
///     relative_path TEXT NOT NULL,
///     uri TEXT NOT NULL,
///     path_hash TEXT NOT NULL,
///     bytes INT NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')), is_deleted INTEGER NOT NULL DEFAULT FALSE CHECK (is_deleted IN
/// (FALSE, TRUE)),     FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON
/// DELETE CASCADE ON UPDATE CASCADE     CHECK(bytes >= 0)
///     UNIQUE(transfer_id, path_hash)
///   );
///   CREATE TABLE incoming_paths (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     transfer_id TEXT NOT NULL,   
///     relative_path TEXT NOT NULL,
///     path_hash TEXT NOT NULL,
///     bytes INT NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     checksum BLOB DEFAULT NULL, is_deleted INTEGER NOT NULL DEFAULT
/// FALSE CHECK (is_deleted IN (FALSE, TRUE)),     FOREIGN KEY(transfer_id)
/// REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE     CHECK(bytes
/// >= 0)     UNIQUE(transfer_id, path_hash)
///   );
///   CREATE TABLE outgoing_path_started_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     bytes_sent INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE,     CHECK(bytes_sent >= 0)
///   );
///   CREATE TABLE outgoing_path_failed_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     status_code INTEGER NOT NULL,
///     bytes_sent INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE,     CHECK(bytes_sent >= 0)
///   );
///   CREATE TABLE outgoing_path_completed_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE incoming_path_pending_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')), base_dir TEXT NOT NULL DEFAULT '',     FOREIGN KEY(path_id)
/// REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE incoming_path_started_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     bytes_received INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE,     CHECK(bytes_received >= 0)
///   );
///   CREATE TABLE incoming_path_failed_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     status_code INTEGER NOT NULL,
///     bytes_received INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE,     CHECK(bytes_received >= 0)
///   );
///   CREATE TABLE incoming_path_completed_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     final_path TEXT NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE incoming_path_reject_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     by_peer INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')), bytes_received INTEGER NOT NULL DEFAULT 0,     FOREIGN KEY(path_id)
/// REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE outgoing_path_reject_states (
///     id INTEGER PRIMARY KEY AUTOINCREMENT,
///     path_id INTEGER NOT NULL,
///     by_peer INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')), bytes_sent     INTEGER NOT NULL DEFAULT 0,     FOREIGN KEY(path_id)
/// REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE sync_transfer (
///     sync_id INTEGER PRIMARY KEY AUTOINCREMENT, -- use separate primary key
/// for cascade to work across sync_ tables     transfer_id TEXT NOT NULL,
///     local_state INTEGER NOT NULL,
///     FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON
/// UPDATE CASCADE   );
///   CREATE TABLE sync_outgoing_files (
///     sync_id INTEGER NOT NULL,
///     path_id INTEGER NOT NULL,
///     local_state INTEGER NOT NULL,
///     PRIMARY KEY(sync_id, path_id)
///     FOREIGN KEY(sync_id) REFERENCES sync_transfer(sync_id) ON DELETE CASCADE
/// ON UPDATE CASCADE     FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON
/// DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE sync_incoming_files (
///     sync_id INTEGER NOT NULL,
///     path_id INTEGER NOT NULL,
///     local_state INTEGER NOT NULL,
///     PRIMARY KEY(sync_id, path_id)
///     FOREIGN KEY(sync_id) REFERENCES sync_transfer(sync_id) ON DELETE CASCADE
/// ON UPDATE CASCADE     FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON
/// DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE sync_incoming_files_inflight (
///     sync_id INTEGER NOT NULL,
///     path_id INTEGER NOT NULL,
///     base_dir TEXT NOT NULL,
///     FOREIGN KEY(sync_id, path_id) REFERENCES sync_incoming_files(sync_id,
/// path_id) ON DELETE CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE incoming_path_paused_states (
///     path_id INTEGER NOT NULL,
///     bytes_received INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE   );
///   CREATE TABLE outgoing_path_paused_states (
///     path_id INTEGER NOT NULL,
///     bytes_sent INTEGER NOT NULL,
///     created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f',
/// 'NOW')),     FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE
/// CASCADE ON UPDATE CASCADE   );

/// ````
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

/// Refresh connections. Should be called when anything about the network
/// changes that might affect connections. Also when peer availability has
/// changed. This will kick-start the automated retries for all transfers.
///
/// # Arguments
///
/// * `dev` - A pointer to the instance.
///
/// # Safety
/// The pointers provided should be valid
#[no_mangle]
pub unsafe extern "C" fn norddrop_network_refresh(dev: &norddrop) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        dev.network_refresh()
            .norddrop_log_result(&dev.logger, "norddrop_network_refresh")
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
