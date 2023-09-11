use std::{net::SocketAddr, ops::ControlFlow, sync::Arc};

use hyper::StatusCode;
use slog::{debug, info, warn, Logger};
use tokio_util::sync::CancellationToken;

use crate::{auth, protocol, service::State, tasks::AliveGuard, IncomingTransfer, Transfer};

pub(crate) fn spawn(
    state: Arc<State>,
    xfer: Arc<IncomingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();
    let job = run(state, xfer, logger.clone());

    tokio::spawn(async move {
        let _guard = guard;

        tokio::select! {
            biased;

            _ = stop.cancelled() => {
                debug!(logger, "Check job stop for transfer: {id}");
            },
            _ = job => (),
        }
    });
}

async fn run(state: Arc<State>, xfer: Arc<IncomingTransfer>, logger: Logger) {
    loop {
        tokio::time::sleep(drop_config::ALIVE_CHECK_INTERVAL).await;

        if !state.transfer_manager.is_incoming_alive(xfer.id()).await {
            break;
        }

        if make_request(&state.auth, &xfer, &logger).await.is_break() {
            break;
        }
    }

    info!(logger, "Transfer {} is gone. Clearing", xfer.id());
    if let Err(err) = state.transfer_manager.incoming_remove(xfer.id()).await {
        warn!(logger, "Failed to clear incoming transfer: {err:?}");
    }
}

async fn make_request(
    auth: &auth::Context,
    xfer: &IncomingTransfer,
    logger: &Logger,
) -> ControlFlow<()> {
    let addr = SocketAddr::new(xfer.peer(), drop_config::PORT);
    let client = hyper::Client::new();
    let versions_to_try = [protocol::Version::V5];

    for version in versions_to_try {
        let url: hyper::Uri = format!("http://{addr}/drop/{version}/check/{}", xfer.id())
            .parse()
            .expect("URL should be valid");

        debug!(logger, "Making HTTP request: {url}");

        let mut response = client.get(url.clone()).await;

        match &response {
            Ok(resp) if resp.status() == StatusCode::UNAUTHORIZED => {
                debug!(logger, "Creating 'authorization' header");

                match auth.create_authorization_header(resp, xfer.peer()) {
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
