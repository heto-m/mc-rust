//! The `textures` profile property: where a player's skin lives, and a bounded download of it.

use std::io::Read;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const MAX_PROPERTY_BYTES: usize = 16 * 1024;
const MAX_SKIN_BYTES: u64 = 256 * 1024;
const TEXTURE_HOST: &str = "textures.minecraft.net";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinSource {
    pub url: String,
    pub slim: bool,
}

/// The skin of a `textures` property value. Only `textures.minecraft.net` is trusted, over https.
pub fn parse_textures(value: &str) -> Option<SkinSource> {
    if value.len() > MAX_PROPERTY_BYTES {
        return None;
    }
    let json = STANDARD.decode(value.trim()).ok()?;
    let root: serde_json::Value = serde_json::from_slice(&json).ok()?;
    let skin = root.get("textures")?.get("SKIN")?;
    let url = skin.get("url")?.as_str()?;
    let slim = skin
        .get("metadata")
        .and_then(|m| m.get("model"))
        .and_then(|m| m.as_str())
        == Some("slim");
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let (host, path) = rest.split_once('/')?;
    let plain_path = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'));
    (host == TEXTURE_HOST && !path.is_empty() && plain_path).then(|| SkinSource {
        url: format!("https://{host}/{path}"),
        slim,
    })
}

/// The PNG bytes behind a skin URL, at most 256 KiB.
pub fn fetch_skin(source: &SkinSource) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .build()
        .into();
    let mut response = agent.get(&source.url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_SKIN_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SKIN_BYTES {
        return Err("skin too large".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(url: &str, model: Option<&str>) -> String {
        let meta = model.map_or(String::new(), |m| {
            format!(r#","metadata":{{"model":"{m}"}}"#)
        });
        STANDARD.encode(format!(
            r#"{{"textures":{{"SKIN":{{"url":"{url}"{meta}}}}}}}"#
        ))
    }

    #[test]
    fn trusted_host_is_upgraded_to_https() {
        let s = parse_textures(&property(
            "http://textures.minecraft.net/texture/abc123",
            None,
        ))
        .unwrap();
        assert_eq!(s.url, "https://textures.minecraft.net/texture/abc123");
        assert!(!s.slim);
    }

    #[test]
    fn slim_model_is_read() {
        let p = property("https://textures.minecraft.net/texture/a", Some("slim"));
        assert!(parse_textures(&p).unwrap().slim);
    }

    #[test]
    fn other_hosts_and_tricks_are_rejected() {
        for url in [
            "http://evil.example/texture/a",
            "http://textures.minecraft.net.evil.example/a",
            "http://textures.minecraft.net@evil.example/a",
            "http://user@textures.minecraft.net/a",
            "http://textures.minecraft.net:8080/a",
            "ftp://textures.minecraft.net/a",
            "http://textures.minecraft.net/",
            "http://textures.minecraft.net/a?x=1",
        ] {
            assert_eq!(parse_textures(&property(url, None)), None, "{url}");
        }
    }

    #[test]
    fn garbage_and_oversize_are_rejected() {
        assert_eq!(parse_textures("not base64!"), None);
        assert_eq!(parse_textures(&STANDARD.encode("{}")), None);
        assert_eq!(parse_textures(&"A".repeat(MAX_PROPERTY_BYTES + 1)), None);
    }
}
