use std::{
    fs,
    net::IpAddr,
    path::{Component, Path},
    sync::Arc,
};

use drop_analytics::Moose;
use drop_config::DropConfig;
use drop_core::Status;
use drop_storage::Storage;
use slog::{debug, Logger};
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth,
    error::ResultExt,
    manager,
    tasks::AliveWaiter,
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
    stop: CancellationToken,
    waiter: AliveWaiter,
    pub(super) logger: Logger,
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

            let waiter = AliveWaiter::new();
            let stop = CancellationToken::new();

            let guard = waiter.guard();

            manager::resume(&state, &logger, &guard, &stop).await;
            ws::server::spawn(
                addr,
                state.clone(),
                auth,
                logger.clone(),
                stop.clone(),
                guard,
            )?;

            Ok(Self {
                state,
                stop,
                waiter,
                logger,
            })
        };

        let res = task.await;
        moose.service_quality_initialization_init(res.to_status());

        res
    }

    pub async fn stop(self) {
        self.stop.cancel();

        self.waiter.wait_for_all().await;
    }

    pub fn storage(&self) -> &Storage {
        &self.state.storage
    }

    pub async fn send_request(&mut self, xfer: crate::OutgoingTransfer) {
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

        ws::client::spawn(
            self.state.clone(),
            xfer,
            self.logger.clone(),
            self.waiter.guard(),
            self.stop.clone(),
        );
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

        let state = lock.get_mut(&uuid).ok_or(crate::Error::BadTransfer)?;
        let started = state.validate_for_download(file_id)?;

        if started {
            validate_dest_path(parent_dir)?;

            state
                .start_download(&self.state.storage, file_id, parent_dir)
                .await?;
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
            res.events.stop_silent(Status::Canceled).await;

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
                    res.events.rejected(false).await;
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
                    res.events.rejected(false).await;
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
                    futures::future::join_all(
                        res.events.iter().map(|ev| ev.stop_silent(Status::Canceled)),
                    )
                    .await;

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
                    futures::future::join_all(
                        res.events.iter().map(|ev| ev.stop_silent(Status::Canceled)),
                    )
                    .await;

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
