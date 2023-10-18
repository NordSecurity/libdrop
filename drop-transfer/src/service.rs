use std::{
    fs,
    net::IpAddr,
    path::{Component, Path},
    sync::Arc,
    time::{Instant, SystemTime},
};

use drop_analytics::{InitEventData, Moose, TransferStateEventData};
use drop_config::DropConfig;
use drop_core::Status;
use drop_storage::Storage;
use slog::{debug, trace, Logger};
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth,
    error::ResultExt,
    manager,
    tasks::AliveWaiter,
    transfer::Transfer,
    ws::{self, FileEventTxFactory},
    Error, Event, FileId, TransferManager,
};

pub(super) struct State {
    pub(super) event_tx: mpsc::UnboundedSender<(Event, SystemTime)>,
    pub(super) transfer_manager: TransferManager,
    pub(crate) moose: Arc<dyn Moose>,
    pub(crate) auth: Arc<auth::Context>,
    pub(crate) config: Arc<DropConfig>,
    pub(crate) storage: Arc<Storage>,
    pub(crate) throttle: Arc<Semaphore>,
    pub(crate) addr: IpAddr,
    #[cfg(unix)]
    pub fdresolv: Option<Arc<crate::file::FdResolver>>,
}

impl State {
    pub fn emit_event(&self, event: crate::Event) {
        self.event_tx
            .send((event, SystemTime::now()))
            .expect("Failed to emit Event");
    }
}

pub struct Service {
    pub(super) state: Arc<State>,
    stop: CancellationToken,
    waiter: AliveWaiter,
    pub(super) logger: Logger,

    refresh_trigger: tokio::sync::watch::Sender<()>,
}

impl Service {
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        addr: IpAddr,
        storage: Arc<Storage>,
        event_tx: mpsc::UnboundedSender<(Event, SystemTime)>,
        logger: Logger,
        config: Arc<DropConfig>,
        moose: Arc<dyn Moose>,
        auth: Arc<auth::Context>,
        init_time: Instant,
        #[cfg(unix)] fdresolv: Option<Arc<crate::FdResolver>>,
    ) -> Result<Self, Error> {
        let task = async {
            let state = Arc::new(State {
                throttle: Arc::new(Semaphore::new(drop_config::MAX_UPLOADS_IN_FLIGHT)), /* TODO: max uploads of 4 files per all libdrop is too restrictive, workout a better plan, like configurable through config */
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

            let refresh_trigger = tokio::sync::watch::channel(()).0;
            ws::server::spawn(
                refresh_trigger.subscribe(),
                state.clone(),
                logger.clone(),
                stop.clone(),
                guard.clone(),
            )?;

            manager::resume(&refresh_trigger.subscribe(), &state, &logger, &guard, &stop).await;

            Ok(Self {
                refresh_trigger,
                state,
                stop,
                waiter,
                logger: logger.clone(),
            })
        };

        let res = task.await;

        moose.event_init(InitEventData {
            init_duration: init_time.elapsed().as_millis() as i32,
            result: res.to_moose_status(),
        });

        res
    }

    pub async fn stop(self) {
        self.stop.cancel();
        self.waiter.wait_for_all().await;
    }

    pub fn storage(&self) -> &Storage {
        &self.state.storage
    }

    pub fn network_refresh(&mut self) {
        if self.refresh_trigger.send(()).is_ok() {
            trace!(self.logger, "Refresh trigger sent");
        }
    }

    pub async fn send_request(&mut self, xfer: crate::OutgoingTransfer) {
        let xfer = Arc::new(xfer);

        self.state.moose.event_transfer_intent(xfer.info());

        if let Err(err) = self
            .state
            .transfer_manager
            .insert_outgoing(xfer.clone())
            .await
        {
            self.state
                .moose
                .event_transfer_state(TransferStateEventData {
                    transfer_id: xfer.id().to_string(),
                    result: i32::from(&err),
                    protocol_version: 0,
                });

            self.state
                .emit_event(Event::OutgoingTransferFailed(xfer.clone(), err, true));

            return;
        }

        self.state.emit_event(Event::RequestQueued(xfer.clone()));

        let subscriber = self.refresh_trigger.subscribe();

        ws::client::spawn(
            subscriber,
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
                .start_download(&self.state.storage, file_id, parent_dir, &self.logger)
                .await?;
        }

        Ok(())
    }

    /// Reject a single file in a transfer. After rejection the file can no
    /// longer be transferred
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
                    // Try to delete temporary files
                    let tmp_bases = self
                        .state
                        .storage
                        .fetch_base_dirs_for_file(transfer_id, file.as_ref())
                        .await;

                    super::ws::server::remove_temp_files(
                        &self.logger,
                        transfer_id,
                        tmp_bases.into_iter().map(|base| (base, &file)),
                    );

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
                        .emit_event(crate::Event::OutgoingTransferCanceled(res.xfer, false));

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
                        .emit_event(crate::Event::IncomingTransferCanceled(res.xfer, false));

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
