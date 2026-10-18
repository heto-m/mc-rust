//! The game versions mcr can play: the canonical 26.3 set plus, later, versions with a translator onto it.

pub const GAME_VERSION: &str = "26.3";

/// Newest first.
pub const SUPPORTED_VERSIONS: &[&str] = &[GAME_VERSION];

pub fn is_supported(id: &str) -> bool {
    SUPPORTED_VERSIONS.contains(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_version_is_supported() {
        assert!(is_supported(GAME_VERSION));
        assert!(!is_supported("26.2"));
        assert!(!is_supported(""));
    }
}
