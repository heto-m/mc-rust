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
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::error::{ClientError, Result};
use crate::events::{ClientEvent, DisconnectReason};
use crate::handler::core_pack;
use crate::state::{ClientState, Outbox, resource_pack_responses};

const MAX_FRAMES_PER_PUMP: usize = 8192;
const LOAD_TIMEOUT_TICKS: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Login = 0,
    Configuration = 1,
    Play = 2,
    Closed = 3,
}

#[derive(Debug)]
pub struct Client {
    conn: Connection,
    ids: &'static PacketIds,
    pub config: ClientConfig,
    pub state: ClientState,
    phase: Phase,
    shared_phase: Arc<AtomicU8>,
    events: VecDeque<ClientEvent>,
    scratch: Vec<u8>,
    pub(crate) sequence: i32,
    transfer: Option<(String, u16)>,
    joined: bool,
    pub ticks: u64,
    pub view: (f32, f32),
    pub(crate) digging: Option<crate::actions::Digging>,
}

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
        let ids = PacketIds::v26_3();
        let phase = Arc::new(AtomicU8::new(Phase::Login as u8));
        let compression_id = ids
            .id_by_name(
                ProtocolState::Login,
                Direction::Clientbound,
                "login_compression",
            )
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
                connect_timeout: Some(config.connect_timeout),
            },
        )?;
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
        let state = ClientState::new(&config, uuid);
        let mut client = Self {
            conn,
            ids,
            config,
            state,
            phase: Phase::Login,
            shared_phase: phase,
            events: VecDeque::new(),
            scratch: Vec::new(),
            sequence: 0,
            transfer: None,
            joined: false,
            ticks: 0,
            view: (0.0, 0.0),
            digging: None,
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

    pub fn drain_events(&mut self) -> std::collections::vec_deque::Drain<'_, ClientEvent> {
        self.events.drain(..)
    }

    pub fn take_transfer(&mut self) -> Option<(String, u16)> {
        self.transfer.take()
    }

    pub fn disconnect(self) {
        self.conn.close();
    }

    fn send_raw(&mut self, id: i32) -> Result<()> {
        self.conn.send(id, &self.scratch)?;
        Ok(())
    }

    fn send_handshake(&mut self, p: &HandshakeServerbound) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(id)
    }

    fn send_login(&mut self, p: &LoginServerbound<'_>) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(id)
    }

    fn send_config(&mut self, p: &ConfigServerbound<'_>) -> Result<()> {
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(id)
    }

    pub fn send_play(&mut self, p: &PlayServerbound<'_>) -> Result<()> {
        if self.phase != Phase::Play {
            return Ok(());
        }
        self.scratch.clear();
        let id = p.encode_into(self.ids, &mut self.scratch);
        self.send_raw(id)
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
        match self.phase {
            Phase::Login => self.handle_login(frame),
            Phase::Configuration => self.handle_config(frame),
            Phase::Play => self.handle_play(frame),
            Phase::Closed => Ok(()),
        }
    }

    fn handle_login(&mut self, frame: InboundFrame) -> Result<()> {
        let packet = LoginClientbound::decode(frame.id, &frame.payload, self.ids)?;
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
            LoginClientbound::CustomQuery(p) => {
                self.send_login(&LoginServerbound::CustomQueryAnswer(l::CustomQueryAnswer {
                    transaction_id: p.transaction_id,
                    data: None,
                }))?;
            }
            LoginClientbound::CookieRequest(p) => {
                self.send_login(&LoginServerbound::CookieResponse(c::CookieResponse {
                    key: p.key,
                    payload: None,
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
        self.conn
            .send_and_enable_encryption(id, &self.scratch, secret)?;
        Ok(())
    }

    fn handle_config(&mut self, frame: InboundFrame) -> Result<()> {
        let packet = ConfigClientbound::decode(frame.id, &frame.payload, self.ids)?;
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
                self.send_config(&ConfigServerbound::FinishConfiguration(
                    cfg::ServerboundFinishConfiguration,
                ))?;
                self.set_phase(Phase::Play);
            }
            ConfigClientbound::KeepAlive(p) => {
                self.send_config(&ConfigServerbound::KeepAlive(c::ServerboundKeepAlive {
                    id: p.id,
                }))?;
            }
            ConfigClientbound::Ping(p) => {
                self.send_config(&ConfigServerbound::Pong(c::Pong { id: p.id }))?;
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
                for r in resource_pack_responses(self.config.resource_packs, p.id) {
                    self.send_config(&ConfigServerbound::ResourcePack(r))?;
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
                self.transfer = Some((p.host.clone(), u16::try_from(p.port).unwrap_or(25565)));
                self.events.push_back(ClientEvent::Transfer {
                    host: p.host,
                    port: u16::try_from(p.port).unwrap_or(25565),
                });
            }
            ConfigClientbound::ServerLinks(p) => {
                self.state.server.links = p
                    .links
                    .into_iter()
                    .take(crate::state::MAX_SERVER_LINKS)
                    .collect();
            }
            ConfigClientbound::CodeOfConduct(_) => {
                self.send_config(&ConfigServerbound::AcceptCodeOfConduct(
                    cfg::AcceptCodeOfConduct,
                ))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_play(&mut self, frame: InboundFrame) -> Result<()> {
        let packet = PlayClientbound::decode(frame.id, &frame.payload, self.ids)?;
        let mut out = Outbox::default();
        self.state.handle_play(packet, &mut out, Instant::now())?;
        let mut reconfigure = false;
        let mut closed = None;
        for event in &out.events {
            match event {
                ClientEvent::Joined { .. } => self.joined = true,
                ClientEvent::Reconfiguring => reconfigure = true,
                ClientEvent::Transfer { host, port } => self.transfer = Some((host.clone(), *port)),
                ClientEvent::Disconnected(DisconnectReason::Server(r)) => closed = Some(r.clone()),
                _ => {}
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

    fn pump(&mut self) -> Result<()> {
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
        if self.phase == Phase::Play && self.joined {
            self.state.tick();
            self.player_tick(input)?;
            self.send_play(&PlayServerbound::ClientTickEnd(sb::ClientTickEnd))?;
        }
        self.ticks += 1;
        Ok(())
    }

    /// Waits up to `timeout` for the next packet so a caller can sleep on the network.
    pub fn wait_for_traffic(&mut self, timeout: Duration) -> Result<()> {
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

    fn player_tick(&mut self, input: &PlayerInput) -> Result<()> {
        let player = &mut self.state.player;
        if !player.loaded {
            player.loaded_waited_ticks += 1;
            let pos = player.physics.pos;
            let ground_loaded = self
                .state
                .chunks
                .column(mcr_world::ChunkPos::from_block(
                    pos[0].floor() as i32,
                    pos[2].floor() as i32,
                ))
                .is_some();
            let timed_out = player.loaded_waited_ticks > LOAD_TIMEOUT_TICKS;
            if (player.placed && ground_loaded) || timed_out {
                player.loaded = true;
                self.send_play(&PlayServerbound::PlayerLoaded(sb::PlayerLoaded))?;
            }
            return Ok(());
        }
        let mut keys = *input;
        if player.dead {
            keys = PlayerInput {
                yaw: input.yaw,
                pitch: input.pitch,
                ..PlayerInput::default()
            };
        }
        self.view = (keys.yaw, keys.pitch);
        let out = player.physics.tick(&keys, &self.state.chunks);
        let entity_id = player.entity_id;
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
