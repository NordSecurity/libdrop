pub mod types;
pub mod utils;

use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
    time::SystemTime,
};

use drop_analytics::DeveloperExceptionEventData;
use drop_auth::{PublicKey, SecretKey, PUBLIC_KEY_LENGTH};
use drop_config::{Config, DropConfig, MooseConfig};
use drop_transfer::{auth, utils::Hidden, Event, FileToSend, OutgoingTransfer, Service, Transfer};
use slog::{debug, error, trace, warn, Logger};
use tokio::sync::{mpsc, Mutex};

use self::types::TransferDescriptor;
use crate::{device::types::FinishEvent, ffi, ffi::types as ffi_types};

pub type Result<T = ()> = std::result::Result<T, ffi::types::norddrop_result>;

const SQLITE_TIMESTAMP_MIN: i64 = -210866760000;
const SQLITE_TIMESTAMP_MAX: i64 = 253402300799;

pub(super) struct NordDropFFI {
    rt: tokio::runtime::Runtime,
    pub logger: Logger,
    instance: Arc<Mutex<Option<drop_transfer::Service>>>,
    event_dispatcher: Arc<EventDispatcher>,
    keys: Arc<auth::Context>,
    config: DropConfig,
    #[cfg(unix)]
    fdresolv: Option<Arc<drop_transfer::file::FdResolver>>,
}

struct EventDispatcher {
    cb: ffi_types::norddrop_event_cb,
    logger: Logger,
}

impl EventDispatcher {
    pub fn dispatch(&self, e: types::Event) {
        let res = std::ffi::CString::new(
            serde_json::to_string(&e).unwrap_or_else(|_| String::from("event_to_json error")),
        );

        match res {
            Ok(s) => unsafe {
                let callback = self.cb.callback();
                let callback_data = self.cb.callback_data();

                (callback)(callback_data, s.as_ptr())
            },
            Err(e) => warn!(self.logger, "Failed to create CString: {}", e),
        }
    }
}

impl NordDropFFI {
    pub(super) fn new(
        event_cb: ffi_types::norddrop_event_cb,
        pubkey_cb: ffi_types::norddrop_pubkey_cb,
        privkey: SecretKey,
        logger: Logger,
    ) -> Result<Self> {
        trace!(logger, "norddrop_new()");

        // It's a debug print. Not visible in the production build
        debug!(logger, "Private key: {:02X?}", privkey.to_bytes());

        Ok(NordDropFFI {
            instance: Arc::default(),
            logger: logger.clone(),
            rt: tokio::runtime::Runtime::new().map_err(|_| ffi::types::NORDDROP_RES_ERROR)?,
            event_dispatcher: Arc::new(EventDispatcher {
                cb: event_cb,
                logger: logger.clone(),
            }),
            config: DropConfig::default(),
            keys: Arc::new(crate_key_context(logger, privkey, pubkey_cb)),
            #[cfg(unix)]
            fdresolv: None,
        })
    }

    pub(super) fn start(&mut self, listen_addr: &str, config_json: &str) -> Result<()> {
        let init_time = std::time::Instant::now();
        trace!(
            self.logger,
            "norddrop_start() listen address: {:?}",
            listen_addr,
        );

        // Check preconditions first
        let config = parse_and_validate_config(&self.logger, config_json)?;
        let addr: IpAddr = match listen_addr.parse() {
            Ok(addr) => addr,
            Err(err) => {
                error!(self.logger, "Failed to parse IP address: {err}");
                return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
            }
        };

        let mut instance = self.instance.blocking_lock();
        if instance.is_some() {
            return Err(ffi::types::NORDDROP_RES_INSTANCE_START);
        };

        // All good, let's proceed

        let moose = initialize_moose(&self.logger, config.moose)?;

        let storage = Arc::new(open_database(
            &config.drop.storage_path,
            &self.event_dispatcher,
            &self.logger,
            &moose,
        )?);

        // Spawn a task grabbing events from the inner service and dispatch them
        // to the host app
        let ed = self.event_dispatcher.clone();
        let event_logger = self.logger.clone();
        let event_storage = storage.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<(Event, SystemTime)>();

        self.rt.spawn(async move {
            let mut dispatch = drop_transfer::StorageDispatch::new(&event_storage);

            while let Some(e) = rx.recv().await {
                debug!(event_logger, "emitting event: {:#?}", e);

                dispatch.handle_event(&e.0).await;
                // Android team reported problems with the event ordering.
                // The events where dispatched in different order than where emitted.
                // To fix that we need to process the events sequentially.
                // Also the callback may block the executor - we need to be resistant to that.
                tokio::task::block_in_place(|| ed.dispatch(e.into()));
            }
        });

        match self.rt.block_on(Service::start(
            addr,
            storage,
            tx,
            self.logger.clone(),
            Arc::new(config.drop.clone()),
            moose,
            self.keys.clone(),
            init_time,
            #[cfg(unix)]
            self.fdresolv.clone(),
        )) {
            Ok(srv) => instance.replace(srv),
            Err(err) => {
                error!(self.logger, "Failed to start the service: {}", err);

                let err = match err {
                    drop_transfer::Error::AddrInUse => ffi::types::NORDDROP_RES_ADDR_IN_USE,
                    _ => ffi::types::NORDDROP_RES_INSTANCE_START,
                };

                return Err(err);
            }
        };

        self.config = config.drop;

        Ok(())
    }

    pub(super) fn stop(&mut self) -> Result<()> {
        trace!(self.logger, "norddrop_stop()");

        let instance = self
            .instance
            .blocking_lock()
            .take()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?;

        self.rt.block_on(instance.stop());
        Ok(())
    }

    pub(super) fn purge_transfers(&mut self, transfer_ids: &str) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_purge_transfers() : {:?}",
            transfer_ids
        );

        let transfer_ids: Vec<String> =
            serde_json::from_str(transfer_ids).map_err(|_| ffi::types::NORDDROP_RES_JSON_PARSE)?;

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
            .storage();

        self.rt.block_on(storage.purge_transfers(&transfer_ids));

        Ok(())
    }

    pub(super) fn purge_transfers_until(&mut self, until_timestamp: i64) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_purge_transfers_until() : {:?}",
            until_timestamp
        );

        if !(SQLITE_TIMESTAMP_MIN..=SQLITE_TIMESTAMP_MAX).contains(&until_timestamp) {
            error!(
                self.logger,
                "Invalid timestamp: {until_timestamp}, the value must be between \
                 {SQLITE_TIMESTAMP_MIN} and {SQLITE_TIMESTAMP_MAX}"
            );
            return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
        }

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
            .storage();

        self.rt
            .block_on(storage.purge_transfers_until(until_timestamp));

        Ok(())
    }

    pub(super) fn transfers_since(&mut self, since_timestamp: i64) -> Result<String> {
        trace!(
            self.logger,
            "norddrop_get_transfers_since() since_timestamp: {:?}",
            since_timestamp
        );

        if !(SQLITE_TIMESTAMP_MIN..=SQLITE_TIMESTAMP_MAX).contains(&since_timestamp) {
            error!(
                self.logger,
                "Invalid timestamp: {since_timestamp}, the value must be between \
                 {SQLITE_TIMESTAMP_MIN} and {SQLITE_TIMESTAMP_MAX}"
            );
            return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
        }

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
            .storage();

        let result = self.rt.block_on(storage.transfers_since(since_timestamp));

        let result =
            serde_json::to_string(&result).map_err(|_| ffi::types::NORDDROP_RES_JSON_PARSE)?;
        Ok(result)
    }

    pub(super) fn remove_transfer_file(
        &self,
        transfer_id: uuid::Uuid,
        file_id: &str,
    ) -> Result<()> {
        trace!(
            self.logger,
            "remove_transfer_file() transfer_id: {transfer_id}, file_id: {file_id}",
        );

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
            .storage();

        let res = self
            .rt
            .block_on(storage.remove_transfer_file(transfer_id, file_id));

        res.ok_or(ffi::types::NORDDROP_RES_BAD_INPUT)
    }

    pub(super) fn new_transfer(&mut self, peer: &str, descriptors: &str) -> Result<uuid::Uuid> {
        trace!(
            self.logger,
            "norddrop_new_transfer() to peer {:?}: {:?}",
            peer,
            descriptors
        );

        let descriptors: Vec<TransferDescriptor> = match serde_json::from_str(descriptors) {
            Ok(descriptors) => descriptors,
            Err(e) => {
                error!(
                    self.logger,
                    "Failed to parse new_transfer() descriptors: {}", e
                );
                return Err(ffi::types::NORDDROP_RES_JSON_PARSE);
            }
        };

        let peer = (peer, drop_config::PORT)
            .to_socket_addrs()
            .map_err(|err| {
                error!(self.logger, "Failed to perform lookup of address: {err}");
                ffi::types::NORDDROP_RES_BAD_INPUT
            })?
            .next()
            .ok_or(ffi::types::NORDDROP_RES_BAD_INPUT)?;

        let xfer = {
            let files = self.prepare_transfer_files(&descriptors)?;
            OutgoingTransfer::new(peer.ip(), files, &self.config).map_err(|e| {
                error!(
                    self.logger,
                    "Could not create transfer ({:?}): {}", descriptors, e
                );

                ffi::types::NORDDROP_RES_TRANSFER_CREATE
            })?
        };

        debug!(
            self.logger,
            "Created transfer with files:\n{:#?}",
            xfer.files().values()
        );

        let xfid = xfer.id();

        let mut instance = self.instance.blocking_lock();
        let instance = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?;

        self.rt.block_on(instance.send_request(xfer));

        Ok(xfid)
    }

    pub(super) fn set_peer_state(&mut self, peer: &str, is_online: bool) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_set_peer_state() {:?} -> {:?}",
            peer,
            is_online
        );

        let peer: IpAddr = peer.parse().map_err(|err| {
            error!(self.logger, "Failed to parse peer address: {err}");
            ffi::types::NORDDROP_RES_BAD_INPUT
        })?;

        let mut instance = self.instance.blocking_lock();
        let instance = instance
            .as_mut()
            .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?;

        self.rt.block_on(instance.set_peer_state(peer, is_online));

        Ok(())
    }

    pub(super) fn download(
        &mut self,
        xfid: uuid::Uuid,
        file_id: String,
        dst: String,
    ) -> Result<()> {
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(
            logger,
            "norddrop_download() for transfer {:?}, file {:?}, to {:?}",
            xfid,
            file_id,
            dst
        );

        let mut inst = self.instance.clone().blocking_lock_owned();
        if inst.is_none() {
            return Err(ffi::types::NORDDROP_RES_NOT_STARTED);
        }

        self.rt.spawn(async move {
            let inst = inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst
                .download(xfid, &file_id.clone().into(), dst.as_ref())
                .await
            {
                error!(
                    logger,
                    "Failed to download a file with xfid: {}, file: {:?}, dst: {:?}, error: {:?}",
                    xfid,
                    Hidden(&file_id),
                    Hidden(&dst),
                    e
                );

                ed.dispatch(types::Event::TransferFinished {
                    transfer: xfid.to_string(),
                    data: FinishEvent::FileFailed {
                        file: file_id,
                        status: From::from(&e),
                    },
                    timestamp: utils::current_timestamp(),
                });
            }
        });

        Ok(())
    }

    pub(super) fn cancel_transfer(&mut self, xfid: uuid::Uuid) -> Result<()> {
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(logger, "norddrop_cancel_transfer() for {:?}", xfid);

        let mut inst = self.instance.clone().blocking_lock_owned();
        if inst.is_none() {
            return Err(ffi::types::NORDDROP_RES_NOT_STARTED);
        }

        self.rt.spawn(async move {
            let inst = inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst.cancel_all(xfid).await {
                error!(
                    logger,
                    "Failed to cancel a transfer with xfid: {:?}, error: {:?}", xfid, e
                );

                ed.dispatch(types::Event::TransferFinished {
                    transfer: xfid.to_string(),
                    data: FinishEvent::TransferFailed {
                        status: From::from(&e),
                    },

                    timestamp: utils::current_timestamp(),
                })
            }
        });

        Ok(())
    }

    pub(super) fn reject_file(&self, xfid: uuid::Uuid, file: String) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_reject_file() for transfer {xfid}, file {file}",
        );

        let logger = self.logger.clone();
        let evdisp = self.event_dispatcher.clone();

        let inst = self.instance.clone().blocking_lock_owned();
        if inst.is_none() {
            return Err(ffi::types::NORDDROP_RES_NOT_STARTED);
        }

        self.rt.spawn(async move {
            let inst = inst.as_ref().expect("Instance not initialized");

            if let Err(err) = inst.reject(xfid, file.clone().into()).await {
                error!(
                    logger,
                    "Failed to reject a file with xfid: {xfid}, file: {file}, error: {err:?}"
                );

                evdisp.dispatch(types::Event::TransferFinished {
                    transfer: xfid.to_string(),
                    data: FinishEvent::FileFailed {
                        file,
                        status: From::from(&err),
                    },
                    timestamp: utils::current_timestamp(),
                });
            }
        });

        Ok(())
    }

    #[cfg(unix)]
    pub(super) fn set_fd_resolver_callback(
        &mut self,
        callback: ffi_types::norddrop_fd_cb,
    ) -> Result<()> {
        trace!(self.logger, "norddrop_set_fd_resolver_callback()",);

        let inst = self.instance.blocking_lock();
        if inst.is_some() {
            error!(
                self.logger,
                "Failed to set FD resolver callback. Instance is already started"
            );
            return Err(ffi::types::NORDDROP_RES_ERROR);
        }
        drop(inst);

        self.fdresolv = Some(crate_fd_callback(self.logger.clone(), callback));
        Ok(())
    }

    fn prepare_transfer_files(
        &self,
        descriptors: &[TransferDescriptor],
    ) -> Result<Vec<FileToSend>> {
        let mut gather = drop_transfer::file::GatherCtx::new(&self.config);

        #[cfg(unix)]
        if let Some(fdresolv) = self.fdresolv.as_ref() {
            gather.with_fd_resover(fdresolv.as_ref());
        }

        for desc in descriptors {
            if let Some(content_uri) = &desc.content_uri {
                #[cfg(target_os = "windows")]
                {
                    let _ = content_uri;

                    error!(
                        self.logger,
                        "Specifying file descriptors in transfers is not supported under Windows"
                    );
                    return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
                }

                #[cfg(not(target_os = "windows"))]
                {
                    gather
                        .gather_from_content_uri(&desc.path.0, content_uri.clone(), desc.fd)
                        .map_err(|err| {
                            error!(
                                self.logger,
                                "Could not open file {desc:?} for transfer ({descriptors:?}): \
                                 {err}",
                            );
                            ffi::types::NORDDROP_RES_TRANSFER_CREATE
                        })?;
                }
            } else {
                gather.gather_from_path(&desc.path.0).map_err(|e| {
                    error!(
                        self.logger,
                        "Could not open file {desc:?} for transfer ({descriptors:?}): {e}",
                    );
                    ffi::types::NORDDROP_RES_TRANSFER_CREATE
                })?;
            }
        }

        Ok(gather.take())
    }
}

fn crate_key_context(
    logger: slog::Logger,
    privkey: SecretKey,
    pubkey_cb: ffi_types::norddrop_pubkey_cb,
) -> auth::Context {
    let pubkey_cb = std::sync::Mutex::new(pubkey_cb);
    let public = move |ip: IpAddr| {
        let mut buf = [0u8; PUBLIC_KEY_LENGTH];

        // Insert the trailing null byte
        let cstr_ip = format!("{ip}\0").into_bytes();

        let guard = pubkey_cb.lock().expect("Failed to lock pubkey callback");
        let res = unsafe { (guard.cb)(guard.ctx, cstr_ip.as_ptr() as _, buf.as_mut_ptr() as _) };
        drop(guard);

        if res == 0 {
            debug!(logger, "Public key for {ip:?}: {buf:02X?}");
            Some(PublicKey::from(buf))
        } else {
            None
        }
    };

    auth::Context::new(privkey, public)
}

fn open_database(
    dbpath: &str,
    events: &EventDispatcher,
    logger: &slog::Logger,
    moose: &Arc<dyn drop_analytics::Moose>,
) -> Result<drop_storage::Storage> {
    match drop_storage::Storage::new(logger.clone(), dbpath) {
        Ok(storage) => Ok(storage),
        Err(err) => {
            error!(logger, "Failed to open DB at \"{dbpath}\": {err}",);

            // If we can't even open the DB in memory, there is nothing else left to do,
            // throw an error
            if dbpath == ":memory:" {
                let error = ffi::types::NORDDROP_RES_DB_ERROR;
                moose.developer_exception(DeveloperExceptionEventData {
                    code: error as i32,
                    note: err.to_string(),
                    message: "Failed to open in-memory DB".to_string(),
                    name: "DB Error".to_string(),
                });

                Err(error)
            } else {
                moose.developer_exception(DeveloperExceptionEventData {
                    code: ffi::types::NORDDROP_RES_DB_ERROR as i32,
                    note: "Initial DB open failed, recreating".to_string(),
                    message: "Failed to open DB file".to_string(),
                    name: "DB Error".to_string(),
                });
                // Still problems? Let's try to delete the file, provided it's not in memory
                warn!(logger, "Removing old DB file");
                if let Err(err) = std::fs::remove_file(dbpath) {
                    moose.developer_exception(DeveloperExceptionEventData {
                        code: ffi::types::NORDDROP_RES_DB_ERROR as i32,
                        note: err.to_string(),
                        message: "Failed to remove old DB file".to_string(),
                        name: "DB Error".to_string(),
                    });
                    error!(
                        logger,
                        "Failed to open DB and failed to remove it's file: {err}"
                    );
                    // Try to at least open db in memory if the path doesn't work
                    return open_database(":memory:", events, logger, moose);
                } else {
                    // Inform app that we wiped the old DB file
                    events.dispatch(types::Event::RuntimeError {
                        status: drop_core::Status::DbLost,
                        timestamp: utils::current_timestamp(),
                    });
                };

                // Final try after cleaning up old DB file
                match drop_storage::Storage::new(logger.clone(), dbpath) {
                    Ok(storage) => Ok(storage),
                    Err(err) => {
                        let error = ffi::types::NORDDROP_RES_DB_ERROR;
                        moose.developer_exception(DeveloperExceptionEventData {
                            code: error as i32,
                            note: err.to_string(),
                            message: "Failed to open DB after cleanup".to_string(),
                            name: "DB Error".to_string(),
                        });
                        error!(
                            logger,
                            "Failed to open DB after cleaning up old file: {err}"
                        );
                        Err(error)
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
fn crate_fd_callback(
    logger: slog::Logger,
    fd_cb: ffi_types::norddrop_fd_cb,
) -> Arc<drop_transfer::file::FdResolver> {
    use std::ffi::CString;

    let fd_cb = std::sync::Mutex::new(fd_cb);

    let func = move |uri: &str| {
        let cstr_uri = match CString::new(uri) {
            Ok(uri) => uri,
            Err(err) => {
                warn!(logger, "URI {uri} is invalid: {err}");
                return None;
            }
        };

        let guard = fd_cb.lock().expect("Failed to lock fd callback");
        let res = unsafe { (guard.cb)(guard.ctx, cstr_uri.as_ptr() as _) };
        drop(guard);

        if res < 0 {
            warn!(logger, "FD callback failed for {uri:?}");
            None
        } else {
            Some(res)
        }
    };

    // The callback may block the executor
    let func = move |uri: &str| tokio::task::block_in_place(|| func(uri));

    Arc::new(func)
}

fn parse_and_validate_config(logger: &slog::Logger, config_json: &str) -> Result<Config> {
    let config: Config = match serde_json::from_str::<types::Config>(config_json) {
        Ok(cfg) => {
            debug!(logger, "start() called with config:\n{cfg:#?}");
            cfg.into()
        }
        Err(err) => {
            error!(logger, "Failed to parse config: {err}");
            return Err(ffi::types::NORDDROP_RES_JSON_PARSE);
        }
    };

    if config.moose.event_path.is_empty() {
        error!(logger, "Moose path cannot be empty");
        return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
    }

    Ok(config)
}

fn initialize_moose(
    logger: &slog::Logger,
    MooseConfig {
        event_path,
        prod,
        app_version,
    }: MooseConfig,
) -> Result<Arc<dyn drop_analytics::Moose>> {
    let moose = match drop_analytics::init_moose(
        logger.clone(),
        event_path,
        env!("DROP_VERSION").to_string(),
        app_version,
        prod,
    ) {
        Ok(moose) => moose,
        Err(err) => {
            error!(logger, "Failed to init moose: {err:?}");

            if !prod {
                return Err(ffi::types::NORDDROP_RES_ERROR);
            }

            warn!(logger, "Falling back to mock moose implementation");
            drop_analytics::moose_mock()
        }
    };

    Ok(moose)
}
