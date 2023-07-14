use std::{
    fs,
    net::IpAddr,
    path::{Component, Path},
    sync::Arc,
};

use drop_analytics::Moose;
use drop_config::DropConfig;
use drop_storage::Storage;
use slog::{debug, error, warn, Logger};
use tokio::{
    sync::{mpsc, Semaphore},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth,
    error::ResultExt,
    file::File,
    transfer::Transfer,
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
    },
    Error, Event, FileId, TransferManager,
};

pub(super) struct State {
    pub(super) event_tx: mpsc::Sender<Event>,
    pub(super) transfer_manager: TransferManager,
    pub(crate) moose: Arc<dyn Moose>,
    pub(crate) auth: Arc<auth::Context>,
    pub(crate) config: Arc<DropConfig>,
    pub(crate) storage: Arc<Storage>,
    pub(crate) throttle: Semaphore,
    #[cfg(unix)]
    pub fdresolv: Option<Arc<crate::file::FdResolver>>,
}

pub struct Service {
    pub(super) state: Arc<State>,
    pub(crate) stop: CancellationToken,
    join_handle: JoinHandle<()>,
    pub(super) logger: Logger,
}

macro_rules! moose_try_file {
    ($moose:expr, $func:expr, $xfer_id:expr, $file_info:expr) => {
        match $func {
            Ok(r) => r,
            Err(e) => {
                $moose.service_quality_transfer_file(
                    Err(u32::from(&e) as i32),
                    drop_analytics::Phase::Start,
                    $xfer_id.to_string(),
                    0,
                    $file_info,
                );

                return Err(e);
            }
        }
    };
}

// todo: better name to reduce confusion
impl Service {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        addr: IpAddr,
        storage: Arc<Storage>,
        event_tx: mpsc::Sender<Event>,
        logger: Logger,
        config: Arc<DropConfig>,
        moose: Arc<dyn Moose>,
        auth: Arc<auth::Context>,
        #[cfg(unix)] fdresolv: Option<Arc<crate::FdResolver>>,
    ) -> Result<Self, Error> {
        let task = async {
            let state = Arc::new(State {
                throttle: Semaphore::new(config.max_uploads_in_flight),
                event_tx,
                transfer_manager: Default::default(),
                moose: moose.clone(),
                config,
                auth: auth.clone(),
                storage,
                #[cfg(unix)]
                fdresolv,
            });

            let stop = CancellationToken::new();
            let join_handle =
                ws::server::start(addr, stop.clone(), state.clone(), auth, logger.clone())?;

            ws::client::resume(&state, &stop, &logger).await;

            Ok(Self {
                state,
                join_handle,
                stop,
                logger,
            })
        };

        let res = task.await;
        moose.service_quality_initialization_init(res.to_status(), drop_analytics::Phase::Start);

        res
    }

    pub async fn stop(self) -> Result<(), Error> {
        let task = async {
            self.stop.cancel();
            self.join_handle.await.map_err(|_| Error::ServiceStop)
        };

        let res = task.await;

        self.state
            .moose
            .service_quality_initialization_init(res.to_status(), drop_analytics::Phase::End);

        res
    }

    pub fn purge_transfers(&self, transfer_ids: Vec<String>) -> Result<(), Error> {
        if let Err(e) = self.state.storage.purge_transfers(transfer_ids) {
            error!(self.logger, "Failed to purge transfers: {e}");
            return Err(Error::StorageError);
        }

        Ok(())
    }

    pub fn purge_transfers_until(&self, until_timestamp: i64) -> Result<(), Error> {
        if let Err(e) = self.state.storage.purge_transfers_until(until_timestamp) {
            error!(self.logger, "Failed to purge transfers until: {e}");
            return Err(Error::StorageError);
        }

        Ok(())
    }

    pub fn transfers_since(
        &self,
        since_timestamp: i64,
    ) -> Result<Vec<drop_storage::types::Transfer>, Error> {
        let result = self.state.storage.transfers_since(since_timestamp);

        match result {
            Err(e) => {
                error!(self.logger, "Failed to get transfers since: {e}");
                Err(Error::StorageError)
            }
            Ok(transfers) => Ok(transfers),
        }
    }

    pub fn remove_transfer_file(&self, transfer_id: Uuid, file_id: &FileId) -> crate::Result<()> {
        match self
            .state
            .storage
            .remove_transfer_file(transfer_id, file_id.as_ref())
        {
            Ok(Some(())) => Ok(()),
            Ok(None) => {
                warn!(self.logger, "File {file_id} not removed from {transfer_id}");
                Err(Error::InvalidArgument)
            }
            Err(err) => {
                error!(self.logger, "Failed to remove transfer file: {err}");
                Err(Error::StorageError)
            }
        }
    }

    pub async fn send_request(&mut self, xfer: crate::OutgoingTransfer) {
        self.state.moose.service_quality_transfer_batch(
            drop_analytics::Phase::Start,
            xfer.id().to_string(),
            xfer.info(),
        );

        if let Err(err) = self.state.storage.insert_transfer(&xfer.storage_info()) {
            error!(self.logger, "Failed to insert transfer into storage: {err}",);
        }

        let xfer = Arc::new(xfer);
        let stop_job = {
            let state = self.state.clone();
            let xfer = xfer.clone();
            let logger = self.logger.clone();

            async move {
                // Stop the download job
                warn!(logger, "Aborting transfer download");

                state
                    .event_tx
                    .send(Event::OutgoingTransferFailed(
                        xfer,
                        crate::Error::Canceled,
                        true,
                    ))
                    .await
                    .expect("Failed to send TransferFailed event");
            }
        };

        let client_job = ws::client::run(self.state.clone(), xfer, self.logger.clone());
        let stop = self.stop.clone();

        tokio::spawn(async move {
            tokio::select! {
                biased;

                _ = stop.cancelled() => {
                    stop_job.await;
                },
                _ = client_job => (),
            }
        });
    }

    pub async fn download(
        &mut self,
        uuid: Uuid,
        file_id: &FileId,
        parent_dir: &Path,
    ) -> crate::Result<()> {
        debug!(
            self.logger,
            "Client::download() called with Uuid: {}, file: {:?}, parent_dir: {}",
            uuid,
            file_id,
            parent_dir.display()
        );

        let fetch_xfer = async {
            let mut lock = self.state.transfer_manager.incoming.lock().await;

            let state = lock.get_mut(&uuid).ok_or(Error::BadTransfer)?;
            state.rejections.ensure_not_rejected(file_id)?;

            let file = state.xfer.files().get(file_id).ok_or(Error::BadFileId)?;

            Ok((state.xfer.clone(), file.clone(), state.conn.clone()))
        };

        let (xfer, file, channel) = moose_try_file!(self.state.moose, fetch_xfer.await, uuid, None);
        let file_info = file.info();

        let dispatch_task = || {
            // Path validation
            if parent_dir.components().any(|x| x == Component::ParentDir) {
                return Err(Error::BadPath(
                    "Path should not contain a reference to parrent directory".into(),
                ));
            }

            // Check if target directory is a symlink
            if parent_dir.ancestors().any(Path::is_symlink) {
                return Err(Error::BadPath(
                    "Destination should not contain directory symlinks".into(),
                ));
            }

            fs::create_dir_all(parent_dir).map_err(|ioerr| Error::BadPath(ioerr.to_string()))?;

            let task = FileXferTask::new(file, xfer, parent_dir.into());
            channel
                .send(ServerReq::Download {
                    task: Box::new(task),
                })
                .map_err(|err| Error::BadTransferState(err.to_string()))?;

            Ok(())
        };

        moose_try_file!(self.state.moose, dispatch_task(), uuid, Some(file_info));

        Ok(())
    }

    /// Cancel a single file in a transfer
    pub async fn cancel(&mut self, xfer_uuid: Uuid, file: FileId) -> crate::Result<()> {
        {
            let lock = self.state.transfer_manager.incoming.lock().await;
            if let Some(state) = lock.get(&xfer_uuid) {
                state.rejections.ensure_not_rejected(&file)?;
                state
                    .conn
                    .send(ServerReq::Cancel { file })
                    .map_err(|err| Error::BadTransferState(err.to_string()))?;

                return Ok(());
            }
        }
        {
            let lock = self.state.transfer_manager.outgoing.lock().await;
            if let Some(state) = lock.get(&xfer_uuid) {
                state.rejections.ensure_not_rejected(&file)?;
                state
                    .conn
                    .send(ClientReq::Cancel { file })
                    .map_err(|err| Error::BadTransferState(err.to_string()))?;

                return Ok(());
            }
        }

        Err(Error::BadTransfer)
    }

    /// Reject a single file in a transfer. After rejection the file can no
    /// logner be transfered
    pub async fn reject(&self, transfer_id: Uuid, file: FileId) -> crate::Result<()> {
        {
            let mut lock = self.state.transfer_manager.incoming.lock().await;
            if let Some(state) = lock.get_mut(&transfer_id) {
                if !state.rejections.reject(file.clone())? {
                    return Err(crate::Error::Rejected);
                }

                state
                    .conn
                    .send(ServerReq::Reject { file })
                    .map_err(|err| Error::BadTransferState(err.to_string()))?;
                return Ok(());
            }
        }
        {
            let mut lock = self.state.transfer_manager.outgoing.lock().await;
            if let Some(state) = lock.get_mut(&transfer_id) {
                if !state.rejections.reject(file.clone())? {
                    return Err(crate::Error::Rejected);
                }

                state
                    .conn
                    .send(ClientReq::Reject { file })
                    .map_err(|err| Error::BadTransferState(err.to_string()))?;
                return Ok(());
            }
        }

        Err(Error::BadTransfer)
    }

    /// Cancel all of the files in a transfer
    pub async fn cancel_all(&mut self, transfer_id: Uuid) -> crate::Result<()> {
        {
            let mut lock = self.state.transfer_manager.incoming.lock().await;
            if let Some(state) = lock.remove(&transfer_id) {
                state.xfer.files().values().for_each(|file| {
                    let status: u32 = From::from(&Error::Canceled);

                    self.state.moose.service_quality_transfer_file(
                        Err(status as _),
                        drop_analytics::Phase::End,
                        state.xfer.id().to_string(),
                        0,
                        Some(file.info()),
                    )
                });

                return Ok(());
            }
        }
        {
            let mut lock = self.state.transfer_manager.outgoing.lock().await;
            if let Some(state) = lock.remove(&transfer_id) {
                state.xfer.files().values().for_each(|file| {
                    let status: u32 = From::from(&Error::Canceled);

                    self.state.moose.service_quality_transfer_file(
                        Err(status as _),
                        drop_analytics::Phase::End,
                        state.xfer.id().to_string(),
                        0,
                        Some(file.info()),
                    )
                });

                return Ok(());
            }
        }

        Err(Error::BadTransfer)
    }
}
