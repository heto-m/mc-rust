use serde::Deserialize;
use serde_json::json;

use crate::error::AuthError;
use crate::http::{HttpClient, Request};
use crate::secret::Secret;

const USER_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";

#[derive(Debug, Clone)]
pub struct XstsToken {
    pub token: Secret,
    pub user_hash: String,
}

impl XstsToken {
    pub fn identity_token(&self) -> Secret {
        Secret::new(format!(
            "XBL3.0 x={};{}",
            self.user_hash,
            self.token.expose()
        ))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxResponse {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Deserialize)]
struct XstsFailure {
    #[serde(rename = "XErr")]
    xerr: u64,
}

pub fn authenticate_user(http: &impl HttpClient, msa_access: &Secret) -> Result<Secret, AuthError> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", msa_access.expose()),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let response: XboxResponse = http
        .send(&Request::post_json(USER_AUTH_URL, &body))?
        .json(USER_AUTH_URL)?;
    Ok(Secret::new(response.token))
}

pub fn authorize_xsts(http: &impl HttpClient, user_token: &Secret) -> Result<XstsToken, AuthError> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [user_token.expose()],
        },
        "RelyingParty": MINECRAFT_RELYING_PARTY,
        "TokenType": "JWT",
    });
    let raw = http.send(&Request::post_json(XSTS_URL, &body))?;
    if !raw.is_success() {
        return Err(match serde_json::from_str::<XstsFailure>(&raw.body) {
            Ok(f) => xerr(f.xerr),
            Err(_) => AuthError::Status {
                status: raw.status,
                url: XSTS_URL.into(),
            },
        });
    }
    let response: XboxResponse = serde_json::from_str(&raw.body)?;
    let user_hash = response
        .display_claims
        .xui
        .into_iter()
        .next()
        .map(|x| x.uhs)
        .ok_or_else(|| AuthError::Key("xsts response has no uhs".into()))?;
    Ok(XstsToken {
        token: Secret::new(response.token),
        user_hash,
    })
}

fn xerr(code: u64) -> AuthError {
    match code {
        2148916233 => AuthError::NoXboxAccount,
        2148916235 => AuthError::XboxCountryBanned,
        2148916236 | 2148916237 => AuthError::XboxAdultVerification,
        2148916238 => AuthError::XboxChildAccount,
        other => AuthError::Xsts(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockHttp;

    const XSTS_OK: &str = r#"{"IssueInstant":"x","NotAfter":"y","Token":"XT","DisplayClaims":{"xui":[{"uhs":"UHS"}]}}"#;

    #[test]
    fn chain_builds_identity_token() {
        let http = MockHttp::default();
        http.push(
            "user.auth.xboxlive.com",
            200,
            r#"{"Token":"UT","DisplayClaims":{"xui":[{"uhs":"UHS"}]}}"#,
        )
        .push("xsts.auth.xboxlive.com", 200, XSTS_OK);
        let user = authenticate_user(&http, &Secret::new("msa")).unwrap();
        let xsts = authorize_xsts(&http, &user).unwrap();
        assert_eq!(xsts.identity_token().expose(), "XBL3.0 x=UHS;XT");
        let seen = http.seen.borrow();
        assert!(seen[0].body.as_ref().unwrap().text.contains("d=msa"));
        assert!(
            seen[1]
                .body
                .as_ref()
                .unwrap()
                .text
                .contains("rp://api.minecraftservices.com/")
        );
    }

    #[test]
    fn xerr_codes_are_typed() {
        for (code, want) in [
            (2148916233u64, "no Xbox account"),
            (2148916238, "child"),
            (2148916235, "country"),
            (2148916236, "adult"),
            (1, "xsts error 1"),
        ] {
            let http = MockHttp::default();
            http.push(
                "xsts",
                401,
                &format!(r#"{{"Identity":"0","XErr":{code},"Message":""}}"#),
            );
            let err = authorize_xsts(&http, &Secret::new("u")).unwrap_err();
            assert!(err.to_string().contains(want), "{err}");
        }
    }
}
