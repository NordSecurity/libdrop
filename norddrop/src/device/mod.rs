pub mod types;

use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
};

use drop_auth::{PublicKey, SecretKey, PUBLIC_KEY_LENGTH};
use drop_config::Config;
use drop_transfer::{auth, utils::Hidden, FileToSend, OutgoingTransfer, Service, Transfer};
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
    config: Config,
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
            config: Config::default(),
            keys: Arc::new(crate_key_context(logger, privkey, pubkey_cb)),
            #[cfg(unix)]
            fdresolv: None,
        })
    }

    pub(super) fn start(&mut self, listen_addr: &str, config_json: &str) -> Result<()> {
        let logger = self.logger.clone();

        trace!(logger, "norddrop_start() listen address: {:?}", listen_addr,);

        let config: types::Config = match serde_json::from_str(config_json) {
            Ok(cfg) => {
                debug!(logger, "start() called with config:\n{:#?}", cfg,);
                cfg
            }
            Err(err) => {
                error!(logger, "Failed to parse config: {}", err);
                return Err(ffi::types::NORDDROP_RES_JSON_PARSE);
            }
        };

        self.config = config.into();

        if self.config.moose.event_path.is_empty() {
            error!(logger, "Moose path cannot be empty");
            return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
        }

        let moose = match drop_analytics::init_moose(
            logger.clone(),
            self.config.moose.event_path.clone(),
            env!("DROP_VERSION").to_string(),
            self.config.moose.prod,
        ) {
            Ok(moose) => moose,
            Err(err) => {
                error!(logger, "Failed to init moose: {:?}", err);

                if !self.config.moose.prod {
                    return Err(ffi::types::NORDDROP_RES_ERROR);
                }

                warn!(logger, "Falling back to mock moose implementation");
                drop_analytics::moose_mock()
            }
        };

        let addr: IpAddr = match listen_addr.parse() {
            Ok(addr) => addr,
            Err(err) => {
                error!(logger, "Failed to parse IP address: {err}");
                return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
            }
        };

        let (tx, mut rx) = mpsc::channel::<drop_transfer::Event>(16);

        let storage = Arc::new(open_database(
            &self.config.drop.storage_path,
            &self.event_dispatcher,
            &self.logger,
            &moose,
        )?);

        // Spawn a task grabbing events from the inner service and dispatch them
        // to the host app
        let ed = self.event_dispatcher.clone();
        let event_logger = self.logger.clone();
        let event_storage = storage.clone();

        self.rt.spawn(async move {
            let mut dispatch = drop_transfer::StorageDispatch::new(&event_storage);

            while let Some(e) = rx.recv().await {
                debug!(event_logger, "emitting event: {:#?}", e);

                dispatch.handle_event(&e).await;
                // Android team reported problems with the event ordering.
                // The events where dispatched in different order than where emitted.
                // To fix that we need to process the events sequentially.
                // Also the callback may block the executor - we need to be resistant to that.
                tokio::task::block_in_place(|| ed.dispatch(e.into()));
            }
        });

        self.rt.block_on(async {
            let service = match Service::start(
                addr,
                storage,
                tx,
                self.logger.clone(),
                Arc::new(self.config.drop.clone()),
                moose,
                self.keys.clone(),
                #[cfg(unix)]
                self.fdresolv.clone(),
            )
            .await
            {
                Ok(srv) => srv,
                Err(err) => {
                    error!(self.logger, "Failed to start the service: {}", err);

                    let err = match err {
                        drop_transfer::Error::AddrInUse => ffi::types::NORDDROP_RES_ADDR_IN_USE,
                        _ => ffi::types::NORDDROP_RES_INSTANCE_START,
                    };

                    return Err(err);
                }
            };

            self.instance.lock().await.replace(service);
            Ok(())
        })?;

        Ok(())
    }

    pub(super) fn stop(&mut self) -> Result<()> {
        trace!(self.logger, "norddrop_stop()");

        self.rt.block_on(async {
            self.instance
                .lock()
                .await
                .take()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .stop()
                .await;
            Ok(())
        })
    }

    pub(super) fn purge_transfers(&mut self, transfer_ids: &str) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_purge_transfers() : {:?}",
            transfer_ids
        );

        let transfer_ids: Vec<String> =
            serde_json::from_str(transfer_ids).map_err(|_| ffi::types::NORDDROP_RES_JSON_PARSE)?;

        self.rt.block_on(async {
            self.instance
                .lock()
                .await
                .as_mut()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .storage()
                .purge_transfers(transfer_ids)
                .await;

            Ok(())
        })
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

        self.rt.block_on(async {
            self.instance
                .lock()
                .await
                .as_mut()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .storage()
                .purge_transfers_until(until_timestamp)
                .await;

            Ok(())
        })
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

        let result = self.rt.block_on(async {
            let transfers = self
                .instance
                .lock()
                .await
                .as_mut()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .storage()
                .transfers_since(since_timestamp)
                .await;

            Ok::<Vec<_>, ffi::types::norddrop_result>(transfers)
        })?;

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

        let res = self.rt.block_on(async {
            Ok::<Option<()>, ffi::types::norddrop_result>(
                self.instance
                    .lock()
                    .await
                    .as_ref()
                    .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                    .storage()
                    .remove_transfer_file(transfer_id, file_id)
                    .await,
            )
        })?;

        match res {
            Some(_) => Ok(()),
            None => Err(ffi::types::NORDDROP_RES_BAD_INPUT),
        }
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
            OutgoingTransfer::new(peer.ip(), files, &self.config.drop).map_err(|e| {
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

        self.rt.block_on(async {
            self.instance
                .lock()
                .await
                .as_mut()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .send_request(xfer)
                .await;

            Result::Ok(())
        })?;

        Ok(xfid)
    }

    pub(super) fn download(
        &mut self,
        xfid: uuid::Uuid,
        file_id: String,
        dst: String,
    ) -> Result<()> {
        let instance = self.instance.clone();
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(
            logger,
            "norddrop_download() for transfer {:?}, file {:?}, to {:?}",
            xfid,
            file_id,
            dst
        );

        self.rt.spawn(async move {
            let mut locked_inst = instance.lock().await;
            let inst = locked_inst.as_mut().expect("Instance not initialized");

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
                });
            }
        });

        Ok(())
    }

    pub(super) fn cancel_transfer(&mut self, xfid: uuid::Uuid) -> Result<()> {
        let instance = self.instance.clone();
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(logger, "norddrop_cancel_transfer() for {:?}", xfid);

        self.rt.spawn(async move {
            let mut locked_inst = instance.lock().await;
            let inst = locked_inst.as_mut().expect("Instance not initialized");

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
                })
            }
        });

        Ok(())
    }

    pub(super) fn cancel_file(&mut self, xfid: uuid::Uuid, file: String) {
        let instance = self.instance.clone();
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(
            logger,
            "norddrop_cancel_file() for transfer {:?}, file {:?}",
            xfid,
            file
        );

        self.rt.spawn(async move {
            let mut locked_inst = instance.lock().await;
            let inst = locked_inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst.cancel(xfid, file.clone().into()).await {
                error!(
                    logger,
                    "Failed to cancel a file with xfid: {}, file: {:?}, error: {:?}",
                    xfid,
                    Hidden(&file),
                    e
                );

                ed.dispatch(types::Event::TransferFinished {
                    transfer: xfid.to_string(),
                    data: FinishEvent::FileFailed {
                        file,
                        status: From::from(&e),
                    },
                })
            }
        });
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
        let mut files = Vec::new();

        #[allow(unused_variables)]
        for (i, desc) in descriptors.iter().enumerate() {
            if let Some(content_uri) = &desc.content_uri {
                #[cfg(target_os = "windows")]
                {
                    error!(
                        self.logger,
                        "Specifying file descriptors in transfers is not supported under Windows"
                    );
                    return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
                }

                #[cfg(not(target_os = "windows"))]
                {
                    let fd = if let Some(fd) = desc.fd {
                        fd
                    } else {
                        let fdresolv = if let Some(fdresolv) = self.fdresolv.as_ref() {
                            fdresolv
                        } else {
                            error!(
                                self.logger,
                                "Content URI provided but RD resolver callback is not set up"
                            );
                            return Err(ffi::types::NORDDROP_RES_TRANSFER_CREATE);
                        };

                        if let Some(fd) = fdresolv(content_uri.as_str()) {
                            fd
                        } else {
                            error!(self.logger, "Failed to fetch FD for file: {content_uri}");
                            return Err(ffi::types::NORDDROP_RES_TRANSFER_CREATE);
                        }
                    };

                    let file = FileToSend::from_fd(&desc.path.0, content_uri.clone(), fd, i)
                        .map_err(|e| {
                            error!(
                                self.logger,
                                "Could not open file {desc:?} for transfer ({descriptors:?}): {e}",
                            );
                            ffi::types::NORDDROP_RES_TRANSFER_CREATE
                        })?;

                    files.push(file);
                }
            } else {
                let batch =
                    FileToSend::from_path(&desc.path.0, &self.config.drop).map_err(|e| {
                        error!(
                            self.logger,
                            "Could not open file {desc:?} for transfer ({descriptors:?}): {e}",
                        );
                        ffi::types::NORDDROP_RES_TRANSFER_CREATE
                    })?;

                files.extend(batch);
            }
        }

        Ok(files)
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
        Ok(storage) => return Ok(storage),
        Err(err) => error!(logger, "Failed to open DB at \"{dbpath}\": {err}",),
    }

    // If we can't even open the DB in memory, there is nothing else left to do,
    // throw an error
    if dbpath == ":memory:" {
        let error = ffi::types::NORDDROP_RES_DB_ERROR;
        let error_msg = "Failed to open in-memory DB";
        moose.developer_exception(
            error as i32,
            "".to_string(),
            error_msg.to_string(),
            "Database Error".to_string(),
        );

        error!(logger, "{}", error_msg);
        Err(error)
    } else {
        moose.developer_exception(
            ffi::types::NORDDROP_RES_DB_ERROR as i32,
            "Initial DB open failed, recreating".to_string(),
            "Failed to open database".to_string(),
            "Database Error".to_string(),
        );
        // Still problems? Let's try to delete the file, provided it's not in memory
        warn!(logger, "Removing old DB file");
        if let Err(err) = std::fs::remove_file(dbpath) {
            let error_msg = format!("Failed to open DB and failed to remove it's file: {err}");
            moose.developer_exception(
                ffi::types::NORDDROP_RES_DB_ERROR as i32,
                "".to_string(),
                error_msg.to_string(),
                "Database Error".to_string(),
            );
            error!(logger, "{}", error_msg);
            // Try to at least open db in memory if the path doesn't work
            return open_database(":memory:", events, logger, moose);
        } else {
            // Inform app that we wiped the old DB file
            events.dispatch(types::Event::RuntimeError {
                status: drop_core::Status::DbLost,
            });
        };

        // Final try after cleaning up old DB file
        match drop_storage::Storage::new(logger.clone(), dbpath) {
            Ok(storage) => Ok(storage),
            Err(err) => {
                let error = ffi::types::NORDDROP_RES_DB_ERROR;
                let error_msg = format!("Failed to open DB after cleaning up old file: {err}");
                moose.developer_exception(
                    error as i32,
                    "".to_string(),
                    error_msg.to_string(),
                    "Database Error".to_string(),
                );
                error!(logger, "{}", error_msg);
                Err(error)
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
