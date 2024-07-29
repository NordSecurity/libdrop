use std::net::IpAddr;

use drop_auth::{PublicKey, SecretKey};
use hyper::{http::HeaderValue, Response};

pub struct Context {
    secret: Box<dyn Fn() -> Option<SecretKey> + Send + Sync>,
    public: Box<dyn Fn(IpAddr) -> Option<PublicKey> + Send + Sync>,
}

impl Context {
    pub fn new(
        secret: impl Fn() -> Option<SecretKey> + Send + Sync + 'static,
        public: impl Fn(IpAddr) -> Option<PublicKey> + Send + Sync + 'static,
    ) -> Self {
        Self {
            secret: Box::new(secret),
            public: Box::new(public),
        }
    }

    pub fn authorize(
        &self,
        peer_ip: IpAddr,
        auth_header_value: &str,
        nonce: &drop_auth::Nonce,
    ) -> bool {
        tokio::task::block_in_place(|| {
            let auth_req = drop_auth::http::Authorization::parse(auth_header_value)?;
            let pubkey = (self.public)(peer_ip)?;
            let secret = (self.secret)()?;
            drop_auth::authorize(nonce, &secret, &pubkey, &auth_req)
        })
        .is_some()
    }

    pub fn authorize_server<T>(
        &self,
        response: &hyper::Response<T>,
        ip: IpAddr,
        nonce: &drop_auth::Nonce,
    ) -> anyhow::Result<()> {
        use anyhow::Context;

        let ticket = response
            .headers()
            .get(drop_auth::http::Authorization::KEY)
            .context("Missing 'authorization' header")?
            .to_str()
            .context("Invalid 'authorization' header value")?;

        anyhow::ensure!(
            self.authorize(ip, ticket, nonce),
            "Cannot authorize server with ticket provided"
        );
        Ok(())
    }

    pub fn create_clients_auth_header<T>(
        &self,
        response: &Response<T>,
        peer_ip: IpAddr,
        check_nonce_prefix: bool,
    ) -> anyhow::Result<(&'static str, HeaderValue)> {
        use anyhow::Context;

        tokio::task::block_in_place(|| {
            let www_auth_header_value = response
                .headers()
                .get(drop_auth::http::WWWAuthenticate::KEY)
                .context("Missing 'www-authenticate' header")?
                .to_str()?;

            let resp = drop_auth::http::WWWAuthenticate::parse(www_auth_header_value)
                .context("Failed to parse 'www-authenticate' header")?;

            let public = (self.public)(peer_ip).context("Failed to fetch peer's public key")?;
            let secret = (self.secret)().context("Failed to fetch private key")?;

            let ticket =
                drop_auth::create_ticket_as_client(&secret, &public, resp, check_nonce_prefix)
                    .context("Failed to create auth ticket")?;

            let value = HeaderValue::from_str(&ticket.to_string())?;
            anyhow::Ok((drop_auth::http::Authorization::KEY, value))
        })
    }

    pub fn create_servers_auth_header(
        &self,
        peer_ip: IpAddr,
        www_auth_value: &str,
    ) -> anyhow::Result<(&'static str, HeaderValue)> {
        use anyhow::Context;

        tokio::task::block_in_place(|| {
            let resp = drop_auth::http::WWWAuthenticate::parse(www_auth_value)
                .context("Failed to parse 'www-authenticate' header")?;

            let public = (self.public)(peer_ip).context("Failed to fetch peer's public key")?;
            let secret = (self.secret)().context("Failed to fetch private key")?;

            let ticket = drop_auth::create_ticket_as_server(&secret, &public, resp)
                .context("Failed to create auth ticket")?;

            let value = HeaderValue::from_str(&ticket.to_string())?;
            anyhow::Ok((drop_auth::http::Authorization::KEY, value))
        })
    }
}

pub fn create_www_authentication_header(nonce: &drop_auth::Nonce) -> (&'static str, HeaderValue) {
    let value = drop_auth::http::WWWAuthenticate::new(*nonce);

    (
        drop_auth::http::WWWAuthenticate::KEY,
        HeaderValue::from_str(&value.to_string())
            .expect("The www-authenticate header value should be always valid"),
    )
}
