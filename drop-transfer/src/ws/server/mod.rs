mod handler;
mod v2;

use std::{
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime},
};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use handler::{FeedbackReport, HandlerInit, HandlerLoop, Request};
use sha1::{Digest, Sha1};
use slog::{debug, error, info, warn, Logger};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use super::events::FileEventTx;
use crate::{
    error::ResultExt,
    event::DownloadSuccess,
    file::{self, FileId},
    manager::{TransferConnection, TransferGuard},
    protocol,
    quarantine::PathExt,
    service::State,
    utils::Hidden,
    ws::Pinger,
    Error, Event,
};

const MAX_FILENAME_LENGTH: usize = 255;
const REPORT_PROGRESS_THRESHOLD: u64 = 1024 * 64;

pub enum ServerReq {
    Download {
        file: FileId,
        task: Box<FileXferTask>,
    },
    Cancel {
        file: FileId,
    },
}

pub struct FileXferTask {
    pub file: crate::File,
    pub location: Hidden<PathBuf>,
    pub filename: String,
    pub tmp_location: Hidden<PathBuf>,
    pub size: u64,
    pub xfer: crate::Transfer,
}

// TODO(msz): remove unused
#[allow(unused)]
struct TmpFileState {
    meta: fs::Metadata,
    csum: [u8; 32],
}

pub(crate) fn start(
    addr: IpAddr,
    stop: CancellationToken,
    state: Arc<State>,
    logger: Logger,
) -> crate::Result<JoinHandle<()>> {
    let service = {
        let stop = stop.clone();
        let logger = logger.clone();

        warp::path("drop")
            .and(warp::path::param().and_then(|version: String| async move {
                version
                    .parse::<protocol::Version>()
                    .map_err(|_| warp::reject::not_found())
            }))
            .and(warp::ws())
            .and(warp::filters::addr::remote())
            .map(
                move |version: protocol::Version, ws: warp::ws::Ws, peer: Option<SocketAddr>| {
                    let state = Arc::clone(&state);
                    let peer = peer.expect("Transport should use IP addresses");
                    let stop = stop.clone();
                    let logger = logger.clone();

                    ws.on_upgrade(move |socket| async move {
                        info!(logger, "Client requested protocol version: {}", version);

                        let ctx = RunContext {
                            logger: &logger,
                            state: &state,
                            socket,
                            stop: &stop,
                        };

                        match version {
                            protocol::Version::V1 => {
                                ctx.run(v2::HandlerInit::<false>::new(peer.ip(), &state, &logger))
                                    .await
                            }
                            protocol::Version::V2 => {
                                ctx.run(v2::HandlerInit::<true>::new(peer.ip(), &state, &logger))
                                    .await
                            }
                            protocol::Version::V3 => unimplemented!(),
                        }
                    })
                },
            )
    };

    let future = match warp::serve(service)
        .try_bind_with_graceful_shutdown((addr, drop_config::PORT), async move {
            stop.cancelled().await
        }) {
        Ok((_, future)) => future,
        Err(err) => {
            // Check if this is IO error about address already in use
            if let Some(ioerr) = std::error::Error::source(&err)
                .and_then(|src| src.downcast_ref::<hyper::Error>())
                .and_then(std::error::Error::source)
                .and_then(|src| src.downcast_ref::<io::Error>())
            {
                if ioerr.kind() == io::ErrorKind::AddrInUse {
                    error!(
                        logger,
                        "Found that the address {}:{} is already used, while trying to bind the \
                         WS server: {}",
                        addr,
                        drop_config::PORT,
                        ioerr
                    );
                    return Err(Error::AddrInUse);
                }
            }

            return Err(err.into());
        }
    };

    let task = tokio::spawn(async move {
        future.await;
        debug!(logger, "WS server stopped");
    });

    Ok(task)
}

struct RunContext<'a> {
    logger: &'a slog::Logger,
    state: &'a Arc<State>,
    stop: &'a CancellationToken,
    socket: WebSocket,
}

impl RunContext<'_> {
    async fn run(mut self, mut handler: impl HandlerInit) {
        let recv_task = handler.recv_req(&mut self.socket);

        let xfer = tokio::select! {
            biased;

            _ = self.stop.cancelled() => {
                debug!(self.logger, "Stopping client request on shutdown");
                return;
            },
            r = recv_task => {
                match r {
                    Ok(xfer) => xfer,
                    Err(err) => {
                        error!(self.logger, "Failed to initiate transfer: {:?}", err);
                        return;
                    }
                }
            },
        };

        let xfer = match xfer.parse() {
            Ok(xfer) => {
                debug!(self.logger, "RunContext::run() called with {:?}", xfer);
                xfer
            }
            Err(err) => {
                if let Err(err) = handler.on_error(&mut self.socket, err).await {
                    error!(
                        self.logger,
                        "Failed to close connection on invalid request: {:?}", err
                    );
                }

                return;
            }
        };

        let stop_job = async {
            // Stop the download job
            info!(self.logger, "Aborting transfer download");

            self.state
                .event_tx
                .send(Event::TransferFailed(xfer.clone(), Error::Canceled))
                .await
                .expect("Failed to send TransferFailed event");
        };

        let job = handle_client(self.state, self.logger, self.socket, handler, xfer.clone());

        tokio::select! {
            biased;

            _ = self.stop.cancelled() => {
                stop_job.await;
            },
            _ = job => (),
        };
    }
}

async fn handle_client(
    state: &Arc<State>,
    logger: &slog::Logger,
    mut socket: WebSocket,
    mut hander: impl handler::HandlerInit,
    xfer: crate::Transfer,
) {
    let _guard = TransferGuard::new(state, xfer.id());
    let (req_send, mut req_rx) = mpsc::unbounded_channel();

    {
        if let Err(err) = state
            .transfer_manager
            .lock()
            .await
            .insert_transfer(xfer.clone(), TransferConnection::Server(req_send))
        {
            error!(logger, "Failed to insert a new trasfer: {}", err);

            let _ = hander
                .on_error(
                    &mut socket,
                    anyhow::anyhow!("Failed to init transfer: {err}"),
                )
                .await;
            let _ = socket.close().await;

            return;
        } else {
            state
                .event_tx
                .send(Event::RequestReceived(xfer.clone()))
                .await
                .expect("Failed to notify receiving peer!");
        }
    }

    let (send_tx, mut send_rx) = mpsc::channel(2);
    let mut ping = hander.pinger();
    let mut handler = hander.upgrade(send_tx, xfer);

    let task = async {
        loop {
            tokio::select! {
                biased;

                // API request
                req = req_rx.recv() => {
                    if let Some(req) = req {
                        handler.on_req(&mut socket, req).await?;
                    } else {
                        debug!(logger, "Stoppping server connection gracefuly");
                        handler.on_close(false).await;
                        break;
                    }
                },
                // Message received
                recv = super::utils::recv(&mut socket, handler.recv_timeout()) => {
                    match recv? {
                        Some(msg) => {
                            if handler.on_recv(&mut socket, msg).await?.is_break() {
                                break;
                            }
                        },
                        None => break,
                    };
                },
                // Message to send down the wire
                msg = send_rx.recv() => {
                    let msg = msg.expect("Handler channel should always be open");
                    socket.send(msg).await?;
                },
                _ = ping.tick() => {
                    socket.send(Message::ping(Vec::new())).await.context("Failed to send PING message")?;
                }
            };
        }
        anyhow::Ok(())
    };

    let result = task.await;
    handler.on_stop().await;

    if let Err(err) = result {
        handler.finalize_failure(err).await;
    } else {
        let task = async {
            socket.send(Message::close()).await?;
            // Drain messages
            while socket.next().await.transpose()?.is_some() {}

            socket.close().await
        };

        if let Err(err) = task.await {
            warn!(
                logger,
                "Failed to gracefully close the client connection: {}", err
            );
        } else {
            debug!(logger, "WS client disconnected");
        }
    }
}

impl FileXferTask {
    pub fn new(file: crate::File, xfer: crate::Transfer, location: PathBuf) -> crate::Result<Self> {
        let filename = location
            .file_stem()
            .ok_or(crate::Error::BadFile)?
            .to_string_lossy()
            .to_string();
        let mut suffix = Sha1::new();

        suffix.update(xfer.id().as_bytes());
        if let Ok(time) = SystemTime::now().elapsed() {
            suffix.update(time.as_nanos().to_ne_bytes());
        }

        let suffix: String = suffix
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let tmp_location: PathBuf = format!(
            "{}.dropdl-{}",
            location.display(),
            suffix.get(..8).unwrap_or(&suffix),
        )
        .into();
        let size = file.size().ok_or(crate::Error::DirectoryNotExpected)?;
        let char_count = tmp_location
            .file_name()
            .expect("Cannot extract filename")
            .len();
        if char_count > MAX_FILENAME_LENGTH {
            return Err(crate::Error::FilenameTooLong);
        }
        Ok(Self {
            file,
            xfer,
            location: Hidden(location),
            filename,
            tmp_location: Hidden(tmp_location),
            size,
        })
    }

    // Blocking operation
    // TODO(msz): remove unused
    #[allow(unused)]
    fn tmp_state(&self) -> io::Result<TmpFileState> {
        let file = fs::File::open(&self.tmp_location.0)?;

        let meta = file.metadata()?;
        let csum = file::checksum(&mut io::BufReader::new(file))?;
        Ok(TmpFileState { meta, csum })
    }

    fn move_tmp_to_dst(&self, logger: &Logger) -> crate::Result<PathBuf> {
        let dst_location = crate::utils::map_path_if_exists(&self.location.0)?;

        fs::rename(&self.tmp_location.0, &dst_location)?;

        if let Err(err) = dst_location.quarantine() {
            error!(logger, "Failed to quarantine downloaded file: {}", err);
        }

        Ok(dst_location)
    }

    async fn run(
        self,
        state: Arc<State>,
        events: Arc<FileEventTx>,
        mut feedback: impl FeedbackReport,
        mut stream: UnboundedReceiver<Vec<u8>>,
        file_id: FileId,
        logger: Logger,
    ) {
        let transfer_time = Instant::now();

        state.moose.service_quality_transfer_file(
            Ok(()),
            drop_analytics::Phase::Start,
            self.xfer.id().to_string(),
            0,
            self.file.info(),
        );

        events
            .start(crate::Event::FileDownloadStarted(
                self.xfer.clone(),
                file_id.clone(),
            ))
            .await;

        let receive_file = async {
            let mut out_file = match fs::File::create(&self.tmp_location.0) {
                Ok(out_file) => out_file,
                Err(err) => {
                    error!(
                        logger,
                        "Could not create {:?} for downloading: {}", self.tmp_location, err
                    );

                    return Err(crate::Error::from(err));
                }
            };

            let consume_file_chunks = async {
                let mut bytes_received = 0;
                let mut last_progress = 0;

                while bytes_received < self.size {
                    let chunk = stream.recv().await.ok_or(crate::Error::Canceled)?;

                    let chunk_size = chunk.len();
                    if chunk_size as u64 + bytes_received > self.size {
                        return Err(crate::Error::MismatchedSize);
                    }

                    out_file.write_all(&chunk)?;

                    bytes_received += chunk_size as u64;

                    if last_progress + REPORT_PROGRESS_THRESHOLD <= bytes_received {
                        // send progress to the caller
                        feedback.progress(bytes_received).await?;

                        events
                            .emit(crate::Event::FileDownloadProgress(
                                self.xfer.clone(),
                                file_id.clone(),
                                bytes_received,
                            ))
                            .await;

                        last_progress = bytes_received;
                    }
                }

                if bytes_received > self.size {
                    Err(crate::Error::UnexpectedData)
                } else {
                    Ok(bytes_received)
                }
            };

            let bytes_received = match consume_file_chunks.await {
                Ok(br) => br,
                Err(err) => {
                    if let Err(ioerr) = fs::remove_file(&self.tmp_location.0) {
                        error!(
                            logger,
                            "Could not remove temporary file {:?} after failed download: {}",
                            self.tmp_location,
                            ioerr
                        );
                    }

                    return Err(err);
                }
            };

            feedback.done(bytes_received).await?;

            let dst = match self.move_tmp_to_dst(&logger) {
                Ok(dst) => dst,
                Err(err) => {
                    error!(
                        logger,
                        "Could not rename temporary file {:?} after downloading: {}",
                        self.location,
                        err
                    );
                    return Err(err);
                }
            };

            Ok(dst)
        };

        let result = receive_file.await;

        state.moose.service_quality_transfer_file(
            result.to_status(),
            drop_analytics::Phase::End,
            self.xfer.id().to_string(),
            transfer_time.elapsed().as_millis() as i32,
            self.file.info(),
        );

        let event = match result {
            Ok(dst_location) => Some(Event::FileDownloadSuccess(
                self.xfer.clone(),
                DownloadSuccess {
                    id: file_id,
                    final_path: Hidden(dst_location.into_boxed_path()),
                },
            )),
            Err(crate::Error::Canceled) => None,
            Err(err) => {
                let _ = feedback.error(err.to_string()).await;
                Some(Event::FileDownloadFailed(self.xfer.clone(), file_id, err))
            }
        };

        if let Some(event) = event {
            events.stop(event).await;
        }
    }
}

impl Drop for FileXferTask {
    fn drop(&mut self) {
        let _ = fs::remove_file(&*self.tmp_location);
    }
}
