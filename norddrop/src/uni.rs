use std::sync::Mutex;

use drop_auth::{PublicKey, SecretKey, PUBLIC_KEY_LENGTH};

use crate::{device::NordDropFFI, Event, TransferDescriptor, TransferInfo};

pub type Result<T> = std::result::Result<T, crate::LibdropError>;

pub trait EventCallback: Send + Sync {
    fn on_event(&self, event: Event);
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
}

impl NordDrop {
    pub fn new(
        event_callback: Box<dyn EventCallback>,
        key_store: Box<dyn KeyStore>,
        logger: Box<dyn Logger>,
    ) -> Result<Self> {
        let logger = super::log::create(logger);

        let privkey = key_store.privkey();
        let privkey: [u8; PUBLIC_KEY_LENGTH] = privkey
            .try_into()
            .map_err(|_| crate::LibdropError::InvalidPrivkey)?;
        let privkey = SecretKey::from(privkey);

        let dev = NordDropFFI::new(
            move |ev| event_callback.on_event(ev),
            move |peer_ip| {
                let pubkey = key_store.on_pubkey(peer_ip.to_string())?;
                let pubkey: [u8; PUBLIC_KEY_LENGTH] = pubkey.try_into().ok()?;
                Some(PublicKey::from(pubkey))
            },
            privkey,
            logger,
        )?;

        Ok(Self {
            dev: Mutex::new(dev),
        })
    }

    #[cfg(not(unix))]
    pub fn set_fd_resolver(&self, resolver: Box<dyn FdResolver>) -> Result<()> {
        Err(crate::LibdropError::Unknown)
    }

    #[cfg(unix)]
    pub fn set_fd_resolver(&self, resolver: Box<dyn FdResolver>) -> Result<()> {
        self.dev
            .lock()
            .expect("Poisoned lock")
            .set_fd_resolver_callback(move |uri| resolver.on_fd(uri.to_string()))?;

        Ok(())
    }

    pub fn start(&self, addr: &str, config: crate::Config) -> Result<()> {
        self.dev
            .lock()
            .expect("Poisoned lock")
            .start(addr, config.into())
    }

    pub fn stop(&self) -> Result<()> {
        self.dev.lock().expect("Poisoned lock").stop()
    }

    pub fn purge_transfers(&self, transfer_ids: &[String]) -> Result<()> {
        self.dev
            .lock()
            .expect("Poisoned lock")
            .purge_transfers(transfer_ids)
    }

    pub fn purge_transfers_until(&self, until: i64) -> Result<()> {
        // The `device` function takes in seconds as an argument and this function takes
        // in ms
        self.dev
            .lock()
            .expect("Poisoned lock")
            .purge_transfers_until(until / 100)
    }

    pub fn transfers_since(&self, since: i64) -> Result<Vec<TransferInfo>> {
        // The `device` function takes in seconds as an argument and this function takes
        // in ms
        let infos = self
            .dev
            .lock()
            .expect("Poisoned lock")
            .transfers_since(since / 100)?;

        let xfers = infos.into_iter().map(TransferInfo::from).collect();
        Ok(xfers)
    }

    pub fn new_transfer(&self, peer: &str, descriptors: &[TransferDescriptor]) -> Result<String> {
        let transfer_id = self
            .dev
            .lock()
            .expect("Poisoned lock")
            .new_transfer(peer, descriptors)?;

        Ok(transfer_id.to_string())
    }

    pub fn finalize_transfer(&self, transfer_id: &str) -> Result<()> {
        self.dev.lock().expect("Poisoned lock").cancel_transfer(
            transfer_id
                .parse()
                .map_err(|_| crate::LibdropError::InvalidString)?,
        )
    }

    pub fn remove_file(&self, transfer_id: &str, file_id: &str) -> Result<()> {
        self.dev
            .lock()
            .expect("Poisoned lock")
            .remove_transfer_file(
                transfer_id
                    .parse()
                    .map_err(|_| crate::LibdropError::InvalidString)?,
                file_id,
            )
    }

    pub fn download_file(&self, transfer_id: &str, file_id: &str, destination: &str) -> Result<()> {
        self.dev.lock().expect("Poisoned lock").download(
            transfer_id
                .parse()
                .map_err(|_| crate::LibdropError::InvalidString)?,
            file_id.to_string(),
            destination.to_string(),
        )
    }

    pub fn reject_file(&self, transfer_id: &str, file_id: &str) -> Result<()> {
        self.dev.lock().expect("Poisoned lock").reject_file(
            transfer_id
                .parse()
                .map_err(|_| crate::LibdropError::InvalidString)?,
            file_id.to_string(),
        )
    }

    pub fn network_refresh(&self) -> Result<()> {
        self.dev.lock().expect("Poisoned lock").network_refresh()
    }
}

pub fn version() -> String {
    env!("DROP_VERSION").to_string()
}
