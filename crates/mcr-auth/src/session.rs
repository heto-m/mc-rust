use serde_json::json;
use sha1::{Digest, Sha1};

use crate::error::AuthError;
use crate::http::{HttpClient, Request};
use crate::secret::Secret;

const JOIN_URL: &str = "https://sessionserver.mojang.com/session/minecraft/join";

pub fn server_hash(server_id: &str, shared_secret: &[u8], server_public_key_der: &[u8]) -> String {
    let mut sha = Sha1::new();
    sha.update(server_id.as_bytes());
    sha.update(shared_secret);
    sha.update(server_public_key_der);
    let mut digest: [u8; 20] = sha.finalize().into();

    let negative = digest[0] & 0x80 != 0;
    if negative {
        let mut carry = true;
        for b in digest.iter_mut().rev() {
            let (v, c) = (!*b).overflowing_add(u8::from(carry));
            *b = v;
            carry = c;
        }
    }
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let trimmed = hex.trim_start_matches('0');
    match (negative, trimmed.is_empty()) {
        (_, true) => "0".into(),
        (true, false) => format!("-{trimmed}"),
        (false, false) => trimmed.into(),
    }
}

pub fn join_server(
    http: &impl HttpClient,
    access_token: &Secret,
    profile_id: &str,
    server_hash: &str,
) -> Result<(), AuthError> {
    let body = json!({
        "accessToken": access_token.expose(),
        "selectedProfile": profile_id.replace('-', ""),
        "serverId": server_hash,
    });
    let response = http.send(&Request::post_json(JOIN_URL, &body))?;
    if response.is_success() {
        Ok(())
    } else {
        Err(AuthError::Status {
            status: response.status,
            url: JOIN_URL.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockHttp;

    fn plain_hash(s: &str) -> String {
        server_hash(s, &[], &[])
    }

    #[test]
    fn known_vectors() {
        assert_eq!(
            plain_hash("Notch"),
            "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"
        );
        assert_eq!(
            plain_hash("jeb_"),
            "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"
        );
        assert_eq!(
            plain_hash("simon"),
            "88e16a1019277b15d58faf0541e11910eb756f6"
        );
    }

    #[test]
    fn join_posts_hash_and_dashless_id() {
        let http = MockHttp::default();
        http.push("session/minecraft/join", 204, "");
        join_server(
            &http,
            &Secret::new("tok"),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5",
            "abc",
        )
        .unwrap();
        let body = http.seen.borrow()[0].body.clone().unwrap().text;
        assert!(body.contains("069a79f444e94726a5befca90e38aaf5"));
        assert!(body.contains(r#""serverId":"abc""#));
    }

    #[test]
    fn join_failure_is_error() {
        let http = MockHttp::default();
        http.push("join", 403, r#"{"error":"ForbiddenOperationException"}"#);
        assert!(matches!(
            join_server(&http, &Secret::new("t"), "id", "h"),
            Err(AuthError::Status { status: 403, .. })
        ));
    }
}
