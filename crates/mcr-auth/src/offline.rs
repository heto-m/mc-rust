use md5::{Digest, Md5};
use uuid::{Builder, Uuid};

use crate::error::AuthError;

pub const MAX_NAME_LEN: usize = 16;

pub fn validate_name(name: &str) -> Result<(), AuthError> {
    if name.is_empty() {
        return Err(AuthError::InvalidName("empty"));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(AuthError::InvalidName("longer than 16 characters"));
    }
    if !name.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(AuthError::InvalidName(
            "must be printable ASCII without spaces",
        ));
    }
    Ok(())
}

pub fn offline_uuid(name: &str) -> Result<Uuid, AuthError> {
    validate_name(name)?;
    let digest: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}")).into();
    Ok(Builder::from_md5_bytes(digest).into_uuid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_uuid() {
        assert_eq!(
            offline_uuid("Notch").unwrap().to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn version_and_variant() {
        let id = offline_uuid("jeb_").unwrap();
        assert_eq!(id.get_version_num(), 3);
        assert_eq!(id.get_variant(), uuid::Variant::RFC4122);
    }

    #[test]
    fn name_rules() {
        assert!(validate_name("Steve_01").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("seventeen_chars_x").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name("héllo").is_err());
    }
}
