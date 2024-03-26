use std::{ffi::CStr, sync::Mutex, time::SystemTime};

use drop_auth::SecretKey;
use libc::{c_char, c_int, c_void};
use slog::{o, Drain};

use crate::{
    device::{types, NordDropFFI},
    ffi,
};

pub type Result<T> = std::result::Result<T, crate::Error>;

pub trait EventCallback: Send + Sync {
    fn on_event(&self, event: String);
}

pub trait Logger: Send + Sync {
    fn on_log(&self, level: crate::LogLevel, msg: String);
    fn level(&self) -> crate::LogLevel;
}

pub trait KeyStore: Send + Sync {
    fn on_pubkey(&self, peer: String) -> Option<Vec<u8>>;
    fn privkey(&self) -> Vec<u8>;
}

pub trait FdResolver: Send + Sync {
    fn on_fd(&self, content_uri: String) -> Option<i32>;
}

pub struct NordDrop {
    dev: Mutex<NordDropFFI>,
    _cb: Box<CbContext>,
    #[cfg(unix)]
    fd_resolv: Mutex<Option<Box<FdResolverContext>>>,
}

#[cfg(unix)]
struct FdResolverContext {
    resolver: Box<dyn FdResolver>,
}

struct CbContext {
    event_callback: Box<dyn EventCallback>,
    key_store: Box<dyn KeyStore>,
    logger: Box<dyn Logger>,
}

pub enum TransferDescriptor {
    Path {
        path: String,
    },
    Fd {
        filename: String,
        content_uri: String,
        fd: Option<i32>,
    },
}

impl NordDrop {
    pub fn new(
        event_callback: Box<dyn EventCallback>,
        key_store: Box<dyn KeyStore>,
        logger: Box<dyn Logger>,
    ) -> Result<Self> {
        // TODO(msz): remove indirection

        let mut cb = Box::new(CbContext {
            event_callback,
            key_store,
            logger,
        });

        unsafe extern "C" fn call_on_event(ctx: *mut c_void, ev: *const c_char) {
            let ctx = ctx as *const CbContext;
            (*ctx).event_callback.on_event(
                CStr::from_ptr(ev as _)
                    .to_str()
                    .expect("Invalid C string")
                    .to_owned(),
            );
        }

        let event_callback_c = ffi::norddrop_event_cb {
            ctx: cb.as_mut() as *mut _ as *mut _,
            cb: call_on_event,
        };

        unsafe extern "C" fn call_on_pubkey(
            ctx: *mut c_void,
            peer: *const c_char,
            pubkey: *mut c_char,
        ) -> c_int {
            let ctx = ctx as *const CbContext;
            let key = (*ctx).key_store.on_pubkey(
                CStr::from_ptr(peer as _)
                    .to_str()
                    .expect("Invalid C string")
                    .to_owned(),
            );
            match key {
                Some(key) => {
                    if key.len() != drop_auth::PUBLIC_KEY_LENGTH {
                        1
                    } else {
                        pubkey.copy_from_nonoverlapping(
                            key.as_ptr().cast(),
                            drop_auth::PUBLIC_KEY_LENGTH,
                        );
                        0
                    }
                }
                None => 1,
            }
        }

        let pubkey_callback_c = ffi::norddrop_pubkey_cb {
            ctx: cb.as_mut() as *mut _ as *mut _,
            cb: call_on_pubkey,
        };

        unsafe extern "C" fn call_on_log(
            ctx: *mut c_void,
            lvl: crate::LogLevel,
            msg: *const c_char,
        ) {
            let ctx = ctx as *const CbContext;
            (*ctx).logger.on_log(
                lvl,
                CStr::from_ptr(msg as _)
                    .to_str()
                    .expect("Invalid C string")
                    .to_owned(),
            );
        }

        let logger_callback_c = ffi::norddrop_logger_cb {
            ctx: cb.as_mut() as *mut _ as *mut _,
            cb: call_on_log,
        };

        let logger = slog::Logger::root(
            logger_callback_c
                .filter_level(cb.logger.level().into())
                .fuse(),
            o!(),
        );

        let privkey = cb.key_store.privkey();
        if privkey.len() != drop_auth::SECRET_KEY_LENGTH {
            return Err(crate::Error::NORDDROP_RES_INVALID_PRIVKEY);
        }
        let mut privkey_buf = [0u8; drop_auth::SECRET_KEY_LENGTH];
        privkey_buf.copy_from_slice(&privkey);
        let privkey = SecretKey::from(privkey_buf);

        let dev = NordDropFFI::new(event_callback_c, pubkey_callback_c, privkey, logger)?;

        Ok(Self {
            dev: Mutex::new(dev),
            _cb: cb,
            fd_resolv: Mutex::new(None),
        })
    }

    #[cfg(not(unix))]
    pub fn set_fd_resolver(&self, resolver: Box<dyn FdResolver>) -> Result<()> {
        Err(crate::Error::NORDDROP_RES_ERROR)
    }

    #[cfg(unix)]
    pub fn set_fd_resolver(&self, resolver: Box<dyn FdResolver>) -> Result<()> {
        unsafe extern "C" fn call_on_fd(ctx: *mut c_void, uri: *const c_char) -> c_int {
            let ctx = ctx as *const FdResolverContext;
            (*ctx)
                .resolver
                .on_fd(
                    CStr::from_ptr(uri as _)
                        .to_str()
                        .expect("Invalid C string")
                        .to_owned(),
                )
                .unwrap_or(-1)
        }

        let mut ctx = Box::new(FdResolverContext { resolver });

        let fd_callback_c = ffi::types::norddrop_fd_cb {
            ctx: ctx.as_mut() as *mut _ as *mut _,
            cb: call_on_fd,
        };

        self.dev
            .lock()
            .expect("Poisoned lock")
            .set_fd_resolver_callback(fd_callback_c)?;

        *self.fd_resolv.lock().expect("Poisoned lock") = Some(ctx);

        Ok(())
    }

    pub fn start(&self, addr: &str, config: crate::Config) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").start(
            addr,
            &serde_json::to_string(&config).expect("Failed to serialize JSON"),
        )
    }

    pub fn stop(&self) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").stop()
    }

    pub fn purge_transfers(&self, transfers: &[String]) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev
            .lock()
            .expect("Poisoned lock")
            .purge_transfers(&serde_json::to_string(transfers).expect("Failed to serialize JSON"))
    }

    pub fn purge_transfers_until(&self, until: SystemTime) -> Result<()> {
        // TODO(msz): remove indirection

        // The `device` function takes in seconds as an argument
        self.dev
            .lock()
            .expect("Poisoned lock")
            .purge_transfers_until(
                until
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as _),
            )
    }

    pub fn transfers_since(&self, since: SystemTime) -> Result<String> {
        // TODO(msz): remove indirection

        // The `device` function takes in seconds as an argument
        self.dev.lock().expect("Poisoned lock").transfers_since(
            since
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as _),
        )
    }

    pub fn new_transfer(&self, peer: &str, descriptors: &[TransferDescriptor]) -> Result<String> {
        // TODO(msz): remove indirection

        let descrs: Vec<_> = descriptors
            .iter()
            .map(|td| {
                let desc = match td {
                    TransferDescriptor::Path { path } => types::TransferDescriptor {
                        path: path.clone().into(),
                        content_uri: None,
                        fd: None,
                    },
                    TransferDescriptor::Fd {
                        filename,
                        content_uri,
                        fd,
                    } => types::TransferDescriptor {
                        path: filename.clone().into(),
                        content_uri: Some(
                            content_uri
                                .parse()
                                .map_err(|_| crate::Error::NORDDROP_RES_INVALID_STRING)?,
                        ),
                        fd: *fd,
                    },
                };
                Ok(desc)
            })
            .collect::<Result<_>>()?;

        let transfer_id = self.dev.lock().expect("Poisoned lock").new_transfer(
            peer,
            &serde_json::to_string(&descrs).expect("Failed to serialize JSON"),
        )?;

        Ok(transfer_id.to_string())
    }

    pub fn finish_transfer(&self, transfer_id: &str) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").cancel_transfer(
            transfer_id
                .parse()
                .map_err(|_| crate::Error::NORDDROP_RES_INVALID_STRING)?,
        )
    }

    pub fn remove_file(&self, transfer_id: &str, file_id: &str) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev
            .lock()
            .expect("Poisoned lock")
            .remove_transfer_file(
                transfer_id
                    .parse()
                    .map_err(|_| crate::Error::NORDDROP_RES_INVALID_STRING)?,
                file_id,
            )
    }

    pub fn download_file(&self, transfer_id: &str, file_id: &str, destination: &str) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").download(
            transfer_id
                .parse()
                .map_err(|_| crate::Error::NORDDROP_RES_INVALID_STRING)?,
            file_id.to_string(),
            destination.to_string(),
        )
    }

    pub fn reject_file(&self, transfer_id: &str, file_id: &str) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").reject_file(
            transfer_id
                .parse()
                .map_err(|_| crate::Error::NORDDROP_RES_INVALID_STRING)?,
            file_id.to_string(),
        )
    }

    pub fn network_refresh(&self) -> Result<()> {
        // TODO(msz): remove indirection

        self.dev.lock().expect("Poisoned lock").network_refresh()
    }
}

pub fn version() -> String {
    env!("DROP_VERSION").to_string()
}
