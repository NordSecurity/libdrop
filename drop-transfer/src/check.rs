use std::{
    net::{IpAddr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
};

use hyper::StatusCode;
use slog::{debug, info, warn, Logger};
use tokio_util::sync::CancellationToken;

use crate::{auth, protocol, service::State, tasks::AliveGuard, utils, IncomingTransfer, Transfer};

#[derive(thiserror::Error, Debug)]
enum RequestError {
    #[error("{0}")]
    General(#[from] anyhow::Error),
    #[error("Unexpected HTTP response: {0}")]
    UnexpectedResponse(StatusCode),
}

pub(crate) fn spawn(
    refresh_trigger: tokio::sync::watch::Receiver<()>,
    state: Arc<State>,
    xfer: Arc<IncomingTransfer>,
    logger: Logger,
    guard: AliveGuard,
    stop: CancellationToken,
) {
    let id = xfer.id();

    tokio::spawn(async move {
        let _guard = guard;
        let mut backoff = utils::RetryTrigger::new(refresh_trigger);

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

                backoff.backoff().await;
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
    debug!(logger, "Incoming transfer job started for {}", xfer.id(),);

    if !state.transfer_manager.is_incoming_alive(xfer.id()).await {
        return ControlFlow::Break(());
    }

    ask_server(state, xfer, logger).await?;
    ControlFlow::Continue(())
}

async fn ask_server(state: &State, xfer: &IncomingTransfer, logger: &Logger) -> ControlFlow<()> {
    let mut connector = hyper::client::HttpConnector::new();
    connector.set_local_address(Some(state.addr));

    let client = hyper::Client::builder().build::<_, hyper::Body>(connector);

    let versions_to_try = [protocol::Version::V6, protocol::Version::V5];

    for version in versions_to_try {
        match make_request(
            &client,
            xfer.peer(),
            version,
            xfer.id(),
            &state.auth,
            logger,
        )
        .await
        {
            Ok(false) => return ControlFlow::Break(()),
            Ok(true) => break,
            Err(RequestError::UnexpectedResponse(status)) => {
                debug!(
                    logger,
                    "Check returned {status}, trying again with lower version"
                );
            }
            Err(RequestError::General(err)) => {
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

// Returns whether the transfer is alive
async fn make_request(
    client: &hyper::Client<hyper::client::HttpConnector>,
    ip: IpAddr,
    version: protocol::Version,
    xfer_id: uuid::Uuid,
    auth: &auth::Context,
    logger: &Logger,
) -> Result<bool, RequestError> {
    use anyhow::Context;

    let addr = SocketAddr::new(ip, drop_config::PORT);
    let url: hyper::Uri = format!("http://{addr}/drop/{version}/check/{xfer_id}")
        .parse()
        .expect("URL should be valid");

    debug!(logger, "Making HTTP request: {url}");

    let mut req = hyper::Request::get(url.clone());

    use protocol::Version as Ver;
    let server_auth_scheme = match version {
        Ver::V1 | Ver::V2 | Ver::V4 | Ver::V5 => None,
        _ => {
            let nonce = drop_auth::Nonce::generate_as_client();

            let (key, value) = auth::create_www_authentication_header(&nonce);
            req = req.header(key, value);

            Some(nonce)
        }
    };

    let req = req
        .body(hyper::Body::empty())
        .expect("Creating request should not fail");

    let response = client
        .request(req)
        .await
        .context("Failed to perform HTTP request")?;

    let authorize = || {
        if let Some(nonce) = &server_auth_scheme {
            // Validate the server response
            auth.authorize_server(&response, ip, nonce)
                .context("Failed to authorize server. Closing connection")?;
        }
        anyhow::Ok(())
    };

    match response.status() {
        StatusCode::OK => {
            authorize()?;
            Ok(true)
        }
        StatusCode::GONE => {
            authorize()?;
            Ok(false)
        }
        StatusCode::UNAUTHORIZED => {
            authorize()?;

            debug!(logger, "Creating 'authorization' header");
            let (key, value) = auth.create_clients_auth_header(&response, ip, false)?;

            debug!(logger, "Building 'authorization' request");
            let req = hyper::Request::get(url)
                .header(key, value)
                .body(hyper::Body::empty())
                .expect("Creating request should not fail");

            let response = client
                .request(req)
                .await
                .context("Failed to perform the second HTTP request")?;

            match response.status() {
                StatusCode::OK => Ok(true),
                StatusCode::GONE => Ok(false),
                status => Err(RequestError::UnexpectedResponse(status)),
            }
        }
        status => Err(RequestError::UnexpectedResponse(status)),
    }
}
