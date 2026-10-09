use std::time::Duration;

use serde::Deserialize;

use crate::error::AuthError;
use crate::http::{HttpClient, Request};
use crate::secret::Secret;

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";
pub const CLIENT_ID_ENV: &str = "MCR_MSA_CLIENT_ID";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId(String);

impl ClientId {
    pub fn new(id: impl Into<String>) -> Result<Self, AuthError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(AuthError::MissingClientId);
        }
        Ok(Self(id))
    }

    pub fn from_env() -> Result<Self, AuthError> {
        Self::new(std::env::var(CLIENT_ID_ENV).unwrap_or_default())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub message: String,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone)]
pub struct MsaTokens {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

pub fn request_device_code(
    http: &impl HttpClient,
    client_id: &ClientId,
) -> Result<DeviceCode, AuthError> {
    let request = Request::post_form(
        DEVICE_CODE_URL,
        &[("client_id", client_id.as_str()), ("scope", SCOPE)],
    );
    http.send(&request)?.json(DEVICE_CODE_URL)
}

pub fn poll_for_tokens(
    http: &impl HttpClient,
    client_id: &ClientId,
    device: &DeviceCode,
    mut sleep: impl FnMut(Duration),
) -> Result<MsaTokens, AuthError> {
    let mut interval = device.interval.max(1);
    let mut elapsed = 0;
    while elapsed < device.expires_in {
        sleep(Duration::from_secs(interval));
        elapsed += interval;
        let request = Request::post_form(
            TOKEN_URL,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id.as_str()),
                ("device_code", &device.device_code),
            ],
        );
        let response = http.send(&request)?;
        if response.is_success() {
            return tokens(&response.body);
        }
        match oauth_error(&response.body)?.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += 5,
            "authorization_declined" | "access_denied" => return Err(AuthError::LoginDeclined),
            "expired_token" | "bad_verification_code" => return Err(AuthError::LoginExpired),
            other => return Err(AuthError::OAuth(other.into())),
        }
    }
    Err(AuthError::LoginExpired)
}

pub fn refresh(
    http: &impl HttpClient,
    client_id: &ClientId,
    refresh_token: &Secret,
) -> Result<MsaTokens, AuthError> {
    let request = Request::post_form(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token.expose()),
            ("scope", SCOPE),
        ],
    );
    let response = http.send(&request)?;
    if response.is_success() {
        return tokens(&response.body);
    }
    match oauth_error(&response.body)? {
        e if e.error == "invalid_grant" => Err(AuthError::RefreshRejected),
        e => Err(AuthError::OAuth(format!(
            "{}: {}",
            e.error, e.error_description
        ))),
    }
}

fn tokens(body: &str) -> Result<MsaTokens, AuthError> {
    let t: TokenResponse = serde_json::from_str(body)?;
    Ok(MsaTokens {
        access_token: Secret::new(t.access_token),
        refresh_token: Secret::new(t.refresh_token),
        expires_in: t.expires_in,
    })
}

fn oauth_error(body: &str) -> Result<OAuthError, AuthError> {
    Ok(serde_json::from_str(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockHttp;

    const TOKENS: &str =
        r#"{"token_type":"bearer","expires_in":3600,"access_token":"AT","refresh_token":"RT"}"#;
    const PENDING: &str = r#"{"error":"authorization_pending"}"#;

    fn id() -> ClientId {
        ClientId::new("cid").unwrap()
    }

    fn device() -> DeviceCode {
        serde_json::from_str(
            r#"{"user_code":"AB12","device_code":"dc","verification_uri":"https://microsoft.com/link","expires_in":900,"interval":5,"message":"go"}"#,
        )
        .unwrap()
    }

    #[test]
    fn empty_client_id_rejected() {
        assert!(matches!(
            ClientId::new(" "),
            Err(AuthError::MissingClientId)
        ));
    }

    #[test]
    fn device_code_request_sends_scope() {
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"AB12","device_code":"dc","verification_uri":"u","expires_in":900}"#,
        );
        let code = request_device_code(&http, &id()).unwrap();
        assert_eq!(code.interval, 5);
        let body = http.seen.borrow()[0].body.clone().unwrap().text;
        assert!(body.contains("scope=XboxLive.signin%20offline_access"));
        assert!(body.contains("client_id=cid"));
    }

    #[test]
    fn pending_then_approved() {
        let http = MockHttp::default();
        http.push("token", 400, PENDING)
            .push("token", 400, PENDING)
            .push("token", 200, TOKENS);
        let mut sleeps = 0;
        let t = poll_for_tokens(&http, &id(), &device(), |_| sleeps += 1).unwrap();
        assert_eq!(t.access_token.expose(), "AT");
        assert_eq!(sleeps, 3);
    }

    #[test]
    fn declined_and_expired() {
        let http = MockHttp::default();
        http.push("token", 400, r#"{"error":"authorization_declined"}"#);
        assert!(matches!(
            poll_for_tokens(&http, &id(), &device(), |_| {}),
            Err(AuthError::LoginDeclined)
        ));
        http.push("token", 400, r#"{"error":"expired_token"}"#);
        assert!(matches!(
            poll_for_tokens(&http, &id(), &device(), |_| {}),
            Err(AuthError::LoginExpired)
        ));
    }

    #[test]
    fn polling_gives_up_after_expiry() {
        let http = MockHttp::default();
        for _ in 0..180 {
            http.push("token", 400, PENDING);
        }
        assert!(matches!(
            poll_for_tokens(&http, &id(), &device(), |_| {}),
            Err(AuthError::LoginExpired)
        ));
    }

    #[test]
    fn refresh_ok_and_rejected() {
        let http = MockHttp::default();
        http.push("token", 200, TOKENS);
        let t = refresh(&http, &id(), &Secret::new("old")).unwrap();
        assert_eq!(t.refresh_token.expose(), "RT");
        http.push(
            "token",
            400,
            r#"{"error":"invalid_grant","error_description":"x"}"#,
        );
        assert!(matches!(
            refresh(&http, &id(), &Secret::new("old")),
            Err(AuthError::RefreshRejected)
        ));
    }
}
