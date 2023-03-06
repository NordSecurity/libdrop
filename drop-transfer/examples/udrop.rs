use std::{
    env,
    net::{AddrParseError, IpAddr},
    path::Path,
    sync::Arc,
};

use clap::{arg, command, value_parser, ArgAction, Command, Result};
use drop_config::DropConfig;
use drop_transfer::{Error as TransferError, Event, File, Service, Transfer};
use lazy_static::lazy_static;
use slog::{info, o, Drain, Logger};
use tokio::sync::{mpsc, Mutex};

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Bad address")]
    BadAddress(#[from] AddrParseError),
    #[error(transparent)]
    Transfer(#[from] TransferError),
}

lazy_static! {
    static ref LOGGER: slog::Logger = Logger::root(
        slog_async::Async::new(
            slog::LevelFilter::new(
                slog_term::FullFormat::new(slog_term::TermDecorator::new().build())
                    .use_file_location()
                    .build()
                    .fuse(),
                slog::Level::Trace
            )
            .fuse()
        )
        .build()
        .fuse(),
        o!()
    );
}

async fn listen(service: Arc<Mutex<Service>>, mut rx: mpsc::Receiver<Event>, _is_sender: bool) {
    info!(LOGGER, "Awaiting events…");

    while let Some(ev) = rx.recv().await {
        let mut svc = service.lock().await;

        match ev {
            Event::RequestReceived(xfer) => {
                let xfid = xfer.id();
                let files = xfer.files();

                info!(LOGGER, "[EVENT] RequestReceived {}: {:?}", xfid, files);

                let out_dir = &home::home_dir().unwrap();

                for file in files.values() {
                    if file.is_dir() {
                        let children: Vec<&File> = file.iter().filter(|c| !c.is_dir()).collect();

                        for child in children {
                            let path = child.path();

                            info!(LOGGER, "Downloading {:?}", path);

                            svc.download(xfid, path, out_dir).await.unwrap();

                            info!(LOGGER, "{:?} finished downloading", path);
                        }
                    } else {
                        let path = file.path();

                        info!(LOGGER, "Downloading {:?}", path);

                        svc.download(xfid, path, out_dir).await.unwrap();
                    }
                }
            }
            Event::FileDownloadStarted(xfer, file) => {
                info!(
                    LOGGER,
                    "[EVENT] [{}] FileDownloadStarted {:?} transfer started",
                    xfer.id(),
                    file,
                );
            }

            Event::FileUploadProgress(xfer, file, byte_count) => {
                info!(
                    LOGGER,
                    "[EVENT] [{}] FileUploadProgress {:?} progress: {}",
                    xfer.id(),
                    file,
                    byte_count,
                );
            }
            Event::FileDownloadSuccess(xfer, info) => {
                info!(
                    LOGGER,
                    "[EVENT] [{}] FileDownloadSuccess {:?} [Final name: {:?}]",
                    xfer.id(),
                    info.id,
                    info.final_path,
                );
            }
            Event::FileUploadSuccess(xfer, path) => {
                info!(
                    LOGGER,
                    "[EVENT] FileUploadSuccess {}: {:?}",
                    xfer.id(),
                    path,
                );
            }
            Event::RequestQueued(xfer) => {
                info!(
                    LOGGER,
                    "[EVENT] RequestQueued {}: {:?}",
                    xfer.id(),
                    xfer.files(),
                );
            }
            Event::FileUploadStarted(xfer, file) => {
                info!(
                    LOGGER,
                    "[EVENT] FileUploadStarted {}: {:?}",
                    xfer.id(),
                    file,
                );
            }
            Event::FileDownloadProgress(xfer, file, progress) => {
                info!(
                    LOGGER,
                    "[EVENT] FileDownloadProgress {}: {:?}, progress: {}",
                    xfer.id(),
                    file,
                    progress
                );
            }
            Event::FileUploadCancelled(xfer, file) => {
                info!(
                    LOGGER,
                    "[EVENT] FileUploadCancelled {}: {:?}",
                    xfer.id(),
                    file,
                );
            }
            Event::FileDownloadCancelled(xfer, file) => {
                info!(
                    LOGGER,
                    "[EVENT] FileDownloadCancelled {}: {:?}",
                    xfer.id(),
                    file
                );
            }
            Event::FileUploadFailed(xfer, file, status) => {
                info!(
                    LOGGER,
                    "[EVENT] FileUploadFailed {}: {:?}, status: {:?}",
                    xfer.id(),
                    file,
                    status
                );
            }
            Event::FileDownloadFailed(xfer, file, status) => {
                info!(
                    LOGGER,
                    "[EVENT] FileDownloadFailed {}: {:?}, {:?}",
                    xfer.id(),
                    file,
                    status
                );
            }
            Event::TransferCanceled(xfer, by_peer) => {
                info!(
                    LOGGER,
                    "[EVENT] TransferCanceled {}, by peer? {}",
                    xfer.id(),
                    by_peer
                );
            }
            Event::TransferFailed(xfer, err) => {
                info!(
                    LOGGER,
                    "[EVENT] TransferFailed {}, status: {}",
                    xfer.id(),
                    err
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let matches = command!()
        .arg(
            arg!(-l --listen <ADDR> "Listen address")
                .required(true)
                .value_parser(value_parser!(IpAddr)),
        )
        .subcommand(
            Command::new("transfer")
                .arg(
                    arg!([ADDR])
                        .required(true)
                        .value_parser(value_parser!(IpAddr)),
                )
                .arg(arg!([FILE] ...).action(ArgAction::Append).required(true)),
        )
        .get_matches();

    let config = DropConfig::default();

    let (tx, rx) = mpsc::channel(256);
    let addr = *matches.get_one::<IpAddr>("listen").unwrap();
    let svc = Arc::new(Mutex::new(Service::start(
        addr,
        tx,
        LOGGER.clone(),
        config,
        drop_analytics::moose_mock(),
    )?));
    let listener_svc = svc.clone();

    info!(LOGGER, "Spawning listener task…");

    let is_sender = matches.subcommand_matches("transfer").is_some();

    let service_task = tokio::spawn(async move {
        info!(LOGGER, "Listening...");
        listen(listener_svc, rx, is_sender).await;
    });

    if let Some(matches) = matches.subcommand_matches("transfer") {
        let addr = matches.get_one::<IpAddr>("ADDR").unwrap();

        info!(LOGGER, "Sending transfer request to {}", addr);

        let mut inst = svc.lock().await;
        let xfer = Transfer::new(
            *addr,
            matches
                .get_many::<String>("FILE")
                .unwrap()
                .map(|p| File::from_path(Path::new(p), None, &config))
                .collect::<Result<Vec<File>, drop_transfer::Error>>()?,
            &config,
        )?;

        inst.send_request(xfer);
    }

    service_task.await.expect("Failed to join service task");

    if let Ok(service) = Arc::try_unwrap(svc) {
        info!(LOGGER, "Stopping the service");
        service
            .into_inner()
            .stop()
            .await
            .expect("Service stop should not fail");
    }

    Ok(())
}
