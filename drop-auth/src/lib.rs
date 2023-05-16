pub mod http;

use base64::{engine::general_purpose::STANDARD_NO_PAD as BASE64, Engine};

pub use crypto_box::{PublicKey, SecretKey};
use rand::RngCore;

pub const AUTH_SCHEME: &str = "drop";

pub const NONCE_LEN: usize = 24;
pub const PUBLIC_KEY_LENGTH: usize = 32;
pub const SECRET_KEY_LENGTH: usize = 32;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Nonce(pub [u8; NONCE_LEN]);

const DOMAIN_STRING: &str = "libdrop-auth";

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
    server_secret: &SecretKey,
    client_pubkey: &PublicKey,
    http::Authorization { ticket, nonce }: &http::Authorization,
) -> Option<()> {
    let nonce = Nonce::from(BASE64.decode(nonce).ok()?.as_slice());
    if nonce != *server_nonce {
        return None;
    }

    let client_tag = BASE64.decode(ticket).ok()?;

    let tag = create_tag(server_secret, client_pubkey, nonce)?;

    if tag == client_tag {
        Some(())
    } else {
        None
    }
}

pub fn create_ticket(
    client_secret: &SecretKey,
    server_pubkey: &PublicKey,
    http::WWWAuthenticate { nonce }: http::WWWAuthenticate,
) -> Option<http::Authorization> {
    let nonce_bytes = Nonce::from(BASE64.decode(&nonce).ok()?.as_slice());

    let tag = create_tag(client_secret, server_pubkey, nonce_bytes)?;
    let ticket = BASE64.encode(tag);

    Some(http::Authorization { ticket, nonce })
}

fn create_tag(secret: &SecretKey, pubkey: &PublicKey, nonce: Nonce) -> Option<Vec<u8>> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let secret = x25519_dalek::StaticSecret::from(secret.as_bytes().clone());
    let pubkey = x25519_dalek::PublicKey::from(pubkey.as_bytes().clone());

    let shared_secret = secret.diffie_hellman(&pubkey);

    let mut hmac = HmacSha256::new_from_slice(shared_secret.as_bytes()).ok()?;
    hmac.update(DOMAIN_STRING.as_bytes());
    hmac.update(nonce.0.as_slice());
    let tag = hmac.finalize().into_bytes().to_vec();

    Some(tag)
}

#[cfg(test)]
mod tests {
    use crypto_box::{PublicKey, SecretKey};

    use super::*;

    const ALICE_PRIV_KEY: [u8; SECRET_KEY_LENGTH] = [
        0x15, 0xc6, 0xe3, 0x45, 0x08, 0xf8, 0x3e, 0x4d, 0x3a, 0x28, 0x9d, 0xd4, 0xa4, 0x05, 0x95,
        0x8d, 0x8a, 0xa4, 0x68, 0x2d, 0x4a, 0xba, 0x4f, 0xf3, 0x2d, 0x8f, 0x72, 0x60, 0x4b, 0x69,
        0x46, 0xc7,
    ];
    const ALICE_PUB_KEY: [u8; PUBLIC_KEY_LENGTH] = [
        0x24, 0x0f, 0xcc, 0x7b, 0xbc, 0x11, 0x0c, 0x12, 0x7a, 0xed, 0xf9, 0x26, 0x8e, 0x9a, 0x24,
        0xa4, 0x5a, 0x1b, 0x4c, 0xb1, 0x87, 0x4e, 0xff, 0x46, 0x5e, 0x56, 0x31, 0xb2, 0x33, 0x6b,
        0xca, 0x6d,
    ];

    const BOB_PRIV_KEY: [u8; SECRET_KEY_LENGTH] = [
        0xac, 0x72, 0xec, 0x24, 0x97, 0xc8, 0x8c, 0xe6, 0xa9, 0x5b, 0xcf, 0xd1, 0x75, 0x22, 0xd8,
        0x25, 0xa7, 0xf3, 0x0e, 0x7b, 0xaf, 0x6c, 0x6d, 0xc7, 0x1c, 0xef, 0x58, 0xee, 0xa1, 0x64,
        0xa2, 0xa1,
    ];
    const BOB_PUB_KEY: [u8; PUBLIC_KEY_LENGTH] = [
        0x48, 0x2b, 0x12, 0x20, 0x0c, 0x53, 0x9f, 0x8e, 0x57, 0x58, 0xf2, 0xb3, 0xb5, 0x66, 0xe3,
        0x98, 0x1d, 0xca, 0x4c, 0xb8, 0xba, 0x0c, 0xf2, 0xbc, 0xb5, 0xac, 0xf6, 0x91, 0xf3, 0xd0,
        0xdb, 0x1f,
    ];

    const CHARLIE_PRIV_KEY: [u8; SECRET_KEY_LENGTH] = [
        0xb5, 0x03, 0xa3, 0x19, 0x43, 0x2a, 0xcd, 0x46, 0x3f, 0x4c, 0x40, 0xb5, 0xfd, 0x8d, 0x6e,
        0x05, 0x59, 0xd0, 0x3b, 0x08, 0x2b, 0x6d, 0x5b, 0x30, 0x78, 0x8f, 0xe2, 0x96, 0xda, 0xbd,
        0xd0, 0xdf,
    ];
    const CHARLIE_PUB_KEY: [u8; PUBLIC_KEY_LENGTH] = [
        0xae, 0xca, 0x77, 0xf4, 0x94, 0x4b, 0x21, 0x21, 0xb0, 0x24, 0xee, 0x64, 0x88, 0xd1, 0x5b,
        0x35, 0xe4, 0x6e, 0xc0, 0x27, 0x77, 0x83, 0x76, 0x4d, 0x5e, 0x52, 0xf0, 0xbf, 0xc2, 0x47,
        0x5e, 0x0a,
    ];

    #[test]
    fn ticket_validation() {
        let alice_public = PublicKey::from(ALICE_PUB_KEY);
        let alice_secret = SecretKey::from(ALICE_PRIV_KEY);

        let bob_public = PublicKey::from(BOB_PUB_KEY);
        let bob_secret = SecretKey::from(BOB_PRIV_KEY);

        let nonce = Nonce([42; NONCE_LEN]);

        assert_eq!(
            create_tag(&alice_secret, &bob_public, nonce),
            create_tag(&bob_secret, &alice_public, nonce)
        );

        let charlie_public = PublicKey::from(CHARLIE_PUB_KEY);
        let charlie_secret = SecretKey::from(CHARLIE_PRIV_KEY);

        // Test if fake keys are detected
        assert_ne!(
            create_tag(&alice_secret, &bob_public, nonce),
            create_tag(&charlie_secret, &alice_public, nonce)
        );
        assert_ne!(
            create_tag(&alice_secret, &charlie_public, nonce),
            create_tag(&bob_secret, &alice_public, nonce)
        );
    }
}
