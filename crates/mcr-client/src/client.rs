use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use mcr_buf::{ByteArray, Identifier, RestBytes};
use mcr_data::{Direction, ProtocolState};
use mcr_net::{ConnectOptions, Connection, Event, InboundFrame, Switch};
use mcr_physics::{MoveKind, PlayerInput};
use mcr_protocol::PacketIds;
use mcr_protocol::common as c;
use mcr_protocol::configuration as cfg;
use mcr_protocol::dispatch::{
    ConfigClientbound, ConfigServerbound, HandshakeServerbound, LoginClientbound, LoginServerbound,
    PlayClientbound, PlayServerbound,
};
use mcr_protocol::handshake::{Intent, Intention};
use mcr_protocol::login as l;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::Component;
use mcr_protocol::version::{Session, Translated, Wire};
use rustc_hash::FxHashSet;
use uuid::Uuid;

use crate::config::{ClientConfig, WireChoice};
use crate::error::{ClientError, Result};
use crate::events::{ClientEvent, DisconnectReason};
use crate::handler::core_pack;
use crate::packs::{PackReply, PackRequest};
use crate::relay::{ClientHooks, LocalAction, Relay, Relayed};
use crate::state::{ClientState, Outbox, resource_pack_responses};

const MAX_FRAMES_PER_PUMP: usize = 8192;
const LOAD_TIMEOUT_TICKS: u32 = 600;
const MAX_PENDING_PONGS: usize = 64;
const MAX_SKIP_LOGS: u64 = 16;
/// Longest wait for the mirror's registries at the end of configuration.
const FINISH_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Login = 0,
    Configuration = 1,
    Play = 2,
    Closed = 3,
}

/// Vanilla `EntityEvent.USE_ITEM_COMPLETE`: the server finished the local player's item use.
const USE_ITEM_COMPLETE: i8 = 9;

#[derive(Debug)]
pub struct Client {
    conn: Connection,
    ids: &'static PacketIds,
    wire: Wire,
    /// Translator state and translated payloads; all stay empty on the canonical wire.
    session: Session,
    inbound_buf: Vec<u8>,
    outbound_buf: Vec<u8>,
    translate_warned: FxHashSet<(u8, i32)>,
    pub config: ClientConfig,
    pub state: ClientState,
    phase: Phase,
    shared_phase: Arc<AtomicU8>,
    events: VecDeque<ClientEvent>,
    scratch: Vec<u8>,
    pub(crate) sequence: i32,
    transfer: Option<(String, u16)>,
    /// Configuration pings the mirror answers, so its custom payloads reach the server before the pong.
    pending_pongs: Vec<i32>,
    /// The server finished configuration while the mirror was still preparing its registries.
    finish_pending: Option<Instant>,
    /// Play packets skipped in mod mode because a mod's content made them undecodable to Rust.
    skipped_play: u64,
    pusher_scratch: Vec<[f64; 3]>,
    joined: bool,
    conduct_pending: bool,
    pub ticks: u64,
    pub view: (f32, f32),
    pub(crate) digging: Option<crate::actions::Digging>,
    pub(crate) destroy_delay: u8,
    pub(crate) using: Option<crate::use_item::UsingItem>,
    pub(crate) ride: Option<crate::riding::Ride>,
    pub(crate) combat: crate::combat::Combat,
    relay: Option<Box<dyn Relay>>,
    #[cfg(test)]
    pub(crate) sent: Vec<(i32, Vec<u8>)>,
}

const MAX_CUSTOM_ACTION_BYTES: usize = 65_600;

fn plain_json_text(json: &str) -> Component {
    for key in ["\"text\":\"", "\"translate\":\""] {
        if let Some(i) = json.find(key) {
            let rest = &json[i + key.len()..];
            if let Some(end) = rest.find('"') {
                return Component::text(&rest[..end]);
            }
        }
    }
    Component::text(json.trim_matches('"'))
}

impl Client {
    pub fn connect(config: ClientConfig) -> Result<Self> {
        Self::connect_hooked(config, ClientHooks::default())
    }

    /// Connects with a frame tap on the reader thread and a relay polled on the game thread.
    pub fn connect_hooked(config: ClientConfig, hooks: ClientHooks) -> Result<Self> {
        let ids = PacketIds::v26_3();
        let wire = match config.wire {
            WireChoice::Canonical => Wire::Canonical,
            WireChoice::Protocol(protocol) => crate::ServerVersion {
                name: mcr_protocol::version::version_by_protocol(protocol)
                    .map_or_else(|| protocol.to_string(), |v| v.name.to_owned()),
                protocol,
            }
            .wire()?,
            WireChoice::Auto => {
                crate::probe(&config.host, config.port, config.connect_timeout)?.wire()?
            }
        };
        if !wire.is_canonical() && (hooks.relay.is_some() || hooks.frame_tap.is_some()) {
            return Err(ClientError::ModModeNeedsCanonical(
                wire.version_name().to_owned(),
            ));
        }
        let phase = Arc::new(AtomicU8::new(Phase::Login as u8));
        let compression_id = ids
            .id_by_name(
                ProtocolState::Login,
                Direction::Clientbound,
                "login_compression",
            )
            .and_then(|id| wire.wire_clientbound_id(ProtocolState::Login, id))
            .ok_or_else(|| ClientError::Invalid("login_compression id".into()))?;
        let hook_phase = phase.clone();
        let hook = Box::new(move |id: i32, payload: &[u8]| {
            if hook_phase.load(Ordering::Acquire) != Phase::Login as u8 || id != compression_id {
                return None;
            }
            let mut p = payload;
            mcr_buf::read_varint(&mut p).ok().map(Switch::Compression)
        });
        let conn = Connection::connect_with(
            (config.host.as_str(), config.port),
            ConnectOptions {
                inbound_hook: Some(hook),
                frame_tap: hooks.frame_tap,
                connect_timeout: Some(config.connect_timeout),
            },
        );
        let mut relay = hooks.relay;
        let conn = match conn {
            Ok(conn) => conn,
            Err(e) => {
                if let Some(r) = relay.as_mut() {
                    r.closed(&e.to_string());
                }
                return Err(e.into());
            }
        };
        let (name, uuid) = match &config.online {
            Some(a) => (
                config.name.clone(),
                Uuid::parse_str(&a.profile_id).unwrap_or_else(|_| Uuid::nil()),
            ),
            None => (
                config.name.clone(),
                mcr_auth::offline::offline_uuid(&config.name)?,
            ),
        };
        let mut state = ClientState::new(&config, uuid);
        state.server.cookies = config.cookies.clone();
        let mut client = Self {
            conn,
            ids,
            wire,
            session: Session::default(),
            inbound_buf: Vec::new(),
            outbound_buf: Vec::new(),
            translate_warned: FxHashSet::default(),
            config,
            state,
            phase: Phase::Login,
            shared_phase: phase,
            events: VecDeque::new(),
            scratch: Vec::new(),
            sequence: 0,
            transfer: None,
            pending_pongs: Vec::new(),
            finish_pending: None,
            skipped_play: 0,
            pusher_scratch: Vec::new(),
            joined: false,
            conduct_pending: false,
            ticks: 0,
            view: (0.0, 0.0),
            digging: None,
            destroy_delay: 0,
            using: None,
            ride: None,
            combat: crate::combat::Combat::default(),
            relay,
            #[cfg(test)]
            sent: Vec::new(),
        };
        let intent = if client.config.transfer {
            Intent::Transfer
        } else {
            Intent::Login
        };
        client.send_handshake(&HandshakeServerbound::Intention(Intention {
            protocol_version: client.state.data.protocol() as i32,
            host: client.config.host.clone(),
            port: client.config.port,
            intent,
        }))?;
        client.send_login(&LoginServerbound::Hello(l::ServerboundHello {
            name,
            profile_id: uuid,
        }))?;
        client.await_play()?;
        Ok(client)
    }

    fn await_play(&mut self) -> Result<()> {
        let deadline = Instant::now() + self.config.join_timeout;
        while self.phase != Phase::Play {
            if Instant::now() >= deadline {
                return Err(ClientError::Timeout("joining"));
            }
            self.flush_pack_replies()?;
            self.flush_dialog_replies()?;
            self.flush_conduct_reply()?;
            self.flush_relay()?;
            self.try_finish_configuration()?;
            match self.conn.recv_timeout(Duration::from_millis(50)) {
                Some(Event::Frame(f)) => self.handle_frame(f)?,
                Some(Event::Closed(reason)) => return Err(self.closed(reason.to_string())),
                None => {}
            }
        }
        Ok(())
    }

    fn closed(&mut self, reason: String) -> ClientError {
        self.set_phase(Phase::Closed);
        if let Some(r) = self.relay.as_mut() {
            r.closed(&reason);
        }
        if !self
            .events
            .iter()
            .any(|e| matches!(e, ClientEvent::Disconnected(_)))
        {
            self.events
                .push_back(ClientEvent::Disconnected(DisconnectReason::Network(
                    reason.clone(),
                )));
        }
        ClientError::Connection(reason)
    }

    fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.shared_phase.store(phase as u8, Ordering::Release);
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_closed(&self) -> bool {
        self.phase == Phase::Closed
    }

    pub(crate) fn push_event(&mut self, event: ClientEvent) {
        self.events.push_back(event);
    }

    pub fn drain_events(&mut self) -> std::collections::vec_deque::Drain<'_, ClientEvent> {
        self.events.drain(..)
    }

    pub fn take_transfer(&mut self) -> Option<(String, u16)> {
        self.transfer.take()
    }

    /// Vanilla drops the connection as soon as it is told to transfer.
    fn transferred(&mut self, host: String, port: u16) -> ClientError {
        self.transfer = Some((host.clone(), port));
        self.set_phase(Phase::Closed);
        ClientError::Transferred(Box::new(crate::config::TransferTarget {
            host,
            port,
            cookies: self.state.server.cookies.clone(),
        }))
    }

    pub fn disconnect(self) {
        self.conn.close();
    }

    fn send_raw(&mut self, state: ProtocolState, id: i32) -> Result<()> {
        match self.wire.outbound(
            state,
            id,
            &self.scratch,
            &mut self.session,
            &mut self.outbound_buf,
        )? {
            Translated::Packet(id, payload) => self.conn.send(id, payload)?,
            Translated::Packets(frames) => {
                for (id, payload) in frames {
                    self.conn.send(id, payload)?;
                }
            }
            Translated::Drop => {}
        }
        Ok(())
    }

    fn send_handshake(&mut self, p: &HandshakeServerbound) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(ProtocolState::Handshake, id)
    }

    fn send_login(&mut self, p: &LoginServerbound<'_>) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(ProtocolState::Login, id)
    }

    fn send_config(&mut self, p: &ConfigServerbound<'_>) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(ProtocolState::Configuration, id)
    }

    /// Tells the mirror about a predicted action; call before its packet goes out.
    pub(crate) fn local_action(&mut self, action: LocalAction) {
        if self.phase == Phase::Play
            && let Some(r) = self.relay.as_mut()
        {
            r.local_action(action);
        }
    }

    pub fn send_play(&mut self, p: &PlayServerbound<'_>) -> Result<()> {
        if self.phase != Phase::Play {
            return Ok(());
        }
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        #[cfg(test)]
        self.sent.push((id, self.scratch.clone()));
        self.send_raw(ProtocolState::Play, id)
    }

    /// A client in the play phase over a loopback socket nobody reads; sent packets land in `sent`.
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let stream =
            std::net::TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (peer, _) = listener.accept().expect("accept");
        std::mem::forget(peer);
        let conn = Connection::from_stream(stream, ConnectOptions::default()).expect("conn");
        let config = ClientConfig::default();
        let state = ClientState::new(&config, Uuid::nil());
        Self {
            conn,
            ids: PacketIds::v26_3(),
            wire: Wire::Canonical,
            session: Session::default(),
            inbound_buf: Vec::new(),
            outbound_buf: Vec::new(),
            translate_warned: FxHashSet::default(),
            config,
            state,
            phase: Phase::Play,
            shared_phase: Arc::new(AtomicU8::new(Phase::Play as u8)),
            events: VecDeque::new(),
            scratch: Vec::new(),
            sequence: 0,
            transfer: None,
            joined: true,
            conduct_pending: false,
            ticks: 0,
            view: (0.0, 0.0),
            digging: None,
            destroy_delay: 0,
            using: None,
            ride: None,
            combat: crate::combat::Combat::default(),
            relay: None,
            sent: Vec::new(),
            pending_pongs: Vec::new(),
            finish_pending: None,
            skipped_play: 0,
            pusher_scratch: Vec::new(),
        }
    }

    /// Decodes and drains the packets sent so far.
    #[cfg(test)]
    pub(crate) fn take_sent(&mut self) -> Vec<PlayServerbound<'static>> {
        std::mem::take(&mut self.sent)
            .into_iter()
            .map(|(id, payload)| {
                let payload: &'static [u8] = Box::leak(payload.into_boxed_slice());
                PlayServerbound::decode(id, payload, self.ids).expect("decodes")
            })
            .collect()
    }

    /// The wire this connection speaks.
    pub fn wire(&self) -> Wire {
        self.wire
    }

    fn send_client_information(&mut self) -> Result<()> {
        let info = c::ClientInformation {
            language: self.config.language.clone(),
            view_distance: self.config.view_distance,
            chat_visibility: self.config.chat_visibility,
            chat_colors: true,
            model_customisation: 0x7f,
            main_hand: self.config.main_hand,
            text_filtering_enabled: false,
            allows_listing: true,
            particle_status: self.config.particle_status,
        };
        let mut brand = Vec::new();
        mcr_buf::Encode::encode(&self.config.brand, &mut brand);
        self.send_config(&ConfigServerbound::CustomPayload(
            c::ServerboundCustomPayload {
                channel: Identifier::parse("minecraft:brand").expect("valid"),
                data: RestBytes(&brand),
            },
        ))?;
        self.send_config(&ConfigServerbound::ClientInformation(info))
    }

    fn handle_frame(&mut self, frame: InboundFrame) -> Result<()> {
        let state = match self.phase {
            Phase::Login => ProtocolState::Login,
            Phase::Configuration => ProtocolState::Configuration,
            Phase::Play => ProtocolState::Play,
            Phase::Closed => return Ok(()),
        };
        let mut buf = std::mem::take(&mut self.inbound_buf);
        let result =
            match self
                .wire
                .inbound(state, frame.id, &frame.payload, &mut self.session, &mut buf)
            {
                Ok(Translated::Packet(id, payload)) => self.dispatch(id, payload),
                Ok(Translated::Packets(frames)) => frames
                    .into_iter()
                    .try_for_each(|(id, payload)| self.dispatch(id, payload)),
                Ok(Translated::Drop) => Ok(()),
                Err(e) if state == ProtocolState::Play => {
                    if self.translate_warned.len() < 256
                        && self.translate_warned.insert((state as u8, frame.id))
                    {
                        eprintln!(
                            "dropping untranslatable packet: {}",
                            crate::events::sanitize_for_log(&e.to_string())
                        );
                    }
                    Ok(())
                }
                Err(e) => Err(e.into()),
            };
        self.inbound_buf = buf;
        result
    }

    fn dispatch(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        match self.phase {
            Phase::Login => self.handle_login(id, payload),
            Phase::Configuration => self.handle_config(id, payload),
            Phase::Play => self.handle_play(id, payload),
            Phase::Closed => Ok(()),
        }
    }

    fn handle_login(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let packet = LoginClientbound::decode(id, payload, self.ids)?;
        match packet {
            LoginClientbound::LoginDisconnect(p) => {
                self.set_phase(Phase::Closed);
                let reason = plain_json_text(&p.reason);
                self.events
                    .push_back(ClientEvent::Disconnected(DisconnectReason::Server(
                        reason.clone(),
                    )));
                return Err(ClientError::Disconnected(Box::new(reason)));
            }
            LoginClientbound::Hello(p) => self.encrypt(&p)?,
            LoginClientbound::LoginCompression(p) => self.conn.enable_compression(p.threshold)?,
            LoginClientbound::LoginFinished(_) => {
                self.send_login(&LoginServerbound::LoginAcknowledged(l::LoginAcknowledged))?;
                self.set_phase(Phase::Configuration);
                self.send_client_information()?;
            }
            LoginClientbound::CustomQuery(_)
                if self
                    .relay
                    .as_ref()
                    .is_some_and(|r| r.answers_custom_queries()) => {}
            LoginClientbound::CustomQuery(p) => {
                self.send_login(&LoginServerbound::CustomQueryAnswer(l::CustomQueryAnswer {
                    transaction_id: p.transaction_id,
                    data: None,
                }))?;
            }
            LoginClientbound::CookieRequest(p) => {
                let payload = self.state.server.cookies.get(p.key.as_str()).cloned();
                self.send_login(&LoginServerbound::CookieResponse(c::CookieResponse {
                    key: p.key,
                    payload: payload.as_deref().map(ByteArray),
                }))?;
            }
        }
        Ok(())
    }

    fn encrypt(&mut self, hello: &l::ClientboundHello<'_>) -> Result<()> {
        let auth = self
            .config
            .online
            .clone()
            .ok_or(ClientError::OnlineModeRequired)?;
        let secret = mcr_auth::crypt::generate_shared_secret();
        if hello.should_authenticate {
            let hash =
                mcr_auth::session::server_hash(&hello.server_id, &secret, hello.public_key.0);
            mcr_auth::session::join_server(
                &mcr_auth::UreqClient::new(),
                &auth.access_token,
                &auth.profile_id,
                &hash,
            )?;
        }
        let enc =
            mcr_auth::crypt::encrypt_handshake(hello.public_key.0, &secret, hello.challenge.0)?;
        let packet = LoginServerbound::Key(l::Key {
            shared_secret: ByteArray(&enc.shared_secret),
            encrypted_challenge: ByteArray(&enc.verify_token),
        });
        self.scratch.clear();
        let id = packet.encode_into(self.ids, &mut self.scratch);
        match self.wire.outbound(
            ProtocolState::Login,
            id,
            &self.scratch,
            &mut self.session,
            &mut self.outbound_buf,
        )? {
            Translated::Packet(id, payload) => {
                self.conn.send_and_enable_encryption(id, payload, secret)?
            }
            Translated::Packets(_) | Translated::Drop => {
                return Err(ClientError::Invalid("key packet not translated 1:1".into()));
            }
        }
        Ok(())
    }

    fn handle_config(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let packet = ConfigClientbound::decode(id, payload, self.ids)?;
        match packet {
            ConfigClientbound::CookieRequest(p) => {
                let payload = self.state.server.cookies.get(p.key.as_str()).cloned();
                self.send_config(&ConfigServerbound::CookieResponse(c::CookieResponse {
                    key: p.key,
                    payload: payload.as_deref().map(ByteArray),
                }))?;
            }
            ConfigClientbound::CustomPayload(p) => {
                if p.channel.as_str() == "minecraft:brand" {
                    let mut input = p.data.0;
                    if let Ok(b) = mcr_buf::decode_str(&mut input, 128) {
                        self.state.server.brand = Some(b.to_owned());
                    }
                }
            }
            ConfigClientbound::Disconnect(p) => {
                self.set_phase(Phase::Closed);
                self.events
                    .push_back(ClientEvent::Disconnected(DisconnectReason::Server(
                        p.reason.clone(),
                    )));
                return Err(ClientError::Disconnected(Box::new(p.reason)));
            }
            ConfigClientbound::FinishConfiguration(_) => {
                self.finish_pending = Some(Instant::now());
                self.try_finish_configuration()?;
            }
            ConfigClientbound::KeepAlive(p) => {
                self.send_config(&ConfigServerbound::KeepAlive(c::ServerboundKeepAlive {
                    id: p.id,
                }))?;
            }
            ConfigClientbound::Ping(p) => {
                if self.pending_pongs.len() < MAX_PENDING_PONGS
                    && self
                        .relay
                        .as_ref()
                        .is_some_and(|r| r.answers_custom_queries())
                {
                    self.pending_pongs.push(p.id);
                } else {
                    self.send_config(&ConfigServerbound::Pong(c::Pong { id: p.id }))?;
                }
            }
            ConfigClientbound::RegistryData(p) => self.state.registries.apply_registry_data(&p),
            ConfigClientbound::UpdateTags(p) => self.state.registries.apply_tags(&p),
            ConfigClientbound::UpdateEnabledFeatures(p) => {
                self.state.registries.enabled_features = p
                    .features
                    .iter()
                    .take(256)
                    .map(|f| f.as_str().to_owned())
                    .collect();
            }
            ConfigClientbound::SelectKnownPacks(_) => {
                let packs = if self.config.known_packs {
                    vec![core_pack()]
                } else {
                    Vec::new()
                };
                self.send_config(&ConfigServerbound::SelectKnownPacks(
                    cfg::ServerboundSelectKnownPacks {
                        packs: mcr_buf::LengthPrefixed(packs),
                    },
                ))?;
            }
            ConfigClientbound::ResourcePackPush(p) => {
                if let Some(link) = &self.config.pack_link {
                    let _ = link.requests.send(PackRequest::Push(p));
                } else {
                    for r in resource_pack_responses(self.config.resource_packs, p.id) {
                        self.send_config(&ConfigServerbound::ResourcePack(r))?;
                    }
                }
            }
            ConfigClientbound::ResourcePackPop(p) => {
                if let Some(link) = &self.config.pack_link {
                    let _ = link.requests.send(PackRequest::Pop(p.id));
                }
            }
            ConfigClientbound::StoreCookie(p) => {
                let key = p.key.as_str().to_owned();
                let cookies = &mut self.state.server.cookies;
                if p.payload.0.len() <= crate::state::MAX_COOKIE_BYTES
                    && (cookies.contains_key(&key) || cookies.len() < crate::state::MAX_COOKIES)
                {
                    cookies.insert(key, p.payload.0.to_vec());
                }
            }
            ConfigClientbound::Transfer(p) => {
                let port = u16::try_from(p.port).unwrap_or(25565);
                self.events.push_back(ClientEvent::Transfer {
                    host: p.host.clone(),
                    port,
                });
                return Err(self.transferred(p.host, port));
            }
            ConfigClientbound::ServerLinks(p) => {
                self.state.server.links = crate::state::trusted_links(p.links);
            }
            ConfigClientbound::CustomReportDetails(p) => {
                self.state.server.report_details =
                    crate::state::bounded_report_details(p.details.0);
            }
            ConfigClientbound::ResetChat(_) => {
                self.state.chat.history.clear();
                self.events.push_back(ClientEvent::ChatCleared);
            }
            ConfigClientbound::ShowDialog(p) => {
                if let (Some(link), Some(dialog)) = (
                    &self.config.dialog_link,
                    crate::resolve_dialog(&self.state.registries, &p.dialog.0),
                ) {
                    link.send(crate::DialogRequest::Show(dialog));
                }
            }
            ConfigClientbound::ClearDialog(_) => {
                if let Some(link) = &self.config.dialog_link {
                    link.send(crate::DialogRequest::Clear);
                }
            }
            ConfigClientbound::CodeOfConduct(p) => {
                let hash = crate::conduct::conduct_hash(p.text.as_str());
                match &self.config.conduct_link {
                    Some(link) if self.config.accepted_conduct != Some(hash) => {
                        link.send(crate::conduct::ConductRequest {
                            text: crate::conduct::sanitize_conduct(p.text.as_str()),
                            hash,
                        });
                        self.conduct_pending = true;
                    }
                    _ => self.accept_conduct()?,
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_play(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let packet = match PlayClientbound::decode(id, payload, self.ids) {
            Ok(packet) => packet,
            // Java decodes the relayed stream with the mods' codecs and stays authoritative.
            Err(e) if self.relay.is_some() => {
                self.skipped_play += 1;
                if self.skipped_play <= MAX_SKIP_LOGS {
                    eprintln!("[mods] skipping play packet {id:#04x} Rust cannot decode: {e}");
                }
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        if let Some(link) = &self.config.pack_link {
            match packet {
                PlayClientbound::ResourcePackPush(p) => {
                    let _ = link.requests.send(PackRequest::Push(p));
                    return Ok(());
                }
                PlayClientbound::ResourcePackPop(p) => {
                    let _ = link.requests.send(PackRequest::Pop(p.id));
                    return Ok(());
                }
                other => return self.handle_play_packet(other),
            }
        }
        self.handle_play_packet(packet)
    }

    fn handle_play_packet(&mut self, packet: PlayClientbound<'_>) -> Result<()> {
        let mut out = Outbox::default();
        self.state.handle_play(packet, &mut out, Instant::now())?;
        let mut reconfigure = false;
        let mut closed = None;
        let mut transfer = None;
        let mut use_completed = false;
        let local = self.state.player.entity_id;
        for event in &out.events {
            match event {
                ClientEvent::EntityEvent {
                    entity_id,
                    event: USE_ITEM_COMPLETE,
                } if *entity_id == local => use_completed = true,
                ClientEvent::Joined { .. } => self.joined = true,
                ClientEvent::Reconfiguring => reconfigure = true,
                ClientEvent::Transfer { host, port } => transfer = Some((host.clone(), *port)),
                ClientEvent::Disconnected(DisconnectReason::Server(r)) => closed = Some(r.clone()),
                _ => {}
            }
        }
        if use_completed {
            self.stop_using_local();
        }
        for request in std::mem::take(&mut out.dialogs) {
            if let Some(link) = &self.config.dialog_link {
                link.send(request);
            }
        }
        for reply in std::mem::take(&mut out.replies) {
            self.send_play(&reply)?;
        }
        for (key, payload) in std::mem::take(&mut out.cookie_responses) {
            self.send_play(&PlayServerbound::CookieResponse(c::CookieResponse {
                key,
                payload: payload.as_deref().map(ByteArray),
            }))?;
        }
        self.events.append(&mut out.events);
        if let Some((host, port)) = transfer {
            return Err(self.transferred(host, port));
        }
        if let Some(reason) = closed {
            self.set_phase(Phase::Closed);
            return Err(ClientError::Disconnected(Box::new(reason)));
        }
        if reconfigure {
            self.set_phase(Phase::Configuration);
            self.state.reset_for_reconfiguration();
            self.joined = false;
            self.send_client_information()?;
        }
        Ok(())
    }

    fn flush_pack_replies(&mut self) -> Result<()> {
        loop {
            let Some(reply) = self.config.pack_link.as_ref().and_then(|l| l.next_reply()) else {
                return Ok(());
            };
            match reply {
                PackReply::Status(status) => match self.phase {
                    Phase::Configuration => {
                        self.send_config(&ConfigServerbound::ResourcePack(status))?
                    }
                    Phase::Play => self.send_play(&PlayServerbound::ResourcePack(status))?,
                    _ => {}
                },
                PackReply::Disconnect(reason) => {
                    self.set_phase(Phase::Closed);
                    self.events
                        .push_back(ClientEvent::Disconnected(DisconnectReason::Server(
                            reason.clone(),
                        )));
                    return Err(ClientError::Disconnected(Box::new(reason)));
                }
            }
        }
    }

    fn accept_conduct(&mut self) -> Result<()> {
        self.send_config(&ConfigServerbound::AcceptCodeOfConduct(
            cfg::AcceptCodeOfConduct,
        ))
    }

    fn flush_conduct_reply(&mut self) -> Result<()> {
        use crate::conduct::ConductReply;
        if !self.conduct_pending {
            return Ok(());
        }
        let Some(reply) = self
            .config
            .conduct_link
            .as_ref()
            .and_then(|l| l.next_reply())
        else {
            return Ok(());
        };
        self.conduct_pending = false;
        match reply {
            ConductReply::Accept => self.accept_conduct(),
            ConductReply::Decline => {
                let reason = Component::translatable("disconnect.closed", Vec::new());
                self.set_phase(Phase::Closed);
                self.events
                    .push_back(ClientEvent::Disconnected(DisconnectReason::Server(
                        reason.clone(),
                    )));
                Err(ClientError::Disconnected(Box::new(reason)))
            }
        }
    }

    fn flush_dialog_replies(&mut self) -> Result<()> {
        use crate::{DialogReply, DialogRequest};
        loop {
            let Some(reply) = self
                .config
                .dialog_link
                .as_ref()
                .and_then(|l| l.next_reply())
            else {
                return Ok(());
            };
            match reply {
                DialogReply::Custom { id, payload } => {
                    let packet = c::CustomClickAction { id, payload };
                    if mcr_buf::to_bytes(&packet).len() > MAX_CUSTOM_ACTION_BYTES {
                        continue;
                    }
                    match self.phase {
                        Phase::Configuration => {
                            self.send_config(&ConfigServerbound::CustomClickAction(packet))?
                        }
                        Phase::Play => {
                            self.send_play(&PlayServerbound::CustomClickAction(packet))?
                        }
                        _ => {}
                    }
                }
                DialogReply::Command(command) => {
                    if self.phase == Phase::Play {
                        self.send_command(&command)?;
                    }
                }
                DialogReply::Open(reference) => {
                    if let (Some(link), Some(dialog)) = (
                        &self.config.dialog_link,
                        crate::resolve_dialog(&self.state.registries, &reference),
                    ) {
                        link.send(DialogRequest::Show(dialog));
                    }
                }
                DialogReply::Disconnect(reason) => {
                    self.set_phase(Phase::Closed);
                    self.events
                        .push_back(ClientEvent::Disconnected(DisconnectReason::Local));
                    return Err(ClientError::Disconnected(Box::new(reason)));
                }
            }
        }
    }

    fn flush_relay(&mut self) -> Result<()> {
        let Some(mut relay) = self.relay.take() else {
            return Ok(());
        };
        let mut result = Ok(());
        relay.poll(self.phase, &mut |item| {
            if result.is_ok() {
                result = self.relayed(item);
            }
        });
        let answers = relay.answers_custom_queries();
        self.relay = Some(relay);
        result?;
        if !answers && self.phase == Phase::Configuration {
            for id in std::mem::take(&mut self.pending_pongs) {
                self.send_config(&ConfigServerbound::Pong(c::Pong { id }))?;
            }
        }
        Ok(())
    }

    fn relayed(&mut self, item: Relayed<'_>) -> Result<()> {
        match item {
            Relayed::Frame { id, payload }
                if self.phase == Phase::Configuration && Some(id) == self.config_pong_id() =>
            {
                let pong = payload
                    .get(..4)
                    .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                if let Some(i) = pong.and_then(|p| self.pending_pongs.iter().position(|&x| x == p))
                {
                    self.pending_pongs.remove(i);
                    self.conn.send(id, payload)?;
                }
            }
            Relayed::Frame { id, payload } => self.conn.send(id, payload)?,
            Relayed::Chat(text) if self.phase == Phase::Play => self.send_chat(text)?,
            Relayed::Command(command) if self.phase == Phase::Play => self.send_command(command)?,
            Relayed::Chat(_) | Relayed::Command(_) => {}
            Relayed::Disconnect(reason) => {
                self.set_phase(Phase::Closed);
                self.events
                    .push_back(ClientEvent::Disconnected(DisconnectReason::Local));
                return Err(ClientError::Disconnected(Box::new(Component::text(reason))));
            }
        }
        Ok(())
    }

    fn try_finish_configuration(&mut self) -> Result<()> {
        let Some(since) = self.finish_pending else {
            return Ok(());
        };
        let ready = self.relay.as_mut().is_none_or(|r| r.finish_configuration());
        if !ready && since.elapsed() < FINISH_WAIT {
            return Ok(());
        }
        if !ready {
            eprintln!(
                "the mod bridge's registries did not arrive; continuing with the current tables"
            );
        }
        self.finish_pending = None;
        self.state.set_data(mcr_data::v26_3());
        self.send_config(&ConfigServerbound::FinishConfiguration(
            cfg::ServerboundFinishConfiguration,
        ))?;
        self.set_phase(Phase::Play);
        Ok(())
    }

    fn config_pong_id(&self) -> Option<i32> {
        self.ids
            .id_by_name(ProtocolState::Configuration, Direction::Serverbound, "pong")
    }

    fn pump(&mut self) -> Result<()> {
        self.flush_pack_replies()?;
        self.flush_dialog_replies()?;
        self.flush_conduct_reply()?;
        self.flush_relay()?;
        self.try_finish_configuration()?;
        for _ in 0..MAX_FRAMES_PER_PUMP {
            match self.conn.try_recv() {
                Some(Event::Frame(f)) => self.handle_frame(f)?,
                Some(Event::Closed(reason)) => return Err(self.closed(reason.to_string())),
                None => return Ok(()),
            }
        }
        Ok(())
    }

    /// One 20 TPS game tick in vanilla order: inbound packets, world, local player, tick end.
    pub fn tick(&mut self, input: &PlayerInput) -> Result<()> {
        if self.phase == Phase::Closed {
            return Err(ClientError::Closed);
        }
        self.pump()?;
        if self.phase == Phase::Play
            && self.ticks.is_multiple_of(20)
            && let Some(update) = self
                .state
                .secure
                .poll_refresh(Instant::now(), crate::secure_chat::now_millis())
        {
            self.send_play(&PlayServerbound::ChatSessionUpdate(update))?;
        }
        if self.phase == Phase::Play && self.joined {
            self.state.tick();
            self.update_using_item();
            self.tick_attack_strength();
            self.player_tick(input)?;
            self.send_play(&PlayServerbound::ClientTickEnd(sb::ClientTickEnd))?;
            if let Some(r) = self.relay.as_mut() {
                r.tick_end(self.ticks, &self.state.player);
            }
        }
        self.ticks += 1;
        Ok(())
    }

    /// Waits up to `timeout` for the next packet so a caller can sleep on the network.
    pub fn wait_for_traffic(&mut self, timeout: Duration) -> Result<()> {
        self.flush_pack_replies()?;
        match self.conn.recv_timeout(timeout) {
            Some(Event::Frame(f)) => self.handle_frame(f),
            Some(Event::Closed(reason)) => Err(self.closed(reason.to_string())),
            None => Ok(()),
        }
    }

    pub fn idle_input(&self) -> PlayerInput {
        PlayerInput {
            yaw: self.view.0,
            pitch: self.view.1,
            ..PlayerInput::default()
        }
    }

    pub(crate) fn player_tick(&mut self, input: &PlayerInput) -> Result<()> {
        let player = &mut self.state.player;
        let pos = player.physics.pos;
        let column_loaded = self
            .state
            .chunks
            .column(mcr_world::ChunkPos::from_block(
                pos[0].floor() as i32,
                pos[2].floor() as i32,
            ))
            .is_some();
        if !player.loaded {
            player.loaded_waited_ticks += 1;
            let timed_out = player.loaded_waited_ticks > LOAD_TIMEOUT_TICKS;
            let dead_and_placed = player.dead && player.placed;
            if (player.placed && column_loaded) || timed_out || dead_and_placed {
                player.loaded = true;
                self.send_play(&PlayServerbound::PlayerLoaded(sb::PlayerLoaded))?;
            }
            return Ok(());
        }
        if !column_loaded {
            return Ok(());
        }
        let mut keys = *input;
        if std::mem::take(&mut player.view_snapped) {
            keys.yaw = player.physics.yaw;
            keys.pitch = player.physics.pitch;
        }
        if player.dead {
            keys = PlayerInput {
                yaw: input.yaw,
                pitch: input.pitch,
                ..PlayerInput::default()
            };
        }
        if self.state.camera_entity.is_some() {
            keys = PlayerInput {
                yaw: keys.yaw,
                pitch: keys.pitch,
                ..PlayerInput::default()
            };
        }
        if self.state.player.is_sleeping() {
            keys = PlayerInput {
                yaw: keys.yaw,
                ..PlayerInput::default()
            };
        }
        self.view = (keys.yaw, keys.pitch);
        let player = &mut self.state.player;
        if let Some(vehicle) = player.vehicle {
            return self.ride_tick(&keys, vehicle);
        }
        self.combat.hands_busy = false;
        if self.ride.take().is_some() {
            self.state.controlled_vehicle = None;
            self.state.player.physics.passenger = false;
        }
        self.state.player.physics.glider_equipped = self.glider_equipped();
        let mut pushers = std::mem::take(&mut self.pusher_scratch);
        self.state.refresh_collision_env(&mut pushers);
        for &from in &pushers {
            self.state
                .player
                .physics
                .push_from(&self.state.chunks, from);
        }
        self.pusher_scratch = pushers;
        let player = &mut self.state.player;
        let out = player.physics.tick(&keys, &self.state.chunks);
        let entity_id = player.entity_id;
        self.apply_firework_boosts();
        if out.abilities_changed {
            let flying = self.state.player.physics.abilities.flying;
            self.send_play(&PlayServerbound::PlayerAbilities(sb::PlayerAbilities {
                flying,
            }))?;
        }
        if out.start_fall_flying {
            self.send_play(&PlayServerbound::PlayerCommand(sb::PlayerCommand {
                entity_id,
                action: sb::PlayerCommandKind::StartFallFlying,
                data: 0,
            }))?;
        }
        if let Some(i) = out.input {
            self.send_play(&PlayServerbound::PlayerInput(sb::PlayerInput {
                forward: i.forward,
                backward: i.backward,
                left: i.left,
                right: i.right,
                jump: i.jump,
                shift: i.sneak,
                sprint: i.sprint,
            }))?;
        }
        if let Some(start) = out.sprint_command {
            self.send_play(&PlayServerbound::PlayerCommand(sb::PlayerCommand {
                entity_id,
                action: if start {
                    sb::PlayerCommandKind::StartSprinting
                } else {
                    sb::PlayerCommandKind::StopSprinting
                },
                data: 0,
            }))?;
        }
        let flags = sb::MoveFlags {
            on_ground: out.on_ground,
            horizontal_collision: out.horizontal_collision,
        };
        let [x, y, z] = out.pos;
        let packet = match out.kind {
            MoveKind::None => return Ok(()),
            MoveKind::StatusOnly => {
                PlayServerbound::MovePlayerStatusOnly(sb::MovePlayerStatusOnly { flags })
            }
            MoveKind::Rot => PlayServerbound::MovePlayerRot(sb::MovePlayerRot {
                y_rot: out.yaw,
                x_rot: out.pitch,
                flags,
            }),
            MoveKind::Pos => PlayServerbound::MovePlayerPos(sb::MovePlayerPos { x, y, z, flags }),
            MoveKind::PosRot => PlayServerbound::MovePlayerPosRot(sb::MovePlayerPosRot {
                x,
                y,
                z,
                y_rot: out.yaw,
                x_rot: out.pitch,
                flags,
            }),
        };
        self.send_play(&packet)
    }
}
