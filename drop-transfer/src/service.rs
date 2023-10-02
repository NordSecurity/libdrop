use std::{
    collections::HashMap,
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
    auth, check,
    error::ResultExt,
    manager,
    tasks::AliveWaiter,
    ws::{self, FileEventTxFactory},
    Error, Event, FileId, Transfer, TransferManager,
};

pub(super) struct State {
    pub(super) event_tx: mpsc::UnboundedSender<Event>,
    pub(super) transfer_manager: TransferManager,
    pub(crate) moose: Arc<dyn Moose>,
    pub(crate) auth: Arc<auth::Context>,
    pub(crate) config: Arc<DropConfig>,
    pub(crate) storage: Arc<Storage>,
    pub(crate) throttle: Semaphore,
    pub(crate) addr: IpAddr,
    #[cfg(unix)]
    pub fdresolv: Option<Arc<crate::file::FdResolver>>,
}

type TaskRegistry = HashMap<uuid::Uuid, tokio::task::JoinHandle<()>>;

pub struct Service {
    pub(super) state: Arc<State>,
    stop: CancellationToken,
    waiter: AliveWaiter,
    pub(super) logger: Logger,

    tasks: TaskRegistry,
}

impl Service {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        addr: IpAddr,
        storage: Arc<Storage>,
        event_tx: mpsc::UnboundedSender<Event>,
        logger: Logger,
        config: Arc<DropConfig>,
        moose: Arc<dyn Moose>,
        auth: Arc<auth::Context>,
        #[cfg(unix)] fdresolv: Option<Arc<crate::FdResolver>>,
    ) -> Result<Self, Error> {
        let task = async {
            let state = Arc::new(State {
                throttle: Semaphore::new(drop_config::MAX_UPLOADS_IN_FLIGHT), /* TODO: max uploads of 4 files per all libdrop is too restrictive, workout a better plan, like configurable through config */
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
                addr,
                #[cfg(unix)]
                fdresolv,
            });

            let waiter = AliveWaiter::new();
            let stop = CancellationToken::new();

            let guard = waiter.guard();

            state.storage.cleanup_garbage_transfers().await;

            manager::restore_transfers_state(&state, &logger).await;

            ws::server::spawn(state.clone(), logger.clone(), stop.clone(), guard.clone())?;

            let mut service = Self {
                state,
                stop,
                waiter,
                logger,
                tasks: Default::default(),
            };

            let outgoing_transfers = {
                let xfers = service.state.transfer_manager.outgoing.lock().await;
                xfers
                    .values()
                    .map(|xstate| xstate.xfer.clone())
                    .collect::<Vec<_>>()
            };

            for xfer in outgoing_transfers {
                service.trigger_peer_outgoing(xfer);
            }

            let incoming_transfers = {
                let xfers = service.state.transfer_manager.incoming.lock().await;
                xfers
                    .values()
                    .map(|xstate| xstate.xfer.clone())
                    .collect::<Vec<_>>()
            };

            for xfer in incoming_transfers {
                service.trigger_peer_incoming(xfer);
            }

            Ok(service)
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

    fn trigger_peer_incoming(&mut self, xfer: Arc<crate::IncomingTransfer>) {
        debug!(self.logger, "trigger_peer_incoming() called: {:?}", xfer);

        let id = xfer.id();

        if let Some(handle) = self.tasks.get(&id) {
            if !handle.is_finished() {
                return;
            } else {
                self.tasks.remove(&id);
            }
        }

        let handle = check::spawn(
            self.state.clone(),
            xfer,
            self.logger.clone(),
            self.waiter.guard(),
            self.stop.clone(),
        );

        self.tasks.insert(id, handle);
    }

    fn trigger_peer_outgoing(&mut self, xfer: Arc<crate::OutgoingTransfer>) {
        debug!(self.logger, "trigger_peer_outgoing() called: {:?}", xfer);

        let id = xfer.id();
        if let Some(handle) = self.tasks.get(&id) {
            if !handle.is_finished() {
                return;
            } else {
                self.tasks.remove(&id);
            }
        }

        let handle = ws::client::spawn(
            self.state.clone(),
            xfer,
            self.logger.clone(),
            self.waiter.guard(),
            self.stop.clone(),
        );

        self.tasks.insert(id, handle);
    }

    pub async fn set_peer_state(&mut self, addr: IpAddr, is_online: bool) {
        {
            if is_online {
                let outgoing_transfers_to_trigger = {
                    let xfers = self.state.transfer_manager.outgoing.lock().await;

                    xfers
                        .values()
                        .filter(|state| {
                            let peer = state.xfer.peer();
                            peer == addr
                        })
                        .map(|state| state.xfer.clone())
                        .collect::<Vec<_>>()
                };

                for xfer in outgoing_transfers_to_trigger {
                    self.trigger_peer_outgoing(xfer);
                }
            }

            if is_online {
                let incoming_transfers_to_trigger = {
                    let xfers = self.state.transfer_manager.incoming.lock().await;

                    xfers
                        .values()
                        .filter(|state| {
                            let peer = state.xfer.peer();
                            peer == addr && is_online
                        })
                        .map(|state| state.xfer.clone())
                        .collect::<Vec<_>>()
                };

                for xfer in incoming_transfers_to_trigger {
                    self.trigger_peer_incoming(xfer);
                }
            }
        }
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
                .expect("Event channel should be open");

            return;
        }

        self.state
            .event_tx
            .send(Event::RequestQueued(xfer.clone()))
            .expect("Could not send a RequestQueued event, channel closed");

        self.trigger_peer_outgoing(xfer);
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
                .start_download(&self.state.storage, file_id, parent_dir, &self.logger)
                .await?;
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
