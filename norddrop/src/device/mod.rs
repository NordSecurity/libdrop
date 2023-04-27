pub mod types;

use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
};

use drop_auth::{PublicKey, SecretKey, PUBLIC_KEY_LENGTH};
use drop_config::Config;
use drop_transfer::{auth, utils::Hidden, File, Service, Transfer};
use slog::{debug, error, trace, warn, Logger};
use tokio::sync::{mpsc, Mutex};

use self::types::TransferDescriptor;
use crate::{device::types::FinishEvent, ffi, ffi::types as ffi_types};

pub type Result<T = ()> = std::result::Result<T, ffi::types::norddrop_result>;

pub(super) struct NordDropFFI {
    rt: tokio::runtime::Runtime,
    pub logger: Logger,
    instance: Arc<Mutex<Option<drop_transfer::Service>>>,
    event_dispatcher: Arc<EventDispatcher>,
    config: Config,
    keys: Arc<auth::Context>,
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
        debug!(logger, "Private key: {:02X?}", privkey.as_bytes());

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

        // Spawn a task grabbing events from the inner service and dispatch them
        // to the host app
        let ed = self.event_dispatcher.clone();
        let event_logger = self.logger.clone();
        self.rt.spawn(async move {
            while let Some(e) = rx.recv().await {
                debug!(event_logger, "emitting event: {:#?}", e);

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
                tx,
                self.logger.clone(),
                self.config.drop,
                moose,
                self.keys.clone(),
            ) {
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
                .await
                .map_err(|_| ffi::types::NORDDROP_RES_INSTANCE_STOP)
        })
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
            let files = descriptors
                .iter()
                .map(|desc| {
                    #[cfg(target_os = "windows")]
                    {
                        if desc.fd.is_some() {
                            error!(
                                self.logger,
                                "Specifying file descriptors in transfers is not supported under \
                                 Windows"
                            );
                            return Err(ffi::types::NORDDROP_RES_BAD_INPUT);
                        }
                    }

                    File::from_path(&desc.path.0, desc.fd, &self.config.drop).map_err(|e| {
                        error!(
                            self.logger,
                            "Could not open file {:?} for transfer ({:?}): {}",
                            desc,
                            descriptors,
                            e
                        );
                        ffi::types::NORDDROP_RES_TRANSFER_CREATE
                    })
                })
                .collect::<Result<Vec<File>>>()?;

            Transfer::new(peer.ip(), files, &self.config.drop).map_err(|e| {
                error!(
                    self.logger,
                    "Could not create transfer ({:?}): {}", descriptors, e
                );

                ffi::types::NORDDROP_RES_TRANSFER_CREATE
            })?
        };

        debug!(
            self.logger,
            "Created transfer with files: {:?}",
            xfer.files()
        );

        let xfid = xfer.id();

        self.rt.block_on(async {
            self.instance
                .lock()
                .await
                .as_mut()
                .ok_or(ffi::types::NORDDROP_RES_NOT_STARTED)?
                .send_request(xfer);

            Result::Ok(())
        })?;

        Ok(xfid)
    }

    pub(super) fn download(&mut self, xfid: uuid::Uuid, file: String, dst: String) -> Result<()> {
        let instance = self.instance.clone();
        let logger = self.logger.clone();
        let ed = self.event_dispatcher.clone();

        trace!(
            logger,
            "norddrop_download() for transfer {:?}, file {:?}, to {:?}",
            xfid,
            file,
            dst
        );

        self.rt.spawn(async move {
            let mut locked_inst = instance.lock().await;
            let inst = locked_inst.as_mut().expect("Instance not initialized");

            if let Err(e) = inst.download(xfid, (&file).into(), dst.as_ref()).await {
                error!(
                    logger,
                    "Failed to download a file with xfid: {}, file: {:?}, dst: {:?}, error: {:?}",
                    xfid,
                    Hidden(&file),
                    Hidden(&dst),
                    e
                );

                ed.dispatch(types::Event::TransferFinished {
                    transfer: xfid.to_string(),
                    data: FinishEvent::FileFailed {
                        file,
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

    pub(super) fn cancel_file(&mut self, xfid: uuid::Uuid, file: String) -> Result<()> {
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

            if let Err(e) = inst.cancel(xfid, (&file).into()).await {
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

        Ok(())
    }
}

fn crate_key_context(
    logger: slog::Logger,
    privkey: SecretKey,
    pubkey_cb: ffi_types::norddrop_pubkey_cb,
) -> auth::Context {
    let pubkey_cb = std::sync::Mutex::new(pubkey_cb);
    let public = move |ip: Option<IpAddr>| {
        let mut buf = [0u8; PUBLIC_KEY_LENGTH];

        let cstr_ip = ip.map(|ip| {
            // Insert the trailing null byte
            format!("{ip}\0").into_bytes()
        });

        let guard = pubkey_cb.lock().expect("Failed to lock pubkey callback");
        let res = unsafe {
            (guard.cb)(
                guard.ctx,
                cstr_ip
                    .as_ref()
                    .map(|v| v.as_ptr())
                    .unwrap_or(std::ptr::null()) as *const _,
                buf.as_mut_ptr() as _,
            )
        };
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
