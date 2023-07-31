use std::{
    fs,
    net::IpAddr,
    path::{Component, Path},
    sync::Arc,
};

use drop_analytics::Moose;
use drop_config::DropConfig;
use drop_storage::Storage;
use slog::{debug, warn, Logger};
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
    manager,
    transfer::Transfer,
    ws::{self, FileEventTxFactory},
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
    pub(crate) client_stopped: (mpsc::Sender<()>, mpsc::Receiver<()>),
    server_task: JoinHandle<()>,
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
                transfer_manager: TransferManager::new(
                    storage.clone(),
                    FileEventTxFactory::new(event_tx.clone(), moose.clone()),
                    logger.clone(),
                ),
                event_tx,
                moose: moose.clone(),
                config,
                auth: auth.clone(),
                storage,
                #[cfg(unix)]
                fdresolv,
            });

            let stop = CancellationToken::new();
            let client_stopped = mpsc::channel(1);

            manager::resume(&state, stop.clone(), &client_stopped.0, &logger).await;
            let server_task =
                ws::server::start(addr, stop.clone(), state.clone(), auth, logger.clone())?;

            Ok(Self {
                state,
                server_task,
                client_stopped,
                stop,
                logger,
            })
        };

        let res = task.await;
        moose.service_quality_initialization_init(res.to_status(), drop_analytics::Phase::Start);

        res
    }

    pub async fn stop(self) {
        self.stop.cancel();

        if let Err(err) = self.server_task.await {
            warn!(
                self.logger,
                "Failed to wait for server task to finish: {err}"
            );
        }

        // Drop the sender and wait for the receiver to get the notification about last
        // sender being dropped. Based on <https://tokio.rs/tokio/topics/shutdown>
        let (send, mut recv) = self.client_stopped;
        drop(send);
        let _ = recv.recv().await;

        self.state
            .moose
            .service_quality_initialization_init(Ok(()), drop_analytics::Phase::End);
    }

    pub fn purge_transfers(&self, transfer_ids: Vec<String>) {
        self.state.storage.purge_transfers(transfer_ids);
    }

    pub fn purge_transfers_until(&self, until_timestamp: i64) {
        self.state.storage.purge_transfers_until(until_timestamp);
    }

    pub fn transfers_since(&self, since_timestamp: i64) -> Vec<drop_storage::types::Transfer> {
        self.state.storage.transfers_since(since_timestamp)
    }

    pub fn remove_transfer_file(&self, transfer_id: Uuid, file_id: &FileId) -> crate::Result<()> {
        match self
            .state
            .storage
            .remove_transfer_file(transfer_id, file_id.as_ref())
        {
            Some(_) => Ok(()),
            None => {
                warn!(
                    self.logger,
                    "File {} not found in transfer {}", file_id, transfer_id
                );
                Err(Error::InvalidArgument)
            }
        }
    }

    pub async fn send_request(&mut self, xfer: crate::OutgoingTransfer) {
        self.state.moose.service_quality_transfer_batch(
            drop_analytics::Phase::Start,
            xfer.id().to_string(),
            xfer.info(),
        );

        let xfer = Arc::new(xfer);
        if let Err(err) = self
            .state
            .transfer_manager
            .insert_outgoing(xfer.clone())
            .await
        {
            self.state
                .event_tx
                .send(Event::OutgoingTransferFailed(xfer.clone(), err, true))
                .await
                .expect("Event channel should be open");

            return;
        }

        self.state
            .event_tx
            .send(Event::RequestQueued(xfer.clone()))
            .await
            .expect("Could not send a RequestQueued event, channel closed");

        let client_job = ws::client::run(
            self.state.clone(),
            xfer,
            self.client_stopped.0.clone(),
            self.logger.clone(),
        );
        let stop = self.stop.clone();

        tokio::spawn(async move {
            tokio::select! {
                biased;

                _ = stop.cancelled() => (),
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

        let mut lock = self.state.transfer_manager.incoming.lock().await;

        let task = async {
            let state = lock.get_mut(&uuid).ok_or(crate::Error::BadTransfer)?;
            let started = state.validate_for_download(file_id)?;
            Ok((state, started))
        };

        let (state, started) = moose_try_file!(self.state.moose, task.await, uuid, None);

        if started {
            let file_info = state.xfer.files()[file_id].info();

            let mut task = || {
                validate_dest_path(parent_dir)?;
                state.start_download(&self.state.storage, file_id, parent_dir, &self.logger)?;
                Ok(())
            };

            moose_try_file!(self.state.moose, task(), uuid, Some(file_info));
        }
        Ok(())
    }

    /// Cancel a single file in a transfer
    pub async fn cancel(&mut self, transfer_id: Uuid, file: FileId) -> crate::Result<()> {
        let res = self
            .state
            .transfer_manager
            .incoming_cancel_file(transfer_id, &file)
            .await?;

        if let Some(res) = res {
            res.events.cancel_silent().await;

            self.state
                .event_tx
                .send(crate::Event::FileDownloadCancelled(res.xfer, file, false))
                .await
                .expect("Event channel should be open");
        }

        Ok(())
    }

    /// Reject a single file in a transfer. After rejection the file can no
    /// logner be transfered
    pub async fn reject(&self, transfer_id: Uuid, file: FileId) -> crate::Result<()> {
        {
            match self
                .state
                .transfer_manager
                .outgoing_rejection_post(transfer_id, &file)
                .await
            {
                Ok(res) => {
                    res.events.cancelled_on_rejection().await;

                    self.state
                        .event_tx
                        .send(crate::Event::FileUploadRejected {
                            transfer_id,
                            file_id: file,
                            by_peer: false,
                        })
                        .await
                        .expect("Event channel should be open");

                    return Ok(());
                }
                Err(crate::Error::BadTransfer) => (),
                Err(err) => return Err(err),
            }
        }
        {
            match self
                .state
                .transfer_manager
                .incoming_rejection_post(transfer_id, &file)
                .await
            {
                Ok(res) => {
                    res.events.cancelled_on_rejection().await;

                    self.state
                        .event_tx
                        .send(crate::Event::FileDownloadRejected {
                            transfer_id,
                            file_id: file,
                            by_peer: false,
                        })
                        .await
                        .expect("Event channel should be open");

                    return Ok(());
                }
                Err(crate::Error::BadTransfer) => (),
                Err(err) => return Err(err),
            }
        }

        Err(crate::Error::BadTransfer)
    }

    /// Cancel all of the files in a transfer
    pub async fn cancel_all(&mut self, transfer_id: Uuid) -> crate::Result<()> {
        {
            match self
                .state
                .transfer_manager
                .outgoing_issue_close(transfer_id)
                .await
            {
                Ok(res) => {
                    futures::future::join_all(res.events.iter().map(|ev| ev.cancel_silent())).await;

                    self.state
                        .event_tx
                        .send(crate::Event::OutgoingTransferCanceled(res.xfer, false))
                        .await
                        .expect("Event channel should be open");

                    return Ok(());
                }
                Err(crate::Error::BadTransfer) => (),
                Err(err) => return Err(err),
            }
        }
        {
            match self
                .state
                .transfer_manager
                .incoming_issue_close(transfer_id)
                .await
            {
                Ok(res) => {
                    futures::future::join_all(res.events.iter().map(|ev| ev.cancel_silent())).await;

                    self.state
                        .event_tx
                        .send(crate::Event::IncomingTransferCanceled(res.xfer, false))
                        .await
                        .expect("Event channel should be open");

                    return Ok(());
                }
                Err(crate::Error::BadTransfer) => (),
                Err(err) => return Err(err),
            }
        }

        Err(crate::Error::BadTransfer)
    }
}

fn validate_dest_path(parent_dir: &Path) -> crate::Result<()> {
    if parent_dir.components().any(|x| x == Component::ParentDir) {
        return Err(crate::Error::BadPath(
            "Path should not contain a reference to parrent directory".into(),
        ));
    }

    if parent_dir.ancestors().any(Path::is_symlink) {
        return Err(crate::Error::BadPath(
            "Destination should not contain directory symlinks".into(),
        ));
    }

    fs::create_dir_all(parent_dir).map_err(|ioerr| crate::Error::BadPath(ioerr.to_string()))?;

    Ok(())
}
