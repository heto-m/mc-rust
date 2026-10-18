use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use std::fmt;
use std::sync::Arc;

use crate::error::AuthError;
use crate::secret::Secret;

/// Fetches fresh keys for an in-game session; `None` when the services fail.
#[derive(Clone)]
pub struct KeyRefresh(Arc<dyn Fn() -> Option<PlayerKeys> + Send + Sync>);

impl KeyRefresh {
    pub fn new(fetch: impl Fn() -> Option<PlayerKeys> + Send + Sync + 'static) -> Self {
        Self(Arc::new(fetch))
    }

    pub fn fetch(&self) -> Option<PlayerKeys> {
        (self.0)()
    }
}

impl fmt::Debug for KeyRefresh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyRefresh")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerKeys {
    pub private_key_pem: Secret,
    pub public_key_pem: String,
    pub public_key_signature: String,
    pub public_key_signature_v2: String,
    pub expires_at_ms: i64,
    pub refreshed_after_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CertificateResponse {
    key_pair: KeyPair,
    public_key_signature: String,
    public_key_signature_v2: String,
    expires_at: String,
    refreshed_after: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyPair {
    private_key: String,
    public_key: String,
}

impl PlayerKeys {
    pub fn from_response(body: &str) -> Result<Self, AuthError> {
        let r: CertificateResponse = serde_json::from_str(body)?;
        let keys = Self {
            private_key_pem: Secret::new(r.key_pair.private_key),
            public_key_pem: r.key_pair.public_key,
            public_key_signature: r.public_key_signature,
            public_key_signature_v2: r.public_key_signature_v2,
            expires_at_ms: parse_ms(&r.expires_at)?,
            refreshed_after_ms: parse_ms(&r.refreshed_after)?,
        };
        keys.private_key()?;
        keys.public_key_der()?;
        Ok(keys)
    }

    pub fn private_key(&self) -> Result<RsaPrivateKey, AuthError> {
        let pem = self.private_key_pem.expose();
        RsaPrivateKey::from_pkcs1_pem(pem)
            .or_else(|_| RsaPrivateKey::from_pkcs8_pem(pem))
            .map_err(|e| AuthError::Key(format!("private key: {e}")))
    }

    pub fn public_key(&self) -> Result<RsaPublicKey, AuthError> {
        RsaPublicKey::from_pkcs1_pem(&self.public_key_pem)
            .or_else(|_| RsaPublicKey::from_public_key_pem(&self.public_key_pem))
            .map_err(|e| AuthError::Key(format!("public key: {e}")))
    }

    pub fn public_key_der(&self) -> Result<Vec<u8>, AuthError> {
        self.public_key()?
            .to_public_key_der()
            .map(|d| d.as_bytes().to_vec())
            .map_err(|e| AuthError::Key(format!("public key der: {e}")))
    }

    pub fn signature_v2_bytes(&self) -> Result<Vec<u8>, AuthError> {
        decode_b64(&self.public_key_signature_v2)
    }

    pub fn signature_bytes(&self) -> Result<Vec<u8>, AuthError> {
        decode_b64(&self.public_key_signature)
    }

    pub fn is_expired(&self, now_secs: u64) -> bool {
        (now_secs as i64).saturating_mul(1000) >= self.expires_at_ms
    }

    pub fn should_refresh(&self, now_secs: u64) -> bool {
        (now_secs as i64).saturating_mul(1000) >= self.refreshed_after_ms
    }
}

fn decode_b64(s: &str) -> Result<Vec<u8>, AuthError> {
    STANDARD
        .decode(s.replace(['\n', '\r'], ""))
        .map_err(|e| AuthError::Key(format!("signature: {e}")))
}

fn parse_ms(s: &str) -> Result<i64, AuthError> {
    let t = OffsetDateTime::parse(s, &Rfc3339).map_err(|e| AuthError::Time(format!("{s}: {e}")))?;
    Ok((t.unix_timestamp_nanos() / 1_000_000) as i64)
}

#[cfg(test)]
pub(crate) mod tests {
    use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding};

    use super::*;

    pub fn fixture_json(expires_at: &str, refreshed_after: &str) -> String {
        let private = RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024).unwrap();
        let public = RsaPublicKey::from(&private);
        serde_json::json!({
            "keyPair": {
                "privateKey": private.to_pkcs1_pem(LineEnding::LF).unwrap().to_string(),
                "publicKey": public.to_pkcs1_pem(LineEnding::LF).unwrap(),
            },
            "publicKeySignature": "AAEC",
            "publicKeySignatureV2": "AwQF",
            "expiresAt": expires_at,
            "refreshedAfter": refreshed_after,
        })
        .to_string()
    }

    #[test]
    fn parses_certificates() {
        let json = fixture_json("2030-01-01T00:00:00.000000Z", "2029-12-31T12:00:00.000000Z");
        let keys = PlayerKeys::from_response(&json).unwrap();
        assert_eq!(keys.expires_at_ms, 1_893_456_000_000);
        assert_eq!(keys.signature_v2_bytes().unwrap(), [3, 4, 5]);
        assert_eq!(keys.signature_bytes().unwrap(), [0, 1, 2]);
        let der = keys.public_key_der().unwrap();
        assert_eq!(der[0], 0x30);
        assert_eq!(
            RsaPublicKey::from(&keys.private_key().unwrap()),
            keys.public_key().unwrap()
        );
        assert!(!keys.is_expired(1_700_000_000));
        assert!(keys.is_expired(1_900_000_000));
        assert!(keys.should_refresh(1_893_450_000));
    }

    #[test]
    fn debug_hides_private_key() {
        let json = fixture_json("2030-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
        let keys = PlayerKeys::from_response(&json).unwrap();
        assert!(!format!("{keys:?}").contains("PRIVATE KEY"));
    }

    #[test]
    fn garbage_key_rejected() {
        let json = r#"{"keyPair":{"privateKey":"x","publicKey":"y"},"publicKeySignature":"","publicKeySignatureV2":"","expiresAt":"2030-01-01T00:00:00Z","refreshedAfter":"2030-01-01T00:00:00Z"}"#;
        assert!(matches!(
            PlayerKeys::from_response(json),
            Err(AuthError::Key(_))
        ));
    }
}
