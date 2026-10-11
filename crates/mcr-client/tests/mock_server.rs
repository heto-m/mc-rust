#![allow(clippy::field_reassign_with_default)]
//! A scripted fake server: login, configuration, play, then a re-configuration round trip.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use mcr_buf::Identifier;
use mcr_client::{Client, ClientConfig, ClientEvent, Phase, PlayerInput};
use mcr_data::{Direction, ProtocolState};
use mcr_net::{BufPool, FrameDecoder, FrameEncoder, InboundFrame};
use mcr_protocol::PacketIds;
use mcr_protocol::common::Disconnect;
use mcr_protocol::configuration::{ClientboundFinishConfiguration, RegistryData};
use mcr_protocol::dispatch::{ConfigClientbound, LoginClientbound, PlayClientbound};
use mcr_protocol::login::LoginFinished;
use mcr_protocol::play::clientbound::player::{
    CommonPlayerSpawnInfo, Login, PlayerPosition, RelativeFlags, StartConfiguration,
};
use mcr_protocol::play::clientbound::world::SetChunkCacheCenter;
use mcr_protocol::types::Component;
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::profile::GameProfile;
use mcr_protocol::types::registry::RegistryEntry;
use uuid::Uuid;

struct Peer {
    stream: TcpStream,
    dec: FrameDecoder,
    enc: FrameEncoder,
    ids: &'static PacketIds,
}

impl Peer {
    fn new(stream: TcpStream) -> Self {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Self {
            stream,
            dec: FrameDecoder::new(BufPool::new(8)),
            enc: FrameEncoder::new(),
            ids: PacketIds::v26_3(),
        }
    }

    fn write(&mut self, id: i32, payload: &[u8]) {
        let mut out = Vec::new();
        self.enc.encode(id, payload, &mut out).unwrap();
        self.stream.write_all(&out).unwrap();
    }

    fn recv(&mut self) -> InboundFrame {
        loop {
            if let Some(f) = self.dec.next_frame().unwrap() {
                return f;
            }
            self.dec.fill(&mut self.stream).unwrap();
        }
    }

    fn name_of(&self, state: ProtocolState, frame: &InboundFrame) -> String {
        mcr_data::v26_3()
            .packet_name(state, Direction::Serverbound, frame.id as u32)
            .unwrap_or_default()
            .to_owned()
    }

    fn recv_until(&mut self, state: ProtocolState, name: &str) {
        for _ in 0..200 {
            let f = self.recv();
            if self.name_of(state, &f) == format!("minecraft:{name}") {
                return;
            }
        }
        panic!("never received {name}");
    }

    fn login(&mut self, p: LoginClientbound<'_>) {
        let (id, body) = p.encode(self.ids);
        self.write(id, &body);
    }

    fn config(&mut self, p: ConfigClientbound<'_>) {
        let (id, body) = p.encode(self.ids);
        self.write(id, &body);
    }

    fn play(&mut self, p: PlayClientbound<'_>) {
        let (id, body) = p.encode(self.ids);
        self.write(id, &body);
    }
}

fn registries(peer: &mut Peer) {
    for (registry, entry) in [
        ("minecraft:dimension_type", "minecraft:overworld"),
        ("minecraft:worldgen/biome", "minecraft:plains"),
    ] {
        peer.config(ConfigClientbound::RegistryData(RegistryData {
            registry: Identifier::parse(registry).unwrap(),
            entries: vec![RegistryEntry {
                id: Identifier::parse(entry).unwrap(),
                data: None,
            }],
        }));
    }
    peer.config(ConfigClientbound::FinishConfiguration(
        ClientboundFinishConfiguration,
    ));
    peer.recv_until(ProtocolState::Configuration, "finish_configuration");
}

fn enter_play(peer: &mut Peer) {
    peer.play(PlayClientbound::Login(Login {
        player_id: 7,
        hardcore: false,
        levels: vec![Identifier::parse("minecraft:overworld").unwrap()],
        max_players: 20,
        chunk_radius: 4,
        simulation_distance: 4,
        reduced_debug_info: false,
        show_death_screen: true,
        do_limited_crafting: false,
        spawn_info: CommonPlayerSpawnInfo {
            dimension_type: 0,
            dimension: Identifier::parse("minecraft:overworld").unwrap(),
            seed: 1,
            game_type: GameType::Survival,
            previous_game_type: None,
            is_debug: false,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        },
        online_mode: false,
        enforces_secure_chat: false,
    }));
    peer.play(PlayClientbound::SetChunkCacheCenter(SetChunkCacheCenter {
        x: 0,
        z: 0,
    }));
    peer.play(PlayClientbound::PlayerPosition(PlayerPosition {
        id: 1,
        position: Vec3d {
            x: 8.5,
            y: 70.0,
            z: 8.5,
        },
        delta_movement: Vec3d::default(),
        y_rot: 0.0,
        x_rot: 0.0,
        relatives: RelativeFlags::default(),
    }));
}

fn serve(listener: TcpListener) {
    let (stream, _) = listener.accept().unwrap();
    let mut peer = Peer::new(stream);
    peer.recv_until(ProtocolState::Handshake, "intention");
    peer.recv_until(ProtocolState::Login, "hello");
    peer.login(LoginClientbound::LoginFinished(LoginFinished {
        profile: GameProfile {
            id: Uuid::nil(),
            name: "Bot".into(),
            properties: vec![],
        },
        session_id: Uuid::nil(),
    }));
    peer.recv_until(ProtocolState::Login, "login_acknowledged");
    registries(&mut peer);
    enter_play(&mut peer);
    peer.recv_until(ProtocolState::Play, "accept_teleportation");

    peer.play(PlayClientbound::StartConfiguration(StartConfiguration));
    peer.recv_until(ProtocolState::Play, "configuration_acknowledged");
    peer.recv_until(ProtocolState::Configuration, "client_information");
    registries(&mut peer);
    enter_play(&mut peer);
    peer.recv_until(ProtocolState::Play, "accept_teleportation");

    peer.play(PlayClientbound::Disconnect(Disconnect {
        reason: Component::text("bye \u{1b}[31m"),
    }));
    std::thread::sleep(Duration::from_millis(300));
}

#[test]
fn reconfiguration_round_trip_and_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || serve(listener));

    let mut cfg = ClientConfig::default();
    cfg.host = "127.0.0.1".into();
    cfg.port = port;
    let mut client = Client::connect(cfg).expect("join mock server");
    assert_eq!(client.phase(), Phase::Play);

    let mut events = Vec::new();
    let mut error = None;
    for _ in 0..200 {
        match client.tick(&PlayerInput::default()) {
            Ok(()) => {}
            Err(e) => {
                error = Some(e);
                events.extend(client.drain_events());
                break;
            }
        }
        events.extend(client.drain_events());
        std::thread::sleep(Duration::from_millis(10));
    }
    let joins = events
        .iter()
        .filter(|e| matches!(e, ClientEvent::Joined { .. }))
        .count();
    assert_eq!(joins, 2, "joined, re-configured, joined again");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::Reconfiguring))
    );
    let err = error
        .expect("server disconnect ends the session")
        .to_string();
    assert!(err.contains("bye") && !err.contains('\u{1b}'), "{err}");
    assert_eq!(client.state.player.teleports_received, 1);
    server.join().unwrap();
}

fn serve_transfer(listener: TcpListener) {
    let (stream, _) = listener.accept().unwrap();
    let mut peer = Peer::new(stream);
    peer.recv_until(ProtocolState::Handshake, "intention");
    peer.recv_until(ProtocolState::Login, "hello");
    peer.login(LoginClientbound::LoginFinished(LoginFinished {
        profile: GameProfile {
            id: Uuid::nil(),
            name: "Bot".into(),
            properties: vec![],
        },
        session_id: Uuid::nil(),
    }));
    peer.recv_until(ProtocolState::Login, "login_acknowledged");
    peer.config(ConfigClientbound::ResetChat(
        mcr_protocol::configuration::ResetChat,
    ));
    registries(&mut peer);
    enter_play(&mut peer);
    peer.recv_until(ProtocolState::Play, "accept_teleportation");
    peer.play(PlayClientbound::StoreCookie(
        mcr_protocol::common::StoreCookie {
            key: Identifier::parse("lobby:ticket").unwrap(),
            payload: mcr_buf::ByteArray(b"seat-7"),
        },
    ));
    peer.play(PlayClientbound::Transfer(mcr_protocol::common::Transfer {
        host: "game.example".into(),
        port: 25570,
    }));
    std::thread::sleep(Duration::from_millis(300));
}

#[test]
fn transfer_closes_the_session_and_carries_cookies() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || serve_transfer(listener));

    let mut cfg = ClientConfig::default();
    cfg.host = "127.0.0.1".into();
    cfg.port = port;
    let mut client = Client::connect(cfg.clone()).expect("join mock server");
    let mut events: Vec<ClientEvent> = client.drain_events().collect();
    let mut error = None;
    for _ in 0..200 {
        if let Err(e) = client.tick(&PlayerInput::default()) {
            error = Some(e);
            break;
        }
        events.extend(client.drain_events());
        std::thread::sleep(Duration::from_millis(10));
    }
    events.extend(client.drain_events());
    assert!(events.iter().any(|e| matches!(e, ClientEvent::ChatCleared)));
    let Some(mcr_client::ClientError::Transferred(target)) = error else {
        panic!("expected a transfer, got {error:?}");
    };
    assert_eq!((target.host.as_str(), target.port), ("game.example", 25570));
    assert_eq!(
        target.cookies.get("lobby:ticket").map(Vec::as_slice),
        Some(&b"seat-7"[..])
    );
    assert!(client.is_closed());
    let next = cfg.for_transfer(&target);
    assert!(next.transfer && next.cookies.contains_key("lobby:ticket"));
    assert_eq!(next.host, "game.example");
    server.join().unwrap();
}
