pub mod http;

use base64::Engine;
use ed25519_dalek::ExpandedSecretKey;
use rand::RngCore;

pub const AUTH_SCHEME: &str = "drop";

pub const NONCE_LEN: usize = 32;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64;
pub use ed25519_dalek::{
    PublicKey, SecretKey, Signature, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH, SIGNATURE_LENGTH,
};

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Nonce(pub [u8; NONCE_LEN]);

impl Nonce {
    pub fn generate() -> Self {
        let mut dst = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut dst);
        Self(dst)
    }
}

impl From<&[u8]> for Nonce {
    fn from(value: &[u8]) -> Self {
        let mut this = [0u8; NONCE_LEN];

        let len = NONCE_LEN.min(value.len());
        this[..len].copy_from_slice(&value[..len]);

        Self(this)
    }
}

pub fn authorize(
    server_nonce: &Nonce,
    client_pubkey: &PublicKey,
    http::Authorization { ticket, nonce }: &http::Authorization,
) -> Option<()> {
    let sign = {
        let sign = BASE64.decode(ticket).ok()?;
        Signature::from_bytes(&sign).ok()?
    };

    let nonce = Nonce::from(BASE64.decode(nonce).ok()?.as_slice());

    if nonce != *server_nonce {
        return None;
    }

    validate(&sign, client_pubkey, &nonce).then_some(())
}

pub fn create_ticket(
    secret: &SecretKey,
    pubkey: &PublicKey,
    http::WWWAuthenticate { nonce }: http::WWWAuthenticate,
) -> Option<http::Authorization> {
    let nonce_bytes = Nonce::from(BASE64.decode(&nonce).ok()?.as_slice());

    let ticket = {
        let signature = sign(secret, pubkey, &nonce_bytes);
        BASE64.encode(signature)
    };

    Some(http::Authorization { ticket, nonce })
}

fn sign(secret: &SecretKey, pubkey: &PublicKey, nonce: &Nonce) -> Signature {
    let expanded: ExpandedSecretKey = secret.into();

    expanded.sign(&nonce.0, pubkey)
}

fn validate(ticket: &Signature, pubkey: &PublicKey, nonce: &Nonce) -> bool {
    pubkey.verify_strict(&nonce.0, ticket).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PRIV_KEY: [u8; SECRET_KEY_LENGTH] = [
        164, 70, 230, 247, 55, 28, 255, 147, 128, 74, 83, 50, 181, 222, 212, 18, 178, 162, 242,
        102, 220, 203, 153, 161, 142, 206, 123, 188, 87, 77, 126, 183,
    ];
    const TEST_PUB_KEY: [u8; PUBLIC_KEY_LENGTH] = [
        68, 103, 21, 143, 132, 253, 95, 17, 203, 20, 154, 169, 66, 197, 210, 103, 56, 18, 143, 142,
        142, 47, 53, 103, 186, 66, 91, 201, 181, 186, 12, 136,
    ];

    #[test]
    fn ticket_validation() {
        let public = PublicKey::from_bytes(&TEST_PUB_KEY).unwrap();
        let secret = SecretKey::from_bytes(&TEST_PRIV_KEY).unwrap();

        let nonce = Nonce([42; NONCE_LEN]);

        let ticket = sign(&secret, &public, &nonce);
        let valid = validate(&ticket, &public, &nonce);
        assert!(valid);
    }
}