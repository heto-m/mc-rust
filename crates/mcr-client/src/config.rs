use std::time::Duration;

use mcr_auth::Secret;
use mcr_protocol::common::{ChatVisibility, MainHand, ParticleStatus};

#[derive(Debug, Clone)]
pub struct OnlineAuth {
    pub access_token: Secret,
    pub profile_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePackPolicy {
    Decline,
    Accept,
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
    pub online: Option<OnlineAuth>,
    /// Handshake with the transfer intent (set when following a transfer packet).
    pub transfer: bool,
    pub connect_timeout: Duration,
    pub join_timeout: Duration,
    pub chat_history: usize,
    pub chat_visibility: ChatVisibility,
    pub main_hand: MainHand,
    pub particle_status: ParticleStatus,
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
            online: None,
            transfer: false,
            connect_timeout: Duration::from_secs(10),
            join_timeout: Duration::from_secs(60),
            chat_history: 256,
            chat_visibility: ChatVisibility::Full,
            main_hand: MainHand::Right,
            particle_status: ParticleStatus::All,
        }
    }
}

impl ClientConfig {
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
