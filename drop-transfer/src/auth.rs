use std::net::IpAddr;

use drop_auth::{PublicKey, SecretKey};

pub struct Context {
    secret: SecretKey,
    public: Box<dyn Fn(Option<IpAddr>) -> Option<PublicKey> + Send + Sync>,
}

impl Context {
    pub fn new(
        secret: SecretKey,
        public: impl Fn(Option<IpAddr>) -> Option<PublicKey> + Send + Sync + 'static,
    ) -> Self {
        Self {
            secret,
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
            let pubkey = (self.public)(Some(peer_ip))?;
            drop_auth::authorize(nonce, &self.secret, &pubkey, &auth_req)
        })
        .is_some()
    }

    pub fn create_ticket_header_val(&self, www_auth_header_value: &str) -> anyhow::Result<String> {
        use anyhow::Context;

        tokio::task::block_in_place(|| {
            let resp = drop_auth::http::WWWAuthenticate::parse(www_auth_header_value)
                .context("Failed to parse 'www-authenticate' header")?;

            let public = (self.public)(None).context("Failed to fetch own keys")?;

            let ticket = drop_auth::create_ticket(&self.secret, &public, resp)
                .context("Failed to create auth ticket")?;

            anyhow::Ok(ticket.to_string())
        })
    }
}
