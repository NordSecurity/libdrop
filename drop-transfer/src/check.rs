use std::{net::SocketAddr, ops::ControlFlow, sync::Arc, time::Duration};

use hyper::StatusCode;
use slog::{debug, info, warn, Logger};
use tokio_util::sync::CancellationToken;

use crate::{protocol, service::State, tasks::AliveGuard, IncomingTransfer, Transfer};

pub(crate) fn spawn(
    mut refresh_trigger: tokio::sync::watch::Receiver<()>,
    state: Arc<State>,
    xfer: Arc<IncomingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();

    tokio::spawn(async move {
        let _guard = guard;

        let task = async {
            loop {
                let cf = run(&state, &xfer, &logger).await;

                if cf.is_break() {
                    info!(logger, "Transfer {} is gone. Clearing", xfer.id());

                    match state.transfer_manager.incoming_remove(xfer.id()).await {
                        Err(err) => {
                            warn!(logger, "Failed to clear incoming transfer: {err:?}");
                        }
                        Ok(false) => state
                            .emit_event(crate::Event::IncomingTransferCanceled(xfer.clone(), true)),
                        _ => (),
                    }

                    break;
                }

                let _ = refresh_trigger.changed().await;
            }
        };

        tokio::select! {
            biased;

            _ = stop.cancelled() => {
                debug!(&logger, "stop checking job for: {}", id);
            },
            _ = task => ()
        }
    });
}

async fn run(state: &State, xfer: &Arc<IncomingTransfer>, logger: &Logger) -> ControlFlow<()> {
    for delay in std::iter::once(Duration::from_secs(0)).chain(drop_config::RETRY_INTERVALS) {
        debug!(
            logger,
            "Incoming transfer job started for {}, will sleep for {:?}",
            xfer.id(),
            delay
        );
        tokio::time::sleep(delay).await;

        if !state.transfer_manager.is_incoming_alive(xfer.id()).await {
            return ControlFlow::Break(());
        }

        make_request(state, xfer, logger).await?;
    }

    ControlFlow::Continue(())
}

async fn make_request(state: &State, xfer: &IncomingTransfer, logger: &Logger) -> ControlFlow<()> {
    let remote = SocketAddr::new(xfer.peer(), drop_config::PORT);

    let mut connector = hyper::client::HttpConnector::new();
    connector.set_local_address(Some(state.addr));

    let client = hyper::Client::builder().build::<_, hyper::Body>(connector);

    let versions_to_try = [protocol::Version::V5];

    for version in versions_to_try {
        let url: hyper::Uri = format!("http://{remote}/drop/{version}/check/{}", xfer.id())
            .parse()
            .expect("URL should be valid");

        debug!(logger, "Making HTTP request: {url}");

        let mut response = client.get(url.clone()).await;

        match &response {
            Ok(resp) if resp.status() == StatusCode::UNAUTHORIZED => {
                debug!(logger, "Creating 'authorization' header");

                match state
                    .auth
                    .create_clients_auth_header(resp, xfer.peer(), false)
                {
                    Ok((key, value)) => {
                        debug!(logger, "Building 'authorization' request");

                        let req = hyper::Request::get(url)
                            .header(key, value)
                            .body(hyper::Body::empty())
                            .expect("Creating request should not fail");

                        response = client.request(req).await;
                    }
                    Err(err) => warn!(logger, "Failed to extract authentication header: {err:?}"),
                }
            }
            _ => (),
        }

        match response {
            Ok(resp) => {
                let status = resp.status();

                if status == StatusCode::OK {
                    break;
                } else if status == StatusCode::GONE {
                    return ControlFlow::Break(());
                } else {
                    debug!(
                        logger,
                        "Check returned {status}, trying again with lower version"
                    );
                }
            }
            Err(err) => {
                debug!(
                    logger,
                    "Failed to check if transfer {} is alive: {err:?}",
                    xfer.id()
                );
                break;
            }
        }
    }

    ControlFlow::Continue(())
}
