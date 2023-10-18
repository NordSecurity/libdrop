use std::net::SocketAddr;

use hyper::http::HeaderValue;
use slog::warn;

use crate::auth;

#[derive(Debug)]
pub struct WWWAuthenticate {
    header_value: Option<String>,
}

#[derive(Debug)]
pub struct Authorization {
    header: Option<(&'static str, HeaderValue)>,
}

impl WWWAuthenticate {
    pub fn new(header_value: Option<String>) -> Self {
        Self { header_value }
    }

    pub fn authorize(
        self,
        auth: &auth::Context,
        peer: SocketAddr,
        logger: &slog::Logger,
    ) -> Authorization {
        let Self { header_value } = self;
        let header = create_authorization_header(auth, header_value, peer, logger);
        Authorization { header }
    }
}

impl Authorization {
    pub fn insert(&self, reply: impl warp::Reply + 'static) -> Box<dyn warp::Reply> {
        if let Some((key, val)) = &self.header {
            Box::new(warp::reply::with_header(reply, *key, val))
        } else {
            Box::new(reply)
        }
    }
}

fn create_authorization_header(
    auth: &auth::Context,
    www_auth_header: Option<String>,
    peer: SocketAddr,
    logger: &slog::Logger,
) -> Option<(&'static str, HeaderValue)> {
    let www_auth = www_auth_header?;

    match auth.create_servers_auth_header(peer.ip(), &www_auth) {
        Ok(header) => Some(header),
        Err(err) => {
            warn!(logger, "Failed to create authentication ticket: {err:?}");
            None
        }
    }
}
