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
    error::ResultExt,
    manager::TransferConnection,
    ws::{
        self,
        client::ClientReq,
        server::{FileXferTask, ServerReq},
    },
    Error, Event, TransferManager,
};

pub(super) struct State {
    pub(super) event_tx: mpsc::Sender<Event>,
    pub(super) transfer_manager: Mutex<TransferManager>,
    pub(crate) moose: Arc<dyn Moose>,
    pub(crate) config: DropConfig,
}

pub struct Service {
    pub(super) state: Arc<State>,
    pub(crate) stop: CancellationToken,
    join_handle: JoinHandle<()>,
    pub(super) logger: Logger,
}

macro_rules! moose_try_file {
    ($moose:expr, $func:expr, $xfer_id:expr, $file_size:expr) => {
        match $func {
            Ok(r) => r,
            Err(e) => {
                $moose.service_quality_transfer_file(
                    Err(u32::from(&e) as i32),
                    drop_analytics::Phase::Start,
                    $xfer_id.to_string(),
                    $file_size,
                    0,
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
        event_tx: mpsc::Sender<Event>,
        logger: Logger,
        config: DropConfig,
        moose: Arc<dyn Moose>,
    ) -> Result<Self, Error> {
        let task = || {
            let state = Arc::new(State {
                event_tx,
                transfer_manager: Mutex::default(),
                moose: moose.clone(),
                config,
            });

            let stop = CancellationToken::new();
            let join_handle = ws::server::start(addr, stop.clone(), state.clone(), logger.clone())?;

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
        let sizes = xfer.sizes_kb();

        self.state.moose.service_quality_transfer_batch(
            drop_analytics::Phase::Start,
            sizes.len() as i32,
            sizes
                .iter()
                .map(|size| size.to_string())
                .collect::<Vec<String>>()
                .join(","),
            xfer.id().to_string(),
            sizes.iter().sum(),
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
                    .send(Event::TransferFailed(xfer, crate::Error::Canceled))
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
        file_id: &Path,
        parent_dir: &Path,
    ) -> crate::Result<()> {
        debug!(
            self.logger,
            "Client::download() called with Uuid: {}, file: {}, parent_dir: {}",
            uuid,
            file_id.display(),
            parent_dir.display()
        );

        let fetch_xfer = async {
            let lock = self.state.transfer_manager.lock().await;

            let xfer = lock.transfer(&uuid).ok_or(Error::BadTransfer)?;
            let chann = lock.connection(uuid).ok_or(Error::BadTransfer)?;

            let chann = match chann {
                TransferConnection::Server(chann) => chann.clone(),
                _ => return Err(Error::BadTransfer),
            };

            Ok((xfer.clone(), chann))
        };

        let (xfer, channel) = moose_try_file!(self.state.moose, fetch_xfer.await, uuid, None);

        let file = moose_try_file!(
            self.state.moose,
            xfer.file(file_id).ok_or(Error::BadFileId),
            uuid,
            None
        )
        .clone();

        let size_kb = file.size_kb().ok_or(Error::DirectoryNotExpected)?;
        let location = parent_dir.join(file.path());

        // Path validation
        if location.components().any(|x| x == Component::ParentDir) {
            let err = Err(Error::BadPath);
            moose_try_file!(self.state.moose, err, uuid, Some(size_kb));
        }

        let parent_location = moose_try_file!(
            self.state.moose,
            location.parent().ok_or(Error::BadPath),
            uuid,
            Some(size_kb)
        );

        // Check if target directory is a symlink
        if parent_location.ancestors().any(Path::is_symlink) {
            error!(
                self.logger,
                "Destination should not contain directory symlinks"
            );
            moose_try_file!(self.state.moose, Err(Error::BadPath), uuid, Some(size_kb));
        }

        moose_try_file!(
            self.state.moose,
            fs::create_dir_all(parent_location).map_err(|_| Error::BadPath),
            uuid,
            Some(size_kb)
        );

        let task = moose_try_file!(
            self.state.moose,
            FileXferTask::new(file, xfer, location),
            uuid,
            Some(size_kb)
        );

        channel
            .send(ServerReq::Download {
                file: file_id.into(),
                task: Box::new(task),
            })
            .map_err(|_| Error::BadTransfer)?;

        Ok(())
    }

    /// Cancel a single file in a transfer
    pub async fn cancel(&mut self, xfer_uuid: Uuid, file: &Path) -> crate::Result<()> {
        let lock = self.state.transfer_manager.lock().await;

        let conn = lock.connection(xfer_uuid).ok_or(Error::BadTransfer)?;

        match conn {
            TransferConnection::Client(conn) => {
                conn.send(ClientReq::Cancel { file: file.into() })
                    .map_err(|_| Error::BadTransferState)?;
            }
            TransferConnection::Server(conn) => {
                conn.send(ServerReq::Cancel { file: file.into() })
                    .map_err(|_| Error::BadTransferState)?;
            }
        }

        Ok(())
    }

    /// Cancel all of the files in a transfer
    pub async fn cancel_all(&mut self, transfer_id: Uuid) -> crate::Result<()> {
        let mut lock = self.state.transfer_manager.lock().await;

        let fids = lock.get_transfer_files(transfer_id);

        {
            let xfer = lock.transfer(&transfer_id).ok_or(Error::BadTransfer)?;

            if let Some(fids) = fids {
                fids.iter().for_each(|id| {
                    let status: u32 = From::from(&Error::Canceled);

                    self.state.moose.service_quality_transfer_file(
                        Err(status as _),
                        drop_analytics::Phase::End,
                        xfer.id().to_string(),
                        xfer.file(id).expect("Bad file").size_kb(),
                        0,
                    )
                });
            }
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
