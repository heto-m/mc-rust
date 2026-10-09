use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::error::AuthError;
use crate::http::{HttpClient, Request};
use crate::keys::PlayerKeys;
use crate::secret::Secret;
use crate::xbox::XstsToken;

const LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const CERTIFICATES_URL: &str = "https://api.minecraftservices.com/player/certificates";

#[derive(Debug, Clone)]
pub struct MinecraftToken {
    pub access_token: Secret,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Skin>,
    #[serde(default)]
    pub capes: Vec<Cape>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skin {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

impl Profile {
    pub fn uuid(&self) -> Result<Uuid, AuthError> {
        Uuid::parse_str(&self.id).map_err(|e| AuthError::Key(format!("profile id: {e}")))
    }
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    items: Vec<EntitlementItem>,
}

#[derive(Deserialize)]
struct EntitlementItem {
    name: String,
}

pub fn login_with_xbox(
    http: &impl HttpClient,
    xsts: &XstsToken,
) -> Result<MinecraftToken, AuthError> {
    let body = json!({ "identityToken": xsts.identity_token().expose() });
    let r: LoginResponse = http
        .send(&Request::post_json(LOGIN_URL, &body))?
        .json(LOGIN_URL)?;
    Ok(MinecraftToken {
        access_token: Secret::new(r.access_token),
        expires_in: r.expires_in,
    })
}

pub fn check_entitlements(http: &impl HttpClient, token: &Secret) -> Result<(), AuthError> {
    let request = Request::get(ENTITLEMENTS_URL).bearer(token.expose());
    let e: Entitlements = http.send(&request)?.json(ENTITLEMENTS_URL)?;
    let owned = e
        .items
        .iter()
        .any(|i| i.name == "game_minecraft" || i.name == "product_minecraft");
    if owned {
        Ok(())
    } else {
        Err(AuthError::NotOwned)
    }
}

pub fn fetch_profile(http: &impl HttpClient, token: &Secret) -> Result<Profile, AuthError> {
    let response = http.send(&Request::get(PROFILE_URL).bearer(token.expose()))?;
    if response.status == 404 {
        return Err(AuthError::NotOwned);
    }
    response.json(PROFILE_URL)
}

pub fn fetch_player_keys(http: &impl HttpClient, token: &Secret) -> Result<PlayerKeys, AuthError> {
    let mut request = Request::post_json(CERTIFICATES_URL, &json!({})).bearer(token.expose());
    request.body = None;
    let response = http.send(&request)?;
    if !response.is_success() {
        return Err(AuthError::Status {
            status: response.status,
            url: CERTIFICATES_URL.into(),
        });
    }
    PlayerKeys::from_response(&response.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockHttp;

    const PROFILE: &str = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[{"id":"s1","state":"ACTIVE","url":"http://x/skin","variant":"CLASSIC"}],"capes":[]}"#;

    #[test]
    fn login_and_profile() {
        let http = MockHttp::default();
        http.push(
            "login_with_xbox",
            200,
            r#"{"username":"u","access_token":"MC","token_type":"Bearer","expires_in":86400}"#,
        )
        .push("minecraft/profile", 200, PROFILE);
        let xsts = XstsToken {
            token: Secret::new("XT"),
            user_hash: "UHS".into(),
        };
        let token = login_with_xbox(&http, &xsts).unwrap();
        assert_eq!(token.expires_in, 86400);
        assert!(
            http.seen.borrow()[0]
                .body
                .as_ref()
                .unwrap()
                .text
                .contains("XBL3.0 x=UHS;XT")
        );
        let profile = fetch_profile(&http, &token.access_token).unwrap();
        assert_eq!(profile.name, "Notch");
        assert_eq!(profile.skins[0].variant.as_deref(), Some("CLASSIC"));
        assert_eq!(
            profile.uuid().unwrap().to_string(),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
        assert_eq!(http.seen.borrow()[1].headers.last().unwrap().1, "Bearer MC");
    }

    #[test]
    fn entitlements() {
        let http = MockHttp::default();
        http.push(
            "mcstore",
            200,
            r#"{"items":[{"name":"product_minecraft"},{"name":"game_minecraft"}]}"#,
        )
        .push("mcstore", 200, r#"{"items":[]}"#);
        let t = Secret::new("t");
        check_entitlements(&http, &t).unwrap();
        assert!(matches!(
            check_entitlements(&http, &t),
            Err(AuthError::NotOwned)
        ));
    }

    #[test]
    fn missing_profile_means_not_owned() {
        let http = MockHttp::default();
        http.push("profile", 404, r#"{"error":"NOT_FOUND"}"#);
        assert!(matches!(
            fetch_profile(&http, &Secret::new("t")),
            Err(AuthError::NotOwned)
        ));
    }
}
