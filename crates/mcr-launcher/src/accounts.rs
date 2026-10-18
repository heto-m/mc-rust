use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mcr_auth::msa::ClientId;
use mcr_auth::offline::validate_name;
use mcr_auth::{AuthError, Authenticator, HttpClient};
use serde_json::{Value, json};

use crate::launch::{LaunchAccount, OnlineSession};

/// Where Microsoft accounts' token files live, under the data dir.
pub const ACCOUNTS_DIR: &str = "accounts";
const WAIT_SLICE: Duration = Duration::from_millis(100);
const MAX_SHOWN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Offline,
    Microsoft,
}

/// An account as the launcher file lists it; a Microsoft account's tokens stay in its own file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub kind: AccountKind,
    pub name: String,
    /// The Minecraft profile id (32 hex digits), Microsoft accounts only.
    pub uuid: Option<String>,
}

impl AccountEntry {
    pub fn offline(name: &str) -> Result<Self, AuthError> {
        validate_name(name)?;
        Ok(Self {
            kind: AccountKind::Offline,
            name: name.to_owned(),
            uuid: None,
        })
    }

    pub fn microsoft(name: &str, uuid: &str) -> Option<Self> {
        (validate_name(name).is_ok() && valid_uuid(uuid)).then(|| Self {
            kind: AccountKind::Microsoft,
            name: name.to_owned(),
            uuid: Some(uuid.to_ascii_lowercase()),
        })
    }

    /// Identifies the account in the file: the UUID, or `offline:<name>` in lowercase.
    pub fn key(&self) -> String {
        match &self.uuid {
            Some(uuid) => uuid.clone(),
            None => format!("offline:{}", self.name.to_ascii_lowercase()),
        }
    }

    pub fn token_file(&self, data_dir: &Path) -> Option<PathBuf> {
        let uuid = self.uuid.as_deref().filter(|u| valid_uuid(u))?;
        Some(data_dir.join(ACCOUNTS_DIR).join(format!("{uuid}.json")))
    }

    pub(crate) fn from_json(v: &Value) -> Option<Self> {
        let name = v["name"].as_str()?;
        match v["kind"].as_str()? {
            "offline" => Self::offline(name).ok(),
            "microsoft" => Self::microsoft(name, v["uuid"].as_str()?),
            _ => None,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        match self.kind {
            AccountKind::Offline => json!({"kind": "offline", "name": self.name}),
            AccountKind::Microsoft => {
                json!({"kind": "microsoft", "name": self.name, "uuid": self.uuid})
            }
        }
    }

    /// Ready to launch: offline at once, Microsoft refreshed without ever prompting.
    pub fn launch_account(
        &self,
        http: impl HttpClient,
        client_id: Option<ClientId>,
        data_dir: &Path,
        now: u64,
    ) -> Result<LaunchAccount, AuthError> {
        let Some(path) = self.token_file(data_dir) else {
            return Ok(LaunchAccount {
                name: self.name.clone(),
                online: None,
            });
        };
        let client_id = client_id.ok_or(AuthError::MissingClientId)?;
        let account = Authenticator::new(http, client_id)
            .with_cache(path)
            .refresh_cached(now)?;
        Ok(LaunchAccount {
            name: account.profile.name.clone(),
            online: Some(OnlineSession {
                access_token: account.minecraft_token,
                profile_id: account.profile.id,
            }),
        })
    }

    /// Removes a Microsoft account's token file.
    pub fn forget(&self, data_dir: &Path) -> std::io::Result<()> {
        match self.token_file(data_dir).map(std::fs::remove_file) {
            Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

pub fn valid_uuid(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What a sign-in reports to the UI, in order: a code, then success or failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInEvent {
    Code {
        user_code: String,
        verification_uri: String,
        expires_in: u64,
    },
    Done(AccountEntry),
    Failed(String),
}

/// A Microsoft device-code sign-in on its own thread.
#[derive(Debug)]
pub struct SignIn {
    cancel: Arc<AtomicBool>,
    events: Receiver<SignInEvent>,
    thread: Option<JoinHandle<()>>,
}

impl SignIn {
    /// The account file is written to `<data dir>/accounts/<uuid>.json` (mode 0600) before `Done`.
    pub fn start<H: HttpClient + Send + 'static>(
        http: H,
        client_id: ClientId,
        data_dir: PathBuf,
        now: u64,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, events) = channel();
        let flag = cancel.clone();
        let thread = std::thread::spawn(move || {
            let code_tx = tx.clone();
            let result = sign_in(&http, client_id, &data_dir, now, &flag, move |code| {
                let _ = code_tx.send(code);
            });
            let _ = tx.send(match result {
                Ok(entry) => SignInEvent::Done(entry),
                Err(AuthError::Cancelled) => return,
                Err(e) => SignInEvent::Failed(crate::clean(&e.to_string(), MAX_SHOWN)),
            });
        });
        Self {
            cancel,
            events,
            thread: Some(thread),
        }
    }

    pub fn poll(&mut self) -> Option<SignInEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                if let Some(t) = self.thread.take() {
                    let _ = t.join();
                }
                None
            }
        }
    }

    /// Stops polling at the next wait; no account is added.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn sign_in(
    http: &impl HttpClient,
    client_id: ClientId,
    data_dir: &Path,
    now: u64,
    cancel: &AtomicBool,
    report: impl FnOnce(SignInEvent),
) -> Result<AccountEntry, AuthError> {
    let auth = Authenticator::new(Borrowed(http), client_id);
    let account = auth.sign_in(
        now,
        |code| {
            report(SignInEvent::Code {
                user_code: crate::clean(&code.user_code, 32),
                verification_uri: crate::clean(&code.verification_uri, MAX_SHOWN),
                expires_in: code.expires_in,
            });
        },
        |wait| {
            let until = Instant::now() + wait;
            while Instant::now() < until {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                std::thread::sleep(WAIT_SLICE.min(until - Instant::now()));
            }
            !cancel.load(Ordering::Relaxed)
        },
    )?;
    let entry = AccountEntry::microsoft(&account.profile.name, &account.profile.id)
        .ok_or_else(|| AuthError::OAuth("the Minecraft profile is malformed".into()))?;
    let path = entry.token_file(data_dir).expect("validated uuid");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    account.save(&path)?;
    Ok(entry)
}

struct Borrowed<'a, H>(&'a H);

impl<H: HttpClient> HttpClient for Borrowed<'_, H> {
    fn send(
        &self,
        request: &mcr_auth::http::Request,
    ) -> Result<mcr_auth::http::Response, AuthError> {
        self.0.send(request)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use mcr_auth::http::{Request, Response};

    use super::*;

    type Script = VecDeque<(&'static str, u16, String)>;

    /// Answers requests in order from a script; never touches the network.
    #[derive(Debug, Default, Clone)]
    pub struct Scripted {
        script: Arc<Mutex<Script>>,
        pub seen: Arc<Mutex<Vec<String>>>,
    }

    impl Scripted {
        pub fn push(&self, needle: &'static str, status: u16, body: &str) -> &Self {
            self.script
                .lock()
                .unwrap()
                .push_back((needle, status, body.to_owned()));
            self
        }
    }

    impl HttpClient for Scripted {
        fn send(&self, request: &Request) -> Result<Response, AuthError> {
            self.seen.lock().unwrap().push(request.url.clone());
            let next = self.script.lock().unwrap().pop_front();
            let Some((needle, status, body)) = next else {
                return Err(AuthError::Transport(format!("unscripted {}", request.url)));
            };
            assert!(
                request.url.contains(needle),
                "expected {needle}, got {}",
                request.url
            );
            Ok(Response { status, body })
        }
    }

    pub const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    const DEVICE: &str = r#"{"user_code":"ABCD-1234","device_code":"D","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":1}"#;
    const MSA: &str = r#"{"expires_in":3600,"access_token":"AT","refresh_token":"RT-SECRET"}"#;
    const XBL: &str = r#"{"Token":"UT","DisplayClaims":{"xui":[{"uhs":"UHS"}]}}"#;
    const MC: &str = r#"{"access_token":"MC-SECRET","expires_in":86400}"#;

    pub fn script_chain(http: &Scripted) {
        let keys = certificates();
        http.push("user.auth.xboxlive", 200, XBL)
            .push("xsts", 200, XBL)
            .push("login_with_xbox", 200, MC)
            .push("mcstore", 200, r#"{"items":[{"name":"game_minecraft"}]}"#)
            .push(
                "minecraft/profile",
                200,
                &format!(r#"{{"id":"{UUID}","name":"Notch"}}"#),
            )
            .push("player/certificates", 200, &keys);
    }

    fn certificates() -> String {
        use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding};
        let private = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024).unwrap();
        let public = rsa::RsaPublicKey::from(&private);
        json!({
            "keyPair": {
                "privateKey": private.to_pkcs1_pem(LineEnding::LF).unwrap().to_string(),
                "publicKey": public.to_pkcs1_pem(LineEnding::LF).unwrap(),
            },
            "publicKeySignature": "AAEC",
            "publicKeySignatureV2": "AwQF",
            "expiresAt": "2099-01-01T00:00:00Z",
            "refreshedAfter": "2099-01-01T00:00:00Z",
        })
        .to_string()
    }

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-launcher-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait_event(sign: &mut SignIn) -> SignInEvent {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(e) = sign.poll() {
                return e;
            }
            assert!(Instant::now() < deadline, "no sign-in event");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn offline_names_validated_like_vanilla() {
        assert!(AccountEntry::offline("Steve_01").is_ok());
        for bad in ["", "bad name!", "seventeen_chars_x", "naïve"] {
            assert!(AccountEntry::offline(bad).is_err(), "{bad}");
        }
        let steve = AccountEntry::offline("Steve").unwrap();
        assert_eq!(steve.key(), "offline:steve");
        assert!(steve.token_file(Path::new("/d")).is_none());
        assert!(AccountEntry::microsoft("Notch", "../../x").is_none());
    }

    #[test]
    fn sign_in_shows_code_then_saves_private_file() {
        let dir = temp_dir("signin");
        let http = Scripted::default();
        http.push("devicecode", 200, DEVICE)
            .push(
                "oauth2/v2.0/token",
                400,
                r#"{"error":"authorization_pending"}"#,
            )
            .push("oauth2/v2.0/token", 200, MSA);
        script_chain(&http);
        let mut sign = SignIn::start(
            http.clone(),
            ClientId::new("cid").unwrap(),
            dir.clone(),
            1_800_000_000,
        );
        assert_eq!(
            wait_event(&mut sign),
            SignInEvent::Code {
                user_code: "ABCD-1234".into(),
                verification_uri: "https://www.microsoft.com/link".into(),
                expires_in: 900
            }
        );
        let SignInEvent::Done(entry) = wait_event(&mut sign) else {
            panic!("sign-in failed");
        };
        assert_eq!(entry, AccountEntry::microsoft("Notch", UUID).unwrap());
        let path = entry.token_file(&dir).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("RT-SECRET"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(!format!("{entry:?}").contains("SECRET"));

        let launch = entry
            .launch_account(http.clone(), ClientId::new("cid").ok(), &dir, 1_800_000_100)
            .unwrap();
        assert_eq!(launch.name, "Notch");
        let online = launch.online.unwrap();
        assert_eq!(online.access_token.expose(), "MC-SECRET");
        assert_eq!(online.profile_id, UUID);
        assert!(!format!("{online:?}").contains("SECRET"));

        entry.forget(&dir).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_stops_polling_without_an_account() {
        let dir = temp_dir("cancel");
        let http = Scripted::default();
        http.push("devicecode", 200, DEVICE);
        for _ in 0..1000 {
            http.push(
                "oauth2/v2.0/token",
                400,
                r#"{"error":"authorization_pending"}"#,
            );
        }
        let mut sign = SignIn::start(http.clone(), ClientId::new("cid").unwrap(), dir.clone(), 0);
        assert!(matches!(wait_event(&mut sign), SignInEvent::Code { .. }));
        sign.cancel();
        std::thread::sleep(Duration::from_millis(1500));
        let requests = http.seen.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(http.seen.lock().unwrap().len(), requests);
        assert!(sign.poll().is_none());
        assert!(!dir.join(ACCOUNTS_DIR).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn declined_sign_in_fails() {
        let dir = temp_dir("declined");
        let http = Scripted::default();
        http.push("devicecode", 200, DEVICE).push(
            "oauth2/v2.0/token",
            400,
            r#"{"error":"authorization_declined"}"#,
        );
        let mut sign = SignIn::start(http, ClientId::new("cid").unwrap(), dir.clone(), 0);
        assert!(matches!(wait_event(&mut sign), SignInEvent::Code { .. }));
        assert!(matches!(wait_event(&mut sign), SignInEvent::Failed(m) if m.contains("declined")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_without_client_id_or_file_fails() {
        let dir = temp_dir("nofile");
        let entry = AccountEntry::microsoft("Notch", UUID).unwrap();
        assert!(matches!(
            entry.launch_account(Scripted::default(), None, &dir, 0),
            Err(AuthError::MissingClientId)
        ));
        assert!(matches!(
            entry.launch_account(Scripted::default(), ClientId::new("cid").ok(), &dir, 0),
            Err(AuthError::NotSignedIn)
        ));
        let offline = AccountEntry::offline("Steve").unwrap();
        let launch = offline
            .launch_account(Scripted::default(), None, &dir, 0)
            .unwrap();
        assert_eq!(
            (launch.name.as_str(), launch.online.is_none()),
            ("Steve", true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
