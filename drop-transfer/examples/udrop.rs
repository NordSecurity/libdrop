use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashSet},
    env,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use clap::{arg, command, value_parser, ArgAction, Command};
use drop_auth::{PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};
use drop_config::DropConfig;
use drop_transfer::{auth, Event, File, Service, Transfer};
use slog::{o, Drain, Logger};
use slog_scope::{info, warn};
use tokio::sync::{mpsc, watch, Mutex};
use uuid::Uuid;

const PRIV_KEY: [u8; SECRET_KEY_LENGTH] = [
    164, 70, 230, 247, 55, 28, 255, 147, 128, 74, 83, 50, 181, 222, 212, 18, 178, 162, 242, 102,
    220, 203, 153, 161, 142, 206, 123, 188, 87, 77, 126, 183,
];
const PUB_KEY: [u8; PUBLIC_KEY_LENGTH] = [
    68, 103, 21, 143, 132, 253, 95, 17, 203, 20, 154, 169, 66, 197, 210, 103, 56, 18, 143, 142,
    142, 47, 53, 103, 186, 66, 91, 201, 181, 186, 12, 136,
];

async fn listen(
    service: &Mutex<Service>,
    xfers: watch::Sender<BTreeSet<Uuid>>,
    rx: &mut mpsc::Receiver<Event>,
    out_dir: &Path,
) -> anyhow::Result<()> {
    info!("Awaiting events…");

    let mut active_file_downloads = BTreeMap::new();

    let xfers = &xfers;
    let cancel_xfer = |xfid| async move {
        service
            .lock()
            .await
            .cancel_all(xfid)
            .await
            .context("Failed to cancled transfer")?;

        xfers.send_modify(|xfers| {
            xfers.remove(&xfid);
        });

        anyhow::Ok(())
    };

    while let Some(ev) = rx.recv().await {
        match ev {
            Event::RequestReceived(xfer) => {
                let xfid = xfer.id();
                let files = xfer.files();

                info!("[EVENT] RequestReceived {}: {:?}", xfid, files);

                xfers.send_modify(|xfers| {
                    xfers.insert(xfid);
                });

                let file_set = active_file_downloads
                    .entry(xfid)
                    .or_insert_with(HashSet::new);

                for (file_id, _) in xfer.flat_file_list() {
                    service
                        .lock()
                        .await
                        .download(xfid, file_id.clone(), out_dir)
                        .await
                        .context("Cannot issue download call")?;

                    file_set.insert(file_id);
                }

                if file_set.is_empty() {
                    service
                        .lock()
                        .await
                        .cancel_all(xfid)
                        .await
                        .context("Failed to cancled transfer")?;

                    active_file_downloads.remove(&xfid);
                    xfers.send_modify(|xfers| {
                        xfers.remove(&xfid);
                    });
                }
            }
            Event::FileDownloadStarted(xfer, file) => {
                info!(
                    "[EVENT] [{}] FileDownloadStarted {:?} transfer started",
                    xfer.id(),
                    file,
                );
            }

            Event::FileUploadProgress(xfer, file, byte_count) => {
                info!(
                    "[EVENT] [{}] FileUploadProgress {:?} progress: {}",
                    xfer.id(),
                    file,
                    byte_count,
                );
            }
            Event::FileDownloadSuccess(xfer, info) => {
                let xfid = xfer.id();

                info!(
                    "[EVENT] [{}] FileDownloadSuccess {:?} [Final name: {:?}]",
                    xfid, info.id, info.final_path,
                );

                if let Entry::Occupied(mut occ) = active_file_downloads.entry(xfer.id()) {
                    occ.get_mut().remove(&info.id);
                    if occ.get().is_empty() {
                        cancel_xfer(xfid).await?;
                        occ.remove_entry();
                    }
                }
            }
            Event::FileUploadSuccess(xfer, path) => {
                info!("[EVENT] FileUploadSuccess {}: {:?}", xfer.id(), path,);
            }
            Event::RequestQueued(xfer) => {
                info!("[EVENT] RequestQueued {}: {:?}", xfer.id(), xfer.files(),);

                xfers.send_modify(|xfers| {
                    xfers.insert(xfer.id());
                });
            }
            Event::FileUploadStarted(xfer, file) => {
                info!("[EVENT] FileUploadStarted {}: {:?}", xfer.id(), file,);
            }
            Event::FileDownloadProgress(xfer, file, progress) => {
                info!(
                    "[EVENT] FileDownloadProgress {}: {:?}, progress: {}",
                    xfer.id(),
                    file,
                    progress
                );
            }
            Event::FileUploadCancelled(xfer, file, by_peer) => {
                info!(
                    "[EVENT] FileUploadCancelled {}: {:?}, by_peer: {by_peer}",
                    xfer.id(),
                    file,
                );
            }
            Event::FileDownloadCancelled(xfer, file, by_peer) => {
                let xfid = xfer.id();

                info!(
                    "[EVENT] FileDownloadCancelled {}: {:?}, by_peer: {by_peer}",
                    xfid, file
                );

                if let Entry::Occupied(mut occ) = active_file_downloads.entry(xfer.id()) {
                    occ.get_mut().remove(&file);
                    if occ.get().is_empty() {
                        cancel_xfer(xfid).await?;
                        occ.remove_entry();
                    }
                }
            }
            Event::FileUploadFailed(xfer, file, status) => {
                info!(
                    "[EVENT] FileUploadFailed {}: {:?}, status: {:?}",
                    xfer.id(),
                    file,
                    status
                );
            }
            Event::FileDownloadFailed(xfer, file, status) => {
                let xfid = xfer.id();

                info!(
                    "[EVENT] FileDownloadFailed {}: {:?}, {:?}",
                    xfid, file, status
                );

                if let Entry::Occupied(mut occ) = active_file_downloads.entry(xfer.id()) {
                    occ.get_mut().remove(&file);
                    if occ.get().is_empty() {
                        cancel_xfer(xfid).await?;
                        occ.remove_entry();
                    }
                }
            }
            Event::TransferCanceled(xfer, by_peer) => {
                info!(
                    "[EVENT] TransferCanceled {}, by peer? {}",
                    xfer.id(),
                    by_peer
                );

                active_file_downloads.remove(&xfer.id());
                xfers.send_modify(|xfers| {
                    xfers.remove(&xfer.id());
                });
            }
            Event::TransferFailed(xfer, err) => {
                info!("[EVENT] TransferFailed {}, status: {}", xfer.id(), err);

                active_file_downloads.remove(&xfer.id());
                xfers.send_modify(|xfers| {
                    xfers.remove(&xfer.id());
                });
            }
        }
    }

    Ok(())
}

async fn handle_stop(
    service: &Mutex<Service>,
    mut xfers: watch::Receiver<BTreeSet<Uuid>>,
) -> anyhow::Result<()> {
    tokio::signal::ctrl_c()
        .await
        .context("Failed to handle CTRL+C signal")?;

    loop {
        {
            let set = xfers.borrow();

            if set.is_empty() {
                break;
            }

            let mut service = service.lock().await;
            for &uuid in set.iter() {
                if let Err(err) = service.cancel_all(uuid).await {
                    warn!("Failed to cancel transfer {uuid}: {err:?}");
                }
            }
        }

        xfers
            .changed()
            .await
            .context("Failed to wait for xfers change")?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start = Instant::now();

    let logger = Logger::root(
        slog_async::Async::new(
            slog::LevelFilter::new(
                slog_term::FullFormat::new(slog_term::TermDecorator::new().build())
                    .use_file_location()
                    .use_custom_timestamp(move |writer: &mut dyn Write| {
                        let ts = start.elapsed();

                        let secs = ts.as_secs();
                        let millis = ts.subsec_millis();

                        write!(writer, "{secs:04}.{millis:03}")
                    })
                    .build()
                    .fuse(),
                slog::Level::Trace,
            )
            .fuse(),
        )
        .build()
        .fuse(),
        o!(),
    );

    let _guard = slog_scope::set_global_logger(logger.clone());

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

    let xfer = if let Some(matches) = matches.subcommand_matches("transfer") {
        let addr = matches
            .get_one::<IpAddr>("ADDR")
            .expect("Missing transfer `ADDR` field");

        info!("Sending transfer request to {}", addr);

        let xfer = Transfer::new(
            *addr,
            matches
                .get_many::<String>("FILE")
                .expect("Missing transfer `FILE` field")
                .map(|p| File::from_path(p, None, &config))
                .collect::<Result<Vec<File>, _>>()
                .context("Cannot build transfer from the files provided")?,
            &config,
        )?;

        Some(xfer)
    } else {
        None
    };

    let (tx, mut rx) = mpsc::channel(256);
    let addr = *matches
        .get_one::<IpAddr>("listen")
        .expect("Missing `listen` flag");

    let out_dir = matches
        .get_one::<PathBuf>("output")
        .expect("Missing `output` flag");

    let auth = {
        let pubkey = drop_auth::PublicKey::from_bytes(&PUB_KEY).unwrap();

        auth::Context::new(
            drop_auth::SecretKey::from_bytes(&PRIV_KEY).unwrap(),
            move |_| Some(pubkey),
        )
    };

    let mut service = Service::start(
        addr,
        tx,
        logger,
        config,
        drop_analytics::moose_mock(),
        Arc::new(auth),
    )
    .context("Failed to start service")?;

    if let Some(xfer) = xfer {
        service.send_request(xfer);
    }

    info!("Listening...");

    let service = Mutex::new(service);
    let (xfers_tx, xfers_rx) = watch::channel(BTreeSet::new());

    let task_result = tokio::select! {
        r = handle_stop(&service, xfers_rx) => r,
        r = listen(&service, xfers_tx, &mut rx, out_dir) => r,
    };

    info!("Stopping the service");

    let stop_result = service.into_inner().stop().await.context("Failed to stop");

    // Drain events
    while rx.recv().await.is_some() {}

    task_result?;
    stop_result?;

    Ok(())
}
