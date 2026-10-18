use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::AuthError;
use crate::http::HttpClient;
use crate::keys::PlayerKeys;
use crate::minecraft::{self, Profile};
use crate::msa::{self, ClientId, DeviceCode, MsaTokens};
use crate::secret::Secret;
use crate::xbox;

const EXPIRY_MARGIN_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub msa_refresh_token: Secret,
    pub minecraft_token: Secret,
    pub minecraft_expires_at: u64,
    pub profile: Profile,
    #[serde(default)]
    pub keys: Option<PlayerKeys>,
}

impl Account {
    pub fn token_valid(&self, now: u64) -> bool {
        self.minecraft_expires_at > now + EXPIRY_MARGIN_SECS
    }

    pub fn load(path: &Path) -> Result<Option<Self>, AuthError> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str(&text).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), AuthError> {
        let tmp = path.with_extension("tmp");
        let _ = fs::remove_file(&tmp);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, &serde_json::to_vec_pretty(self)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Authenticator<H> {
    http: H,
    client_id: ClientId,
    cache_path: Option<PathBuf>,
}

impl<H: HttpClient> Authenticator<H> {
    pub fn new(http: H, client_id: ClientId) -> Self {
        Self {
            http,
            client_id,
            cache_path: None,
        }
    }

    pub fn with_cache(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_path = Some(path.into());
        self
    }

    pub fn http(&self) -> &H {
        &self.http
    }

    pub fn ensure_account(
        &self,
        now: u64,
        prompt: impl FnOnce(&DeviceCode),
        sleep: impl FnMut(Duration),
    ) -> Result<Account, AuthError> {
        let cached = match &self.cache_path {
            Some(path) => Account::load(path)?,
            None => None,
        };
        let mut account = match cached {
            Some(a) if a.token_valid(now) => a,
            Some(a) => match self.refresh_account(&a, now) {
                Ok(fresh) => fresh,
                Err(AuthError::RefreshRejected) => self.device_login(now, prompt, sleep)?,
                Err(e) => return Err(e),
            },
            None => self.device_login(now, prompt, sleep)?,
        };
        self.refresh_keys_if_needed(&mut account, now)?;
        if let Some(path) = &self.cache_path {
            account.save(path)?;
        }
        Ok(account)
    }

    /// A new device-code sign-in that ignores the cache; `wait` returning `false` cancels polling.
    /// The account is saved to the cache path when one is set.
    pub fn sign_in(
        &self,
        now: u64,
        prompt: impl FnOnce(&DeviceCode),
        wait: impl FnMut(Duration) -> bool,
    ) -> Result<Account, AuthError> {
        let device = msa::request_device_code(&self.http, &self.client_id)?;
        prompt(&device);
        let tokens = msa::poll_until(&self.http, &self.client_id, &device, wait)?;
        let mut account = self.minecraft_account(tokens, now, None)?;
        self.refresh_keys_if_needed(&mut account, now)?;
        if let Some(path) = &self.cache_path {
            account.save(path)?;
        }
        Ok(account)
    }

    /// The cached account with a usable token, refreshing it if expired; never starts a device login.
    pub fn refresh_cached(&self, now: u64) -> Result<Account, AuthError> {
        let path = self.cache_path.as_ref().ok_or(AuthError::NotSignedIn)?;
        let cached = Account::load(path)?.ok_or(AuthError::NotSignedIn)?;
        let mut account = if cached.token_valid(now) {
            cached
        } else {
            self.refresh_account(&cached, now)?
        };
        self.refresh_keys_if_needed(&mut account, now)?;
        account.save(path)?;
        Ok(account)
    }

    fn device_login(
        &self,
        now: u64,
        prompt: impl FnOnce(&DeviceCode),
        sleep: impl FnMut(Duration),
    ) -> Result<Account, AuthError> {
        let device = msa::request_device_code(&self.http, &self.client_id)?;
        prompt(&device);
        let tokens = msa::poll_for_tokens(&self.http, &self.client_id, &device, sleep)?;
        self.minecraft_account(tokens, now, None)
    }

    fn refresh_account(&self, old: &Account, now: u64) -> Result<Account, AuthError> {
        let tokens = msa::refresh(&self.http, &self.client_id, &old.msa_refresh_token)?;
        self.minecraft_account(tokens, now, old.keys.clone())
    }

    fn minecraft_account(
        &self,
        msa: MsaTokens,
        now: u64,
        keys: Option<PlayerKeys>,
    ) -> Result<Account, AuthError> {
        let user = xbox::authenticate_user(&self.http, &msa.access_token)?;
        let xsts = xbox::authorize_xsts(&self.http, &user)?;
        let mc = minecraft::login_with_xbox(&self.http, &xsts)?;
        minecraft::check_entitlements(&self.http, &mc.access_token)?;
        let profile = minecraft::fetch_profile(&self.http, &mc.access_token)?;
        Ok(Account {
            msa_refresh_token: msa.refresh_token,
            minecraft_token: mc.access_token,
            minecraft_expires_at: now + mc.expires_in,
            profile,
            keys,
        })
    }

    fn refresh_keys_if_needed(&self, account: &mut Account, now: u64) -> Result<(), AuthError> {
        let stale = account.keys.as_ref().is_none_or(|k| k.should_refresh(now));
        if stale {
            account.keys = Some(minecraft::fetch_player_keys(
                &self.http,
                &account.minecraft_token,
            )?);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockHttp;
    use crate::keys::tests::fixture_json;

    const NOW: u64 = 1_800_000_000;
    const MSA: &str = r#"{"expires_in":3600,"access_token":"AT","refresh_token":"RT2"}"#;
    const XBL: &str = r#"{"Token":"UT","DisplayClaims":{"xui":[{"uhs":"UHS"}]}}"#;
    const MC: &str = r#"{"access_token":"MC","expires_in":86400}"#;
    const ENT: &str = r#"{"items":[{"name":"game_minecraft"}]}"#;
    const PROFILE: &str = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;

    fn script_chain(http: &MockHttp) {
        http.push("user.auth.xboxlive", 200, XBL)
            .push("xsts", 200, XBL)
            .push("login_with_xbox", 200, MC)
            .push("mcstore", 200, ENT)
            .push("minecraft/profile", 200, PROFILE);
    }

    fn keys_json() -> String {
        fixture_json("2030-01-01T00:00:00Z", "2030-01-01T00:00:00Z")
    }

    fn auth(http: MockHttp, path: &Path) -> Authenticator<MockHttp> {
        Authenticator::new(http, ClientId::new("cid").unwrap()).with_cache(path)
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-auth-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("account.json")
    }

    #[test]
    fn full_login_then_cached_without_network() {
        let path = temp_path("full");
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
        )
        .push("oauth2/v2.0/token", 200, MSA);
        script_chain(&http);
        http.push("player/certificates", 200, &keys_json());
        let a = auth(http, &path);
        let mut prompted = false;
        let acct = a.ensure_account(NOW, |_| prompted = true, |_| {}).unwrap();
        assert!(prompted);
        assert_eq!(acct.profile.name, "Notch");
        assert!(acct.keys.is_some());

        let again = a
            .ensure_account(NOW + 10, |_| panic!("no prompt"), |_| {})
            .unwrap();
        assert_eq!(again.minecraft_token, acct.minecraft_token);
        assert_eq!(a.http().seen.borrow().len(), 8);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn expired_token_refreshes_and_rewrites_cache() {
        let path = temp_path("refresh");
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
        )
        .push("oauth2/v2.0/token", 200, MSA);
        script_chain(&http);
        http.push("player/certificates", 200, &keys_json());
        let a = auth(http, &path);
        a.ensure_account(NOW, |_| {}, |_| {}).unwrap();

        a.http().push("oauth2/v2.0/token", 200, MSA);
        script_chain(a.http());
        let later = NOW + 86400 + 5;
        let acct = a
            .ensure_account(later, |_| panic!("no prompt"), |_| {})
            .unwrap();
        assert_eq!(acct.minecraft_expires_at, later + 86400);
        let saved = Account::load(&path).unwrap().unwrap();
        assert_eq!(saved.minecraft_expires_at, later + 86400);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(
            a.http()
                .seen
                .borrow()
                .last()
                .unwrap()
                .url
                .contains("profile")
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn rejected_refresh_falls_back_to_device_login() {
        let path = temp_path("fallback");
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
        )
        .push("oauth2/v2.0/token", 200, MSA);
        script_chain(&http);
        http.push("player/certificates", 200, &keys_json());
        let a = auth(http, &path);
        a.ensure_account(NOW, |_| {}, |_| {}).unwrap();

        a.http()
            .push("oauth2/v2.0/token", 400, r#"{"error":"invalid_grant"}"#)
            .push(
                "devicecode",
                200,
                r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
            )
            .push("oauth2/v2.0/token", 200, MSA);
        script_chain(a.http());
        a.http().push("player/certificates", 200, &keys_json());
        let mut prompted = false;
        a.ensure_account(NOW + 100_000, |_| prompted = true, |_| {})
            .unwrap();
        assert!(prompted);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn refresh_cached_never_prompts() {
        let path = temp_path("noprompt");
        let _ = fs::remove_file(&path);
        let a = auth(MockHttp::default(), &path);
        assert!(matches!(a.refresh_cached(NOW), Err(AuthError::NotSignedIn)));

        a.http()
            .push(
                "devicecode",
                200,
                r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
            )
            .push("oauth2/v2.0/token", 200, MSA);
        script_chain(a.http());
        a.http().push("player/certificates", 200, &keys_json());
        let signed = a.sign_in(NOW, |_| {}, |_| true).unwrap();
        assert_eq!(Account::load(&path).unwrap().unwrap().profile.name, "Notch");

        let seen = a.http().seen.borrow().len();
        let same = a.refresh_cached(NOW + 10).unwrap();
        assert_eq!(same.minecraft_token, signed.minecraft_token);
        assert_eq!(a.http().seen.borrow().len(), seen);

        a.http()
            .push("oauth2/v2.0/token", 400, r#"{"error":"invalid_grant"}"#);
        assert!(matches!(
            a.refresh_cached(NOW + 100_000),
            Err(AuthError::RefreshRejected)
        ));
        assert!(
            !a.http()
                .seen
                .borrow()
                .iter()
                .skip(seen)
                .any(|r| r.url.contains("devicecode"))
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn cancelled_sign_in_saves_nothing() {
        let path = temp_path("cancel");
        let _ = fs::remove_file(&path);
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
        );
        let a = auth(http, &path);
        assert!(matches!(
            a.sign_in(NOW, |_| {}, |_| false),
            Err(AuthError::Cancelled)
        ));
        assert!(!path.exists());
    }

    #[test]
    fn debug_never_leaks_tokens() {
        let path = temp_path("debug");
        let http = MockHttp::default();
        http.push(
            "devicecode",
            200,
            r#"{"user_code":"U","device_code":"D","verification_uri":"v","expires_in":900}"#,
        )
        .push("oauth2/v2.0/token", 200, MSA);
        script_chain(&http);
        http.push("player/certificates", 200, &keys_json());
        let acct = auth(http, &path)
            .ensure_account(NOW, |_| {}, |_| {})
            .unwrap();
        let dbg = format!("{acct:?}");
        assert!(!dbg.contains("RT2") && !dbg.contains("\"MC\"") && !dbg.contains("PRIVATE"));
        let _ = fs::remove_file(&path);
    }
}
