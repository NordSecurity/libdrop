use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
    time::SystemTime,
};

use drop_analytics::DeveloperExceptionEventData;
use drop_auth::{PublicKey, SecretKey};
use drop_config::{Config, DropConfig, MooseConfig};
use drop_storage::types::Transfer as TransferInfo;
use drop_transfer::{auth, utils::Hidden, Event, FileToSend, OutgoingTransfer, Service, Transfer};
use slog::{debug, error, trace, warn, Logger};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};

use crate::{event, TransferDescriptor};

pub type Result<T = ()> = std::result::Result<T, crate::Error>;

const SQLITE_TIMESTAMP_MIN: i64 = -210866760000;
const SQLITE_TIMESTAMP_MAX: i64 = 253402300799;

pub(super) struct NordDropFFI {
    rt: tokio::runtime::Runtime,
    pub logger: Logger,
    instance: Arc<Mutex<Option<ServiceData>>>,
    event_dispatcher: EventDispatcher,
    keys: Arc<auth::Context>,
    config: DropConfig,
    #[cfg(unix)]
    fdresolv: Option<Arc<drop_transfer::file::FdResolver>>,
}

struct ServiceData {
    service: drop_transfer::Service,
    event_task: JoinHandle<()>,
}

#[derive(Clone)]
struct EventDispatcher {
    cb: Arc<dyn Fn(crate::Event) + Send + Sync>,
}

impl EventDispatcher {
    fn dispatch(&self, e: impl Into<crate::Event>) {
        (self.cb)(e.into());
    }
}

impl NordDropFFI {
    pub(super) fn new(
        event_cb: impl Fn(crate::Event) + Send + Sync + 'static,
        pubkey_cb: impl Fn(IpAddr) -> Option<PublicKey> + Send + 'static,
        privkey: SecretKey,
        logger: Logger,
    ) -> Result<Self> {
        trace!(logger, "norddrop_new()");

        // It's a debug print. Not visible in the production build
        debug!(logger, "Private key: {:02X?}", privkey.to_bytes());

        Ok(NordDropFFI {
            instance: Arc::default(),
            logger: logger.clone(),
            rt: tokio::runtime::Runtime::new().map_err(|_| crate::Error::Unknown)?,
            event_dispatcher: EventDispatcher {
                cb: Arc::new(event_cb) as _,
            },
            config: DropConfig::default(),
            keys: Arc::new(crate_key_context(logger, privkey, pubkey_cb)),
            #[cfg(unix)]
            fdresolv: None,
        })
    }

    pub(super) fn start(&mut self, listen_addr: &str, config: Config) -> Result<()> {
        let init_time = std::time::Instant::now();
        trace!(
            self.logger,
            "norddrop_start() listen address: {:?}",
            listen_addr,
        );

        // Check preconditions first
        validate_config(&self.logger, &config)?;
        let addr: IpAddr = match listen_addr.parse() {
            Ok(addr) => addr,
            Err(err) => {
                error!(self.logger, "Failed to parse IP address: {err}");
                return Err(crate::Error::BadInput);
            }
        };

        let mut instance = self.instance.blocking_lock();
        if instance.is_some() {
            return Err(crate::Error::InstanceStart);
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

        let event_task = self.rt.spawn(async move {
            let mut dispatch = drop_transfer::StorageDispatch::new(&event_storage);

            while let Some(e) = rx.recv().await {
                debug!(event_logger, "emitting event: {:#?}", e);

                dispatch.handle_event(&e.0).await;
                // Android team reported problems with the event ordering.
                // The events where dispatched in different order than where emitted.
                // To fix that we need to process the events sequentially.
                // Also the callback may block the executor - we need to be resistant to that.
                tokio::task::block_in_place(|| ed.dispatch(e));
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
            Ok(service) => instance.replace(ServiceData {
                service,
                event_task,
            }),
            Err(err) => {
                error!(self.logger, "Failed to start the service: {}", err);

                let err = match err {
                    drop_transfer::Error::AddrInUse => crate::Error::AddrInUse,
                    _ => crate::Error::InstanceStart,
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
            .ok_or(crate::Error::NotStarted)?;

        self.rt.block_on(async {
            instance.service.stop().await;
            let _ = instance.event_task.await;
        });

        Ok(())
    }

    pub(super) fn purge_transfers(&mut self, transfer_ids: &[String]) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_purge_transfers() : {:?}",
            transfer_ids
        );

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(crate::Error::NotStarted)?
            .service
            .storage();

        self.rt.block_on(storage.purge_transfers(transfer_ids));
        Ok(())
    }

    pub(super) fn purge_transfers_until(&mut self, until_timestamp_s: i64) -> Result<()> {
        trace!(
            self.logger,
            "norddrop_purge_transfers_until() : {:?}",
            until_timestamp_s
        );

        if !(SQLITE_TIMESTAMP_MIN..=SQLITE_TIMESTAMP_MAX).contains(&until_timestamp_s) {
            error!(
                self.logger,
                "Invalid timestamp: {until_timestamp_s}, the value must be between \
                 {SQLITE_TIMESTAMP_MIN} and {SQLITE_TIMESTAMP_MAX}"
            );
            return Err(crate::Error::BadInput);
        }

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(crate::Error::NotStarted)?
            .service
            .storage();

        self.rt
            .block_on(storage.purge_transfers_until(until_timestamp_s));

        Ok(())
    }

    pub(super) fn transfers_since(&mut self, since_timestamp_s: i64) -> Result<Vec<TransferInfo>> {
        trace!(
            self.logger,
            "norddrop_get_transfers_since() since_timestamp: {:?}",
            since_timestamp_s
        );

        if !(SQLITE_TIMESTAMP_MIN..=SQLITE_TIMESTAMP_MAX).contains(&since_timestamp_s) {
            error!(
                self.logger,
                "Invalid timestamp: {since_timestamp_s}, the value must be between \
                 {SQLITE_TIMESTAMP_MIN} and {SQLITE_TIMESTAMP_MAX}"
            );
            return Err(crate::Error::BadInput);
        }

        let mut instance = self.instance.blocking_lock();
        let storage = instance
            .as_mut()
            .ok_or(crate::Error::NotStarted)?
            .service
            .storage();

        let result = self.rt.block_on(storage.transfers_since(since_timestamp_s));
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
            .ok_or(crate::Error::NotStarted)?
            .service
            .storage();

        let res = self
            .rt
            .block_on(storage.remove_transfer_file(transfer_id, file_id));

        res.ok_or(crate::Error::BadInput)
    }

    pub(super) fn new_transfer(
        &mut self,
        peer: &str,
        descriptors: &[TransferDescriptor],
    ) -> Result<uuid::Uuid> {
        trace!(self.logger, "norddrop_new_transfer() to peer {peer:?}",);

        let peer = (peer, drop_config::PORT)
            .to_socket_addrs()
            .map_err(|err| {
                error!(self.logger, "Failed to perform lookup of address: {err}");
                crate::Error::BadInput
            })?
            .next()
            .ok_or(crate::Error::BadInput)?;

        let xfer = {
            let files = self.prepare_transfer_files(descriptors)?;
            OutgoingTransfer::new(peer.ip(), files, &self.config).map_err(|e| {
                error!(self.logger, "Could not create transfer: {e}");
                crate::Error::TransferCreate
            })?
        };

        debug!(
            self.logger,
            "Created transfer with files:\n{:#?}",
            xfer.files().values()
        );

        let xfid = xfer.id();

        let mut instance = self.instance.blocking_lock();
        let instance = instance.as_mut().ok_or(crate::Error::NotStarted)?;

        self.rt.block_on(instance.service.send_request(xfer));

        Ok(xfid)
    }

    pub(super) fn network_refresh(&mut self) -> Result<()> {
        trace!(self.logger, "norddrop_network_refresh()");

        let mut instance = self.instance.blocking_lock();
        let instance = instance.as_mut().ok_or(crate::Error::NotStarted)?;

        instance.service.network_refresh();

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
            return Err(crate::Error::NotStarted);
        }

        self.rt.spawn(async move {
            let inst = inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst
                .service
                .download(xfid, &file_id.clone().into(), &dst)
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

                ed.dispatch(event::EventKind::FileFailed {
                    transfer_id: xfid.to_string(),
                    file_id,
                    status: From::from(&e),
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
            return Err(crate::Error::NotStarted);
        }

        self.rt.spawn(async move {
            let inst = inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst.service.cancel_all(xfid).await {
                error!(
                    logger,
                    "Failed to cancel a transfer with xfid: {:?}, error: {:?}", xfid, e
                );

                ed.dispatch(crate::EventKind::TransferFailed {
                    transfer_id: xfid.to_string(),
                    status: From::from(&e),
                });
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
            return Err(crate::Error::NotStarted);
        }

        self.rt.spawn(async move {
            let inst = inst.as_ref().expect("Instance not initialized");

            if let Err(err) = inst.service.reject(xfid, file.clone().into()).await {
                error!(
                    logger,
                    "Failed to reject a file with xfid: {xfid}, file: {file}, error: {err:?}"
                );

                evdisp.dispatch(crate::EventKind::FileFailed {
                    transfer_id: xfid.to_string(),
                    file_id: file,
                    status: From::from(&err),
                });
            }
        });

        Ok(())
    }

    #[cfg(unix)]
    pub(super) fn set_fd_resolver_callback(
        &mut self,
        callback: impl Fn(&str) -> Option<std::os::fd::RawFd> + Send + 'static,
    ) -> Result<()> {
        trace!(self.logger, "norddrop_set_fd_resolver_callback()",);

        let inst = self.instance.blocking_lock();
        if inst.is_some() {
            error!(
                self.logger,
                "Failed to set FD resolver callback. Instance is already started"
            );
            return Err(crate::Error::Unknown);
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
            match desc {
                #[cfg(windows)]
                TransferDescriptor::Fd { .. } => {
                    error!(self.logger, "FD transfers are not supported on Windows");
                    return Err(crate::Error::TransferCreate);
                }
                #[cfg(unix)]
                TransferDescriptor::Fd {
                    filename,
                    content_uri,
                    fd,
                } => {
                    let uri = content_uri
                        .parse()
                        .map_err(|_| crate::Error::InvalidString)?;

                    gather
                        .gather_from_content_uri(filename, uri, *fd)
                        .map_err(|err| {
                            error!(
                                self.logger,
                                "Could not open file {:?} ({:?}) for transfer: {err}",
                                Hidden(filename),
                                Hidden(content_uri)
                            );
                            crate::Error::TransferCreate
                        })?;
                }
                TransferDescriptor::Path { path } => {
                    gather.gather_from_path(path).map_err(|e| {
                        error!(
                            self.logger,
                            "Could not open file {:?} for transfer: {e}",
                            Hidden(path)
                        );
                        crate::Error::TransferCreate
                    })?;
                }
            }
        }

        Ok(gather.take())
    }
}

fn crate_key_context(
    logger: slog::Logger,
    privkey: SecretKey,
    pubkey_cb: impl Fn(IpAddr) -> Option<PublicKey> + Send + 'static,
) -> auth::Context {
    let pubkey_cb = std::sync::Mutex::new(pubkey_cb);
    let public = move |ip: IpAddr| {
        let guard = pubkey_cb.lock().expect("Failed to lock pubkey callback");
        let key = guard(ip)?;
        drop(guard);

        debug!(logger, "Public key for {ip:?}: {key:?}");
        Some(key)
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
                let error = crate::Error::DbError;
                moose.developer_exception(DeveloperExceptionEventData {
                    code: error as i32,
                    note: err.to_string(),
                    message: "Failed to open in-memory DB".to_string(),
                    name: "DB Error".to_string(),
                });

                Err(error)
            } else {
                moose.developer_exception(DeveloperExceptionEventData {
                    code: crate::Error::DbError as i32,
                    note: "Initial DB open failed, recreating".to_string(),
                    message: "Failed to open DB file".to_string(),
                    name: "DB Error".to_string(),
                });
                // Still problems? Let's try to delete the file, provided it's not in memory
                warn!(logger, "Removing old DB file");
                if let Err(err) = std::fs::remove_file(dbpath) {
                    moose.developer_exception(DeveloperExceptionEventData {
                        code: crate::Error::DbError as i32,
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
                    events.dispatch(crate::EventKind::RuntimeError {
                        status: drop_core::Status::DbLost as _,
                    });
                };

                // Final try after cleaning up old DB file
                match drop_storage::Storage::new(logger.clone(), dbpath) {
                    Ok(storage) => Ok(storage),
                    Err(err) => {
                        let error = crate::Error::DbError;
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
    fd_cb: impl Fn(&str) -> Option<std::os::fd::RawFd> + Send + 'static,
) -> Arc<drop_transfer::file::FdResolver> {
    let fd_cb = std::sync::Mutex::new(fd_cb);

    let func = move |uri: &str| {
        let guard = fd_cb.lock().expect("Failed to lock fd callback");
        let res = guard(uri);
        drop(guard);

        if res.is_none() {
            warn!(logger, "FD callback failed for {uri:?}");
        }
        res
    };

    // The callback may block the executor
    let func = move |uri: &str| tokio::task::block_in_place(|| func(uri));

    Arc::new(func)
}

fn validate_config(logger: &slog::Logger, config: &Config) -> Result<()> {
    if config.moose.event_path.is_empty() {
        error!(logger, "Moose path cannot be empty");
        return Err(crate::Error::BadInput);
    }

    Ok(())
}

fn initialize_moose(
    logger: &slog::Logger,
    MooseConfig { event_path, prod, app_version }: MooseConfig,
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
                error!(
                    logger,
                    "Moose is in debug mode and failed to initialize. Bailing initialization"
                );

                return Err(crate::Error::Unknown);
            }

            warn!(logger, "Falling back to mock moose implementation");
            drop_analytics::moose_mock()
        }
    };

    Ok(moose)
}
