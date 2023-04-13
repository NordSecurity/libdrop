use std::net::IpAddr;

use drop_auth::{Keypair, PublicKey};

pub trait Context: Sync + Send + 'static {
    fn own(&self) -> Keypair;
    fn peer_public(&self, ip: IpAddr) -> Option<PublicKey>;
}
