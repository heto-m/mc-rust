//! Secure chat crypto: vanilla's signed message layout, SHA256withRSA chat signatures and Mojang's
//! SHA1withRSA profile key signatures. No session state lives here.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::{DigestSigner, DigestVerifier, SignatureEncoding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AuthError;

pub const SIGNATURE_BYTES: usize = 256;
pub const MAX_LAST_SEEN: usize = 20;
pub const MAX_PUBLIC_KEY_BYTES: usize = 512;
pub const MAX_KEY_SIGNATURE_BYTES: usize = 4096;
pub const MAX_SERVICES_KEYS: usize = 16;
const MAX_KEY_BITS: usize = 4096;
const SIGNED_MESSAGE_VERSION: i32 = 1;

pub type MessageSignature = [u8; SIGNATURE_BYTES];

/// A message's place in its sender's chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageLink {
    pub sender: Uuid,
    pub session: Uuid,
    pub index: i32,
}

/// The signed part of a chat message or command argument.
#[derive(Debug, Clone, Copy)]
pub struct SignedBody<'a> {
    pub content: &'a str,
    pub timestamp_ms: i64,
    pub salt: i64,
    pub last_seen: &'a [MessageSignature],
}

/// Feeds vanilla's signed bytes for `link` and `body` to `out` in order.
pub fn feed_signed(link: &MessageLink, body: &SignedBody<'_>, mut out: impl FnMut(&[u8])) {
    out(&SIGNED_MESSAGE_VERSION.to_be_bytes());
    out(link.sender.as_bytes());
    out(link.session.as_bytes());
    out(&link.index.to_be_bytes());
    out(&body.salt.to_be_bytes());
    out(&body.timestamp_ms.div_euclid(1000).to_be_bytes());
    out(&(body.content.len() as i32).to_be_bytes());
    out(body.content.as_bytes());
    out(&(body.last_seen.len() as i32).to_be_bytes());
    for signature in body.last_seen {
        out(signature);
    }
}

pub fn signed_bytes(link: &MessageLink, body: &SignedBody<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    feed_signed(link, body, |b| bytes.extend_from_slice(b));
    bytes
}

fn digest(link: &MessageLink, body: &SignedBody<'_>) -> Sha256 {
    let mut hasher = Sha256::new();
    feed_signed(link, body, |b| hasher.update(b));
    hasher
}

/// Signs chat with the profile's private key.
#[derive(Clone)]
pub struct ChatSigner {
    key: SigningKey<Sha256>,
}

impl fmt::Debug for ChatSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChatSigner(***)")
    }
}

impl ChatSigner {
    /// `None` unless the key makes vanilla's 256-byte signatures (2048 bits).
    pub fn new(key: RsaPrivateKey) -> Option<Self> {
        (key.size() == SIGNATURE_BYTES).then(|| Self {
            key: SigningKey::new(key),
        })
    }

    pub fn sign(&self, link: &MessageLink, body: &SignedBody<'_>) -> Option<MessageSignature> {
        let signature: Signature = self.key.try_sign_digest(digest(link, body)).ok()?;
        signature.to_bytes().as_ref().try_into().ok()
    }
}

/// A chat sender's public key.
#[derive(Clone)]
pub struct ChatVerifier {
    key: VerifyingKey<Sha256>,
}

impl fmt::Debug for ChatVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChatVerifier")
    }
}

/// An X.509 SubjectPublicKeyInfo RSA key within the protocol's bounds.
pub fn parse_public_key(der: &[u8]) -> Result<RsaPublicKey, AuthError> {
    if der.len() > MAX_PUBLIC_KEY_BYTES {
        return Err(AuthError::Key("public key too long".into()));
    }
    let key = RsaPublicKey::from_public_key_der(der)
        .map_err(|e| AuthError::Key(format!("public key: {e}")))?;
    if key.n().bits() > MAX_KEY_BITS {
        return Err(AuthError::Key("public key too large".into()));
    }
    Ok(key)
}

impl ChatVerifier {
    pub fn new(key: RsaPublicKey) -> Self {
        Self {
            key: VerifyingKey::new(key),
        }
    }

    pub fn from_der(der: &[u8]) -> Result<Self, AuthError> {
        parse_public_key(der).map(Self::new)
    }

    pub fn verify(&self, link: &MessageLink, body: &SignedBody<'_>, signature: &[u8]) -> bool {
        if signature.len() != SIGNATURE_BYTES {
            return false;
        }
        let Ok(signature) = Signature::try_from(signature) else {
            return false;
        };
        self.key
            .verify_digest(digest(link, body), &signature)
            .is_ok()
    }
}

/// Mojang's player-certificate keys, which sign every profile public key.
#[derive(Clone, Default)]
pub struct ServicesKeys {
    keys: Vec<VerifyingKey<Sha1>>,
}

impl fmt::Debug for ServicesKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ServicesKeys({} keys)", self.keys.len())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeySetResponse {
    #[serde(default)]
    player_certificate_keys: Vec<KeyEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyEntry {
    public_key: String,
}

impl ServicesKeys {
    pub fn new(keys: impl IntoIterator<Item = RsaPublicKey>) -> Self {
        Self {
            keys: keys
                .into_iter()
                .take(MAX_SERVICES_KEYS)
                .map(VerifyingKey::new)
                .collect(),
        }
    }

    /// The `/publickeys` response; unparseable entries are skipped.
    pub fn from_response(body: &str) -> Result<Self, AuthError> {
        let response: KeySetResponse = serde_json::from_str(body)?;
        Ok(Self::new(
            response
                .player_certificate_keys
                .iter()
                .take(MAX_SERVICES_KEYS)
                .filter_map(|k| STANDARD.decode(k.public_key.as_bytes()).ok())
                .filter_map(|der| RsaPublicKey::from_public_key_der(&der).ok()),
        ))
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Vanilla's `ProfilePublicKey.Data.validateSignature`.
    pub fn validate_profile_key(
        &self,
        profile: Uuid,
        expires_at_ms: i64,
        public_key_der: &[u8],
        signature: &[u8],
    ) -> bool {
        if signature.len() > MAX_KEY_SIGNATURE_BYTES || public_key_der.len() > MAX_PUBLIC_KEY_BYTES
        {
            return false;
        }
        let Ok(signature) = Signature::try_from(signature) else {
            return false;
        };
        let mut hasher = Sha1::new();
        hasher.update(profile.as_bytes());
        hasher.update(expires_at_ms.to_be_bytes());
        hasher.update(public_key_der);
        self.keys
            .iter()
            .any(|k| k.verify_digest(hasher.clone(), &signature).is_ok())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::OnceLock;

    use rsa::pkcs8::EncodePublicKey;

    use super::*;

    /// One 2048-bit key per test binary; generated here, never a real key.
    pub fn test_key() -> &'static RsaPrivateKey {
        static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
        KEY.get_or_init(|| RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap())
    }

    fn other_key() -> &'static RsaPrivateKey {
        static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
        KEY.get_or_init(|| RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap())
    }

    fn link() -> MessageLink {
        MessageLink {
            sender: Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            session: Uuid::from_u128(0x1112_1314_1516_1718_191a_1b1c_1d1e_1f20),
            index: 3,
        }
    }

    #[test]
    fn signed_body_bytes() {
        let body = SignedBody {
            content: "hi",
            timestamp_ms: 1_700_000_000_123,
            salt: 7,
            last_seen: &[[0xab; 256]],
        };
        let expected = [
            &[0, 0, 0, 1][..],
            &(1u8..=16).collect::<Vec<_>>(),
            &(0x11u8..=0x20).collect::<Vec<_>>(),
            &[0, 0, 0, 3],
            &[0, 0, 0, 0, 0, 0, 0, 7],
            &[0, 0, 0, 0, 0x65, 0x53, 0xf1, 0x00],
            &[0, 0, 0, 2, b'h', b'i'],
            &[0, 0, 0, 1],
            &[0xab; 256],
        ]
        .concat();
        assert_eq!(signed_bytes(&link(), &body), expected);
    }

    #[test]
    fn negative_timestamp_floors_like_instant() {
        let body = SignedBody {
            content: "",
            timestamp_ms: -1,
            salt: 0,
            last_seen: &[],
        };
        let bytes = signed_bytes(&link(), &body);
        assert_eq!(&bytes[48..56], &(-1i64).to_be_bytes());
    }

    #[test]
    fn sign_verify_round_trip() {
        let signer = ChatSigner::new(test_key().clone()).unwrap();
        let verifier = ChatVerifier::new(RsaPublicKey::from(test_key()));
        let seen = [[1u8; 256], [2u8; 256]];
        let body = SignedBody {
            content: "hello there",
            timestamp_ms: 1_700_000_000_000,
            salt: -42,
            last_seen: &seen,
        };
        let signature = signer.sign(&link(), &body).unwrap();
        assert!(verifier.verify(&link(), &body, &signature));

        let tampered = SignedBody {
            content: "hello therE",
            ..body
        };
        assert!(!verifier.verify(&link(), &tampered, &signature));
        let next = MessageLink { index: 4, ..link() };
        assert!(!verifier.verify(&next, &body, &signature));
        assert!(!verifier.verify(&link(), &body, &signature[..255]));
        let mut flipped = signature;
        flipped[100] ^= 1;
        assert!(!verifier.verify(&link(), &body, &flipped));
        let stranger = ChatVerifier::new(RsaPublicKey::from(other_key()));
        assert!(!stranger.verify(&link(), &body, &signature));
        assert!(!format!("{signer:?}").contains("PRIVATE"));
    }

    #[test]
    fn signer_needs_a_2048_bit_key() {
        let small = RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024).unwrap();
        assert!(ChatSigner::new(small).is_none());
    }

    #[test]
    fn public_key_bounds() {
        let der = RsaPublicKey::from(test_key())
            .to_public_key_der()
            .unwrap()
            .into_vec();
        assert!(ChatVerifier::from_der(&der).is_ok());
        assert!(ChatVerifier::from_der(&der[..der.len() - 1]).is_err());
        assert!(ChatVerifier::from_der(&[0x30; 600]).is_err());
        assert!(ChatVerifier::from_der(&[]).is_err());
    }

    pub fn sign_profile_key(
        services: &RsaPrivateKey,
        profile: Uuid,
        expires_at_ms: i64,
        der: &[u8],
    ) -> Vec<u8> {
        let mut hasher = Sha1::new();
        hasher.update(profile.as_bytes());
        hasher.update(expires_at_ms.to_be_bytes());
        hasher.update(der);
        let signature: Signature = SigningKey::<Sha1>::new(services.clone()).sign_digest(hasher);
        signature.to_vec()
    }

    #[test]
    fn profile_key_validation() {
        let services = other_key();
        let der = RsaPublicKey::from(test_key())
            .to_public_key_der()
            .unwrap()
            .into_vec();
        let profile = Uuid::from_u128(77);
        let signature = sign_profile_key(services, profile, 1_900_000_000_000, &der);
        let keys = ServicesKeys::new([RsaPublicKey::from(services)]);
        assert!(keys.validate_profile_key(profile, 1_900_000_000_000, &der, &signature));
        assert!(!keys.validate_profile_key(
            Uuid::from_u128(78),
            1_900_000_000_000,
            &der,
            &signature
        ));
        assert!(!keys.validate_profile_key(profile, 1_900_000_000_001, &der, &signature));
        assert!(!keys.validate_profile_key(profile, 1_900_000_000_000, &der, &[0; 5000]));
        assert!(!ServicesKeys::default().validate_profile_key(
            profile,
            1_900_000_000_000,
            &der,
            &signature
        ));
    }

    #[test]
    fn services_key_response() {
        let der = RsaPublicKey::from(test_key())
            .to_public_key_der()
            .unwrap()
            .into_vec();
        let entry = serde_json::json!({ "publicKey": STANDARD.encode(&der) });
        let mut certs: Vec<serde_json::Value> =
            vec![entry.clone(), serde_json::json!({"publicKey": "!!"})];
        certs.extend(std::iter::repeat_n(entry, 40));
        let body = serde_json::json!({
            "profilePropertyKeys": [],
            "playerCertificateKeys": certs,
        })
        .to_string();
        let keys = ServicesKeys::from_response(&body).unwrap();
        assert_eq!(keys.len(), MAX_SERVICES_KEYS - 1);
        assert!(ServicesKeys::from_response("{}").unwrap().is_empty());
        assert!(ServicesKeys::from_response("[").is_err());
        assert_eq!(format!("{keys:?}"), "ServicesKeys(15 keys)");
    }
}
