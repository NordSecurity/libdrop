use std::net::IpAddr;

use drop_auth::{PublicKey, SecretKey};

#[async_trait::async_trait]
pub trait Context: Sync + Send + 'static {
    async fn own(&self) -> Option<(&SecretKey, PublicKey)>;
    async fn peer_public(&self, ip: IpAddr) -> Option<PublicKey>;
}
