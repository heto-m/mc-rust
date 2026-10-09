use rand::RngCore;
use rand::rngs::OsRng;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};

use crate::error::AuthError;

pub const SHARED_SECRET_LEN: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedHandshake {
    pub shared_secret: Vec<u8>,
    pub verify_token: Vec<u8>,
}

pub fn generate_shared_secret() -> [u8; SHARED_SECRET_LEN] {
    let mut secret = [0u8; SHARED_SECRET_LEN];
    OsRng.fill_bytes(&mut secret);
    secret
}

pub fn parse_server_key(der: &[u8]) -> Result<RsaPublicKey, AuthError> {
    RsaPublicKey::from_public_key_der(der).map_err(|e| AuthError::Key(format!("server key: {e}")))
}

pub fn encrypt_handshake(
    server_public_key_der: &[u8],
    shared_secret: &[u8],
    verify_token: &[u8],
) -> Result<EncryptedHandshake, AuthError> {
    let key = parse_server_key(server_public_key_der)?;
    let encrypt = |data: &[u8]| {
        key.encrypt(&mut OsRng, Pkcs1v15Encrypt, data)
            .map_err(|e| AuthError::Key(format!("encrypt: {e}")))
    };
    Ok(EncryptedHandshake {
        shared_secret: encrypt(shared_secret)?,
        verify_token: encrypt(verify_token)?,
    })
}

#[cfg(test)]
mod tests {
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::EncodePublicKey;

    use super::*;

    #[test]
    fn secrets_are_16_bytes_and_distinct() {
        assert_ne!(generate_shared_secret(), generate_shared_secret());
    }

    #[test]
    fn round_trip() {
        let private = RsaPrivateKey::new(&mut OsRng, 1024).unwrap();
        let der = RsaPublicKey::from(&private).to_public_key_der().unwrap();
        let secret = generate_shared_secret();
        let enc = encrypt_handshake(der.as_bytes(), &secret, &[9, 8, 7, 6]).unwrap();
        assert_eq!(
            private
                .decrypt(Pkcs1v15Encrypt, &enc.shared_secret)
                .unwrap(),
            secret
        );
        assert_eq!(
            private.decrypt(Pkcs1v15Encrypt, &enc.verify_token).unwrap(),
            [9, 8, 7, 6]
        );
    }

    #[test]
    fn bad_der_rejected() {
        assert!(encrypt_handshake(&[1, 2, 3], &[0; 16], &[0; 4]).is_err());
    }
}
