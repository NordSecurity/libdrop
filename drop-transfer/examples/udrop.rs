use std::{
    env,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::{arg, command, value_parser, ArgAction, Command, Result};
use drop_config::DropConfig;
use drop_transfer::{Event, File, Service, Transfer};
use lazy_static::lazy_static;
use slog::{info, o, Drain, Logger};
use tokio::sync::mpsc;

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

async fn listen(
    service: &mut Service,
    mut rx: mpsc::Receiver<Event>,
    out_dir: &Path,
) -> anyhow::Result<()> {
    info!(LOGGER, "Awaiting events…");

    while let Some(ev) = rx.recv().await {
        match ev {
            Event::RequestReceived(xfer) => {
                let xfid = xfer.id();
                let files = xfer.files();

                info!(LOGGER, "[EVENT] RequestReceived {}: {:?}", xfid, files);

                for file in files.values() {
                    if file.is_dir() {
                        let children: Vec<&File> = file.iter().filter(|c| !c.is_dir()).collect();

                        for child in children {
                            let path = child.path();

                            info!(LOGGER, "Downloading {:?}", path);

                            service
                                .download(xfid, path, out_dir)
                                .await
                                .context("Cannot issue download call")?;

                            info!(LOGGER, "{:?} finished downloading", path);
                        }
                    } else {
                        let path = file.path();

                        info!(LOGGER, "Downloading {:?}", path);

                        service
                            .download(xfid, path, out_dir)
                            .await
                            .context("Cannot issue download call")?;
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

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = command!()
        .arg(
            arg!(-l --listen <ADDR> "Listen address")
                .required(true)
                .value_parser(value_parser!(IpAddr)),
        )
        .arg(
            arg!(-o --output <DIR> "Download directory")
                .required(true)
                .value_parser(value_parser!(PathBuf)),
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

    let config = DropConfig {
        req_connection_timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let (tx, rx) = mpsc::channel(256);

    let addr = *matches
        .get_one::<IpAddr>("listen")
        .expect("Missing `listen` flag");

    let out_dir = matches
        .get_one::<PathBuf>("output")
        .expect("Missing `output` flag");

    info!(LOGGER, "Spawning listener task…");

    let mut service = Service::start(
        addr,
        tx,
        LOGGER.clone(),
        config,
        drop_analytics::moose_mock(),
    )
    .context("Failed to start service")?;

    let task = async {
        if let Some(matches) = matches.subcommand_matches("transfer") {
            let addr = matches
                .get_one::<IpAddr>("ADDR")
                .expect("Missing transfer `ADDR` field");

            info!(LOGGER, "Sending transfer request to {}", addr);

            let xfer = Transfer::new(
                *addr,
                matches
                    .get_many::<String>("FILE")
                    .expect("Missing transfer `FILE` field")
                    .map(|p| File::from_path(Path::new(p), None, &config))
                    .collect::<Result<Vec<File>, _>>()
                    .context("Cannot build transfer from the files provided")?,
                &config,
            )?;

            service.send_request(xfer);
        }

        info!(LOGGER, "Listening...");
        listen(&mut service, rx, out_dir).await
    };

    let task_result = task.await;
    info!(LOGGER, "Stopping the service");

    let stop_result = service.stop().await.context("Failed to stop");
    task_result?;
    stop_result?;

    Ok(())
}
