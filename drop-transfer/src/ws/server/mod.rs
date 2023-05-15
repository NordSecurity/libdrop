mod handler;
mod v2;
mod v3;
mod v4;

use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use drop_auth::Nonce;
use futures::{SinkExt, StreamExt};
use handler::{Downloader, HandlerInit, HandlerLoop, Request};
use hyper::StatusCode;
use slog::{debug, error, info, warn, Logger};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver},
        Mutex,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use super::events::FileEventTx;
use crate::{
    auth,
    error::ResultExt,
    event::DownloadSuccess,
    file,
    manager::{TransferConnection, TransferGuard},
    protocol,
    quarantine::PathExt,
    service::State,
    utils::Hidden,
    ws::Pinger,
    Error, Event, FileId,
};

const MAX_FILENAME_LENGTH: usize = 255;
const MAX_FILE_SUFFIX_LEN: usize = 5; // Assume that the suffix will fit into 5 characters e.g.
                                      // `<filename>(999).<ext>`
const REPORT_PROGRESS_THRESHOLD: u64 = 1024 * 64;

pub enum ServerReq {
    Download { task: Box<FileXferTask> },
    Cancel { file: FileId },
}

pub struct FileXferTask {
    pub file: crate::File,
    pub location: Hidden<PathBuf>,
    pub size: u64,
    pub xfer: crate::Transfer,
}

struct TmpFileState {
    meta: fs::Metadata,
    csum: [u8; 32],
}

pub(crate) fn start(
    addr: IpAddr,
    stop: CancellationToken,
    state: Arc<State>,
    auth: Arc<auth::Context>,
    logger: Logger,
) -> crate::Result<JoinHandle<()>> {
    let nonce_store = Arc::new(Mutex::new(HashMap::new()));

    #[derive(Debug)]
    struct MissingAuth(SocketAddr);
    impl warp::reject::Reject for MissingAuth {}

    #[derive(Debug)]
    struct Unauthrorized;
    impl warp::reject::Reject for Unauthrorized {}

    async fn handle_rejection(
        nonces: &Mutex<HashMap<SocketAddr, Nonce>>,
        err: warp::Rejection,
    ) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        if let Some(MissingAuth(peer)) = err.find() {
            let nonce = Nonce::generate();
            let value = drop_auth::http::WWWAuthenticate::new(nonce);

            nonces.lock().await.insert(*peer, nonce);

            Ok(Box::new(warp::reply::with_header(
                StatusCode::UNAUTHORIZED,
                drop_auth::http::WWWAuthenticate::KEY,
                value.to_string(),
            )))
        } else if let Some(Unauthrorized) = err.find() {
            Ok(Box::new(StatusCode::UNAUTHORIZED))
        } else {
            Err(err)
        }
    }

    let service = {
        let stop = stop.clone();
        let logger = logger.clone();
        let nonces = nonce_store.clone();

        warp::path("drop")
            .and(warp::path::param().and_then(|version: String| async move {
                version
                    .parse::<protocol::Version>()
                    .map_err(|_| warp::reject::not_found())
            }))
            .and(
                warp::filters::addr::remote().then(|peer: Option<SocketAddr>| async move {
                    peer.expect("Transport should use IP addresses")
                }),
            )
            .and(warp::filters::header::optional("authorization"))
            .and_then(
                move |version: protocol::Version, peer: SocketAddr, auth_header: Option<String>| {
                    let nonces = nonces.clone();
                    let auth = auth.clone();

                    async move {
                        // Uncache the peer nonce first
                        let nonce = nonces.lock().await.remove(&peer);

                        match version {
                            protocol::Version::V1 | protocol::Version::V2 => (),
                            protocol::Version::V3 | protocol::Version::V4 => {
                                let auth_header = auth_header
                                    .ok_or_else(|| warp::reject::custom(MissingAuth(peer)))?;

                                let nonce =
                                    nonce.ok_or_else(|| warp::reject::custom(Unauthrorized))?;

                                if !auth.authorize(peer.ip(), &auth_header, &nonce) {
                                    return Err(warp::reject::custom(Unauthrorized));
                                }
                            }
                        };

                        Ok((version, peer))
                    }
                },
            )
            .untuple_one()
            .and(warp::ws())
            .map(
                move |version: protocol::Version, peer: SocketAddr, ws: warp::ws::Ws| {
                    let state = Arc::clone(&state);
                    let stop = stop.clone();
                    let logger = logger.clone();

                    ws.on_upgrade(move |socket| async move {
                        info!(logger, "Client requested protocol version: {}", version);

                        let ctx = RunContext {
                            logger: &logger,
                            state: state.clone(),
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
                            protocol::Version::V3 => {
                                ctx.run(v3::HandlerInit::new(peer.ip(), state, &logger))
                                    .await
                            }
                            protocol::Version::V4 => {
                                ctx.run(v4::HandlerInit::new(peer.ip(), state, &logger))
                                    .await
                            }
                        }
                    })
                },
            )
            .recover(move |err| {
                let nonces = Arc::clone(&nonce_store);
                async move { handle_rejection(&nonces, err).await }
            })
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
    state: Arc<State>,
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

        let job = handle_client(&self.state, self.logger, self.socket, handler, xfer.clone());

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
    let _guard = TransferGuard::new(state.clone(), xfer.id());
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
        let size = file.size();

        Ok(Self {
            file,
            xfer,
            location: Hidden(location),
            size,
        })
    }

    fn move_tmp_to_dst(
        &self,
        tmp_location: &Hidden<PathBuf>,
        logger: &Logger,
    ) -> crate::Result<PathBuf> {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);

        let mut iter = crate::utils::filepath_variants(&self.location.0)?;
        let dst_location = loop {
            let path = iter.next().expect("File paths iterator should never end");

            match opts.open(&path) {
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    continue;
                }
                Err(err) => {
                    error!(logger, "Failed to crate destination file: {err}");
                    return Err(err.into());
                }
                Ok(file) => {
                    drop(file); // Close the file
                    break path;
                }
            }
        };

        fs::rename(&tmp_location.0, &dst_location)?;

        if let Err(err) = dst_location.quarantine() {
            error!(logger, "Failed to quarantine downloaded file: {}", err);
        }

        Ok(dst_location)
    }

    async fn stream_file(
        &mut self,
        logger: &slog::Logger,
        downloader: &mut impl Downloader,
        tmp_location: &Hidden<PathBuf>,
        stream: &mut UnboundedReceiver<Vec<u8>>,
        events: &FileEventTx,
        offset: u64,
    ) -> crate::Result<PathBuf> {
        let mut out_file = match downloader.open(tmp_location).await {
            Ok(out_file) => out_file,
            Err(err) => {
                error!(
                    logger,
                    "Could not create {tmp_location:?} for downloading: {err}"
                );

                return Err(err);
            }
        };

        let consume_file_chunks = async {
            let mut bytes_received = offset;
            let mut last_progress = bytes_received;

            // Announce initial state of the transfer
            downloader.progress(bytes_received).await?;
            events
                .emit(crate::Event::FileDownloadProgress(
                    self.xfer.clone(),
                    self.file.id().clone(),
                    bytes_received,
                ))
                .await;

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
                    downloader.progress(bytes_received).await?;
                    events
                        .emit(crate::Event::FileDownloadProgress(
                            self.xfer.clone(),
                            self.file.id().clone(),
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
                if let Err(ioerr) = fs::remove_file(&tmp_location.0) {
                    error!(
                        logger,
                        "Could not remove temporary file {tmp_location:?} after failed download: \
                         {}",
                        ioerr
                    );
                }

                return Err(err);
            }
        };

        downloader.done(bytes_received).await?;

        let dst = match self.move_tmp_to_dst(tmp_location, logger) {
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
    }

    async fn run(
        mut self,
        state: Arc<State>,
        events: Arc<FileEventTx>,
        mut downloader: impl Downloader,
        mut stream: UnboundedReceiver<Vec<u8>>,
        logger: Logger,
    ) {
        let init_res = match downloader.init(&self).await {
            Ok(init) => init,
            Err(err) => {
                events
                    .emit_force(crate::Event::FileDownloadFailed(
                        self.xfer.clone(),
                        self.file.id().clone(),
                        err,
                    ))
                    .await;
                return;
            }
        };

        match init_res {
            handler::DownloadInit::Stream {
                offset,
                tmp_location,
            } => {
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
                        self.file.id().clone(),
                    ))
                    .await;

                let result = self
                    .stream_file(
                        &logger,
                        &mut downloader,
                        &tmp_location,
                        &mut stream,
                        &events,
                        offset,
                    )
                    .await;

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
                            id: self.file.id().clone(),
                            final_path: Hidden(dst_location.into_boxed_path()),
                        },
                    )),
                    Err(crate::Error::Canceled) => None,
                    Err(err) => {
                        let _ = downloader.error(err.to_string()).await;
                        Some(Event::FileDownloadFailed(
                            self.xfer.clone(),
                            self.file.id().clone(),
                            err,
                        ))
                    }
                };

                if let Some(event) = event {
                    events.stop(event).await;
                }
            }
            handler::DownloadInit::AlreadyDone { destination } => {
                events
                    .emit_force(crate::Event::FileDownloadSuccess(
                        self.xfer.clone(),
                        DownloadSuccess {
                            id: self.file.id().clone(),
                            final_path: Hidden(destination.0.into_boxed_path()),
                        },
                    ))
                    .await;
            }
        }
    }
}

impl TmpFileState {
    // Blocking operation
    fn load(path: &Path) -> io::Result<Self> {
        let file = fs::File::open(path)?;

        let meta = file.metadata()?;
        let csum = file::checksum(&mut io::BufReader::new(file))?;
        Ok(TmpFileState { meta, csum })
    }
}

fn validate_tmp_location_path(tmp_location: &Hidden<PathBuf>) -> crate::Result<()> {
    let char_count = tmp_location
        .file_name()
        .expect("Cannot extract filename")
        .len();

    if char_count > MAX_FILENAME_LENGTH {
        return Err(crate::Error::FilenameTooLong);
    }

    Ok(())
}
