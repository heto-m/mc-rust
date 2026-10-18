use std::sync::Arc;
use std::time::Duration;

use mcr_auth::Secret;
use mcr_auth::chat::ServicesKeys;
use mcr_auth::keys::{KeyRefresh, PlayerKeys};
use mcr_protocol::common::{ChatVisibility, MainHand, ParticleStatus};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct OnlineAuth {
    pub access_token: Secret,
    pub profile_id: String,
    /// Chat signing keys; without them chat is unsigned.
    pub keys: Option<PlayerKeys>,
    /// Mojang's keys for checking other players' chat keys.
    pub services: Arc<ServicesKeys>,
    pub refresh: Option<KeyRefresh>,
}

impl OnlineAuth {
    pub fn new(access_token: Secret, profile_id: String) -> Self {
        Self {
            access_token,
            profile_id,
            keys: None,
            services: Arc::default(),
            refresh: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePackPolicy {
    Decline,
    Accept,
}

/// Which protocol the connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WireChoice {
    #[default]
    Canonical,
    /// A protocol number already known, e.g. from the server list's ping.
    Protocol(i32),
    /// Ping the server's status first and use the protocol it reports.
    Auto,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub view_distance: i8,
    pub language: String,
    pub brand: String,
    /// Reply `minecraft:core` to the known-packs request (the server then omits vanilla registry data).
    pub known_packs: bool,
    pub resource_packs: ResourcePackPolicy,
    /// When set, pack pushes go to the app instead of being answered by `resource_packs`.
    pub pack_link: Option<crate::PackLink>,
    /// When set, server dialogs go to the app, which answers through the same link.
    pub dialog_link: Option<crate::DialogLink>,
    /// When set, an unaccepted code of conduct goes to the app, which answers through the same link.
    pub conduct_link: Option<crate::ConductLink>,
    /// Hash of the code of conduct this server's entry already accepted.
    pub accepted_conduct: Option<i32>,
    pub online: Option<OnlineAuth>,
    /// Handshake with the transfer intent (set when following a transfer packet).
    pub transfer: bool,
    /// Cookies carried over from the server that transferred us.
    pub cookies: FxHashMap<String, Vec<u8>>,
    pub connect_timeout: Duration,
    pub join_timeout: Duration,
    pub chat_history: usize,
    pub chat_visibility: ChatVisibility,
    pub main_hand: MainHand,
    pub particle_status: ParticleStatus,
    /// Vanilla's Only Show Secure Chat option.
    pub only_show_secure_chat: bool,
    /// The managed singleplayer server: the player's own messages count as secure, as vanilla's integrated server.
    pub local_server: bool,
    /// Fills registry entries the server left out because they come from a known pack.
    pub builtin_data: Option<crate::BuiltinData>,
    /// Draws chat as the player sees it, for the secure chat check; without it translated lines count as modified.
    pub text_renderer: Option<crate::TextRenderer>,
    pub wire: WireChoice,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 25565,
            name: "Bot".into(),
            view_distance: 10,
            language: "en_us".into(),
            brand: "mcr".into(),
            known_packs: true,
            resource_packs: ResourcePackPolicy::Decline,
            pack_link: None,
            dialog_link: None,
            conduct_link: None,
            accepted_conduct: None,
            online: None,
            transfer: false,
            cookies: FxHashMap::default(),
            connect_timeout: Duration::from_secs(10),
            join_timeout: Duration::from_secs(60),
            chat_history: 256,
            chat_visibility: ChatVisibility::Full,
            main_hand: MainHand::Right,
            particle_status: ParticleStatus::All,
            only_show_secure_chat: false,
            local_server: false,
            builtin_data: None,
            text_renderer: None,
            wire: WireChoice::Canonical,
        }
    }
}

/// Where a `transfer` packet sends the player, with the cookies vanilla keeps across the hop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTarget {
    pub host: String,
    pub port: u16,
    pub cookies: FxHashMap<String, Vec<u8>>,
}

impl ClientConfig {
    /// This config pointed at the transfer target with the transfer intent.
    pub fn for_transfer(&self, target: &TransferTarget) -> Self {
        let mut cfg = self.clone();
        cfg.host = target.host.clone();
        cfg.port = target.port;
        cfg.transfer = true;
        cfg.cookies = target.cookies.clone();
        if cfg.wire != WireChoice::Canonical {
            cfg.wire = WireChoice::Auto;
        }
        cfg
    }

    pub fn with_addr(mut self, addr: &str) -> Self {
        match addr.rsplit_once(':') {
            Some((host, port)) if port.parse::<u16>().is_ok() => {
                self.host = host.to_owned();
                self.port = port.parse().unwrap_or(25565);
            }
            _ => self.host = addr.to_owned(),
        }
        self
    }
}
