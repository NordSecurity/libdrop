use std::{
    fs,
    net::IpAddr,
    path::{Component, Path},
    sync::Arc,
};

use drop_analytics::Moose;
use drop_config::DropConfig;
use slog::{debug, error, warn, Logger};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth,
    error::ResultExt,
    manager::TransferConnection,
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
    },
    Error, Event, FileId, TransferManager,
};

pub(super) struct State {
    pub(super) event_tx: mpsc::Sender<Event>,
    pub(super) transfer_manager: Mutex<TransferManager>,
    pub(crate) moose: Arc<dyn Moose>,
    pub(crate) auth: Arc<auth::Context>,
    pub(crate) config: Arc<DropConfig>,
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
    pub fn start(
        addr: IpAddr,
        storage: Arc<drop_storage::Storage>,
        event_tx: mpsc::Sender<Event>,
        logger: Logger,
        config: Arc<DropConfig>,
        moose: Arc<dyn Moose>,
        auth: Arc<auth::Context>,
    ) -> Result<Self, Error> {
        let task = || {
            let state = Arc::new(State {
                event_tx,
                transfer_manager: Mutex::new(TransferManager::new(logger.clone(), storage)),
                moose: moose.clone(),
                config,
                auth: auth.clone(),
            });

            let stop = CancellationToken::new();
            let join_handle =
                ws::server::start(addr, stop.clone(), state.clone(), auth, logger.clone())?;

            Ok(Self {
                state,
                join_handle,
                stop,
                logger,
            })
        };

        let res = task();
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

    pub fn send_request(&mut self, xfer: crate::Transfer) {
        self.state.moose.service_quality_transfer_batch(
            drop_analytics::Phase::Start,
            xfer.id().to_string(),
            xfer.info(),
        );

        let stop_job = {
            let state = self.state.clone();
            let xfer = xfer.clone();
            let logger = self.logger.clone();

            async move {
                // Stop the download job
                warn!(logger, "Aborting transfer download");

                state
                    .event_tx
                    .send(Event::TransferFailed(xfer, crate::Error::Canceled, true))
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
            let mut lock = self.state.transfer_manager.lock().await;

            let chann = lock.connection(uuid).ok_or(Error::BadTransfer)?;
            let chann = match chann {
                TransferConnection::Server(chann) => chann.clone(),
                _ => return Err(Error::BadTransfer),
            };

            let mapped_file_path =
                parent_dir.join(lock.apply_dir_mapping(uuid, parent_dir, file_id)?);

            let xfer = lock.transfer(&uuid).ok_or(Error::BadTransfer)?.clone();

            Ok((xfer, chann, mapped_file_path))
        };

        let (xfer, channel, location) =
            moose_try_file!(self.state.moose, fetch_xfer.await, uuid, None);

        let file = moose_try_file!(
            self.state.moose,
            xfer.files().get(file_id).ok_or(Error::BadFileId),
            uuid,
            None
        )
        .clone();

        let file_info = file.info();

        // Path validation
        if location.components().any(|x| x == Component::ParentDir) {
            let err = Err(Error::BadPath);
            moose_try_file!(self.state.moose, err, uuid, file_info);
        }

        let parent_location = moose_try_file!(
            self.state.moose,
            location.parent().ok_or(Error::BadPath),
            uuid,
            file_info
        );

        // Check if target directory is a symlink
        if parent_location.ancestors().any(Path::is_symlink) {
            error!(
                self.logger,
                "Destination should not contain directory symlinks"
            );
            moose_try_file!(self.state.moose, Err(Error::BadPath), uuid, file_info);
        }

        moose_try_file!(
            self.state.moose,
            fs::create_dir_all(parent_location).map_err(|_| Error::BadPath),
            uuid,
            file_info
        );

        let task = moose_try_file!(
            self.state.moose,
            FileXferTask::new(file, xfer, location),
            uuid,
            file_info
        );

        channel
            .send(ServerReq::Download {
                task: Box::new(task),
            })
            .map_err(|_| Error::BadTransfer)?;

        Ok(())
    }

    /// Cancel a single file in a transfer
    pub async fn cancel(&mut self, xfer_uuid: Uuid, file: FileId) -> crate::Result<()> {
        let lock = self.state.transfer_manager.lock().await;

        let conn = lock.connection(xfer_uuid).ok_or(Error::BadTransfer)?;

        match conn {
            TransferConnection::Client(conn) => {
                conn.send(ClientReq::Cancel { file })
                    .map_err(|_| Error::BadTransferState)?;
            }
            TransferConnection::Server(conn) => {
                conn.send(ServerReq::Cancel { file })
                    .map_err(|_| Error::BadTransferState)?;
            }
        }

        Ok(())
    }

    /// Cancel all of the files in a transfer
    pub async fn cancel_all(&mut self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.state.transfer_manager.lock().await;

        {
            let xfer = lock.transfer(&transfer_id).ok_or(Error::BadTransfer)?;

            xfer.files().values().for_each(|file| {
                let status: u32 = From::from(&Error::Canceled);

                self.state.moose.service_quality_transfer_file(
                    Err(status as _),
                    drop_analytics::Phase::End,
                    xfer.id().to_string(),
                    0,
                    file.info(),
                )
            });
        }

        if let Err(e) = lock.cancel_transfer(transfer_id) {
            error!(
                self.logger,
                "Could not cancel transfer(client): {}. xfer: {}", e, transfer_id,
            );
        }

        Ok(())
    }
}
