//! Packet-audit fixes: each test pins one vanilla behaviour the client used to miss.

use std::time::{Duration, Instant};

use mcr_buf::{IdOr, Identifier};
use mcr_nbt::{Compound, NetworkNbt, Value};
use mcr_protocol::common::{ReportDetail, ServerLink, ServerLinkLabel};
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::entity::{Animate, EntitySwing, SetPassengers};
use mcr_protocol::play::clientbound::player::{PlayerCombatKill, PlayerRotation};
use mcr_protocol::play::clientbound::ui::{
    BossBarColor, BossBarFlags, BossBarOverlay, BossEvent, BossEventOperation, ClearTitles,
    CollisionRule, DisplaySlot, NameTagVisibility, NumberFormat, ObjectiveAction, ObjectiveDisplay,
    ObjectiveRenderType, SetActionBarText, SetDisplayObjective, SetObjective, SetScore,
    SetSubtitleText, SetTitleText, SetTitlesAnimation, TeamAction, TeamColor, TeamParameters,
};
use mcr_protocol::play::clientbound::world::{
    Explode, ExplosionParticle, GameEvent, SetBorderLerpSize, TickingState, TickingStep,
};
use mcr_protocol::types::component::{SwingAnimation, SwingAnimationType};
use mcr_protocol::types::game::{GameType, Hand};
use mcr_protocol::types::text::{Color, NamedColor};
use mcr_protocol::types::{Component, ItemStack, Particle, Vec3d};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::entity::Entity;
use crate::events::{ChatKind, ClientEvent};
use crate::social::{ScoreStyle, format_score, tab_name};
use crate::state::{ClientState, Outbox};
use crate::toast::ToastEvent;

const PLAYER: i32 = 7;

fn state() -> ClientState {
    let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
    s.player.entity_id = PLAYER;
    s.name = "Bot".into();
    s
}

fn run(s: &mut ClientState, packet: P<'_>) -> Outbox {
    let mut out = Outbox::default();
    s.handle_play(packet, &mut out, Instant::now()).unwrap();
    out
}

fn events(s: &mut ClientState, packet: P<'_>) -> Vec<ClientEvent> {
    run(s, packet).events.into()
}

fn text(c: &Component) -> String {
    c.to_plain_string()
}

#[test]
fn subtitle_alone_never_shows_and_expiry_clears_both_lines() {
    let mut s = state();
    run(
        &mut s,
        P::SetSubtitleText(SetSubtitleText {
            text: Component::text("sub"),
        }),
    );
    assert_eq!(s.titles.title_alpha(0.0), None);
    run(
        &mut s,
        P::SetTitleText(SetTitleText {
            text: Component::text("Hi"),
        }),
    );
    assert_eq!(s.titles.title_time, 100);
    for _ in 0..100 {
        s.titles.tick();
    }
    assert!(s.titles.title.is_none() && s.titles.subtitle.is_none());
    run(
        &mut s,
        P::SetTitleText(SetTitleText {
            text: Component::text("Again"),
        }),
    );
    assert!(
        s.titles.subtitle.is_none(),
        "an old subtitle must not resurface"
    );
}

#[test]
fn title_times_skip_negatives_and_restart_a_showing_title() {
    let mut s = state();
    run(
        &mut s,
        P::SetTitleText(SetTitleText {
            text: Component::text("Hi"),
        }),
    );
    for _ in 0..30 {
        s.titles.tick();
    }
    run(
        &mut s,
        P::SetTitlesAnimation(SetTitlesAnimation {
            fade_in: 5,
            stay: -1,
            fade_out: 7,
        }),
    );
    assert_eq!(
        (s.titles.fade_in, s.titles.stay, s.titles.fade_out),
        (5, 70, 7)
    );
    assert_eq!(s.titles.title_time, 82);
    run(&mut s, P::ClearTitles(ClearTitles { reset_times: true }));
    assert_eq!(
        (s.titles.fade_in, s.titles.stay, s.titles.fade_out),
        (10, 70, 20)
    );
}

#[test]
fn title_alpha_fades_in_holds_and_fades_out() {
    let mut s = state();
    s.titles.set_times(10, 10, 10);
    s.titles.set_title(Component::text("x"));
    assert!(s.titles.title_alpha(0.0).unwrap_or(0) < 10);
    for _ in 0..12 {
        s.titles.tick();
    }
    assert_eq!(s.titles.title_alpha(0.0), Some(255));
    for _ in 0..13 {
        s.titles.tick();
    }
    let fading = s.titles.title_alpha(0.0).unwrap();
    assert!(fading > 0 && fading < 255);
    s.titles.set_times(i32::MAX, i32::MAX, i32::MAX);
    assert!(
        s.titles.title_time > 0,
        "hostile times saturate instead of wrapping"
    );
}

#[test]
fn clearing_titles_keeps_the_action_bar() {
    let mut s = state();
    run(
        &mut s,
        P::SetActionBarText(SetActionBarText {
            text: Component::text("bar"),
        }),
    );
    assert_eq!(s.titles.overlay_time, 60);
    run(&mut s, P::ClearTitles(ClearTitles { reset_times: false }));
    assert!(s.titles.overlay_alpha(0.0).is_some());
}

fn boss_add(name: &str) -> BossEventOperation {
    BossEventOperation::Add {
        name: Component::text(name),
        progress: 1.0,
        color: BossBarColor::Red,
        overlay: BossBarOverlay::Progress,
        flags: BossBarFlags {
            darken_screen: false,
            play_music: false,
            create_world_fog: false,
        },
    }
}

#[test]
fn boss_bars_stack_in_arrival_order_and_progress_eases() {
    let mut s = state();
    let ids = [Uuid::from_u128(9), Uuid::from_u128(1), Uuid::from_u128(5)];
    for (i, id) in ids.iter().enumerate() {
        run(
            &mut s,
            P::BossEvent(BossEvent {
                id: *id,
                operation: boss_add(&i.to_string()),
            }),
        );
    }
    run(
        &mut s,
        P::BossEvent(BossEvent {
            id: ids[0],
            operation: boss_add("again"),
        }),
    );
    let order: Vec<Uuid> = crate::social::boss_bars_ordered(&s.boss_bars)
        .into_iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(order, ids);

    let bar = s.boss_bars.get_mut(&ids[1]).unwrap();
    let t0 = Instant::now();
    bar.set_progress(0.0, t0);
    let mid = bar.displayed_progress(t0 + Duration::from_millis(50));
    assert!((mid - 0.5).abs() < 0.05, "{mid}");
    assert_eq!(bar.displayed_progress(t0 + Duration::from_millis(200)), 0.0);
}

fn objective(name: &str) -> P<'static> {
    P::SetObjective(SetObjective {
        objective_name: name.into(),
        action: ObjectiveAction::Add(ObjectiveDisplay {
            display_name: Component::text(name),
            render_type: ObjectiveRenderType::Integer,
            number_format: None,
        }),
    })
}

fn score(owner: &str, objective: &str, value: i32) -> P<'static> {
    P::SetScore(SetScore {
        owner: owner.into(),
        objective_name: objective.into(),
        score: value,
        display: None,
        number_format: None,
    })
}

fn team(name: &str, color: Option<TeamColor>, players: &[&str]) -> P<'static> {
    P::SetPlayerTeam(mcr_protocol::play::clientbound::ui::SetPlayerTeam {
        name: name.into(),
        action: TeamAction::Add {
            parameters: TeamParameters {
                display_name: Component::text(name),
                player_prefix: Component::text("["),
                player_suffix: Component::text("]"),
                name_tag_visibility: NameTagVisibility::Always,
                collision_rule: CollisionRule::Always,
                color,
                options: 0,
            },
            players: players.iter().map(|p| (*p).to_owned()).collect(),
        },
    })
}

#[test]
fn scores_and_changes_for_unknown_objectives_are_ignored() {
    let mut s = state();
    run(&mut s, score("Alex", "ghost", 3));
    assert!(s.scoreboard.scores.is_empty());
    run(
        &mut s,
        P::SetObjective(SetObjective {
            objective_name: "ghost".into(),
            action: ObjectiveAction::Change(ObjectiveDisplay {
                display_name: Component::text("Ghost"),
                render_type: ObjectiveRenderType::Integer,
                number_format: None,
            }),
        }),
    );
    assert!(s.scoreboard.objectives.is_empty());
    run(
        &mut s,
        P::SetDisplayObjective(SetDisplayObjective {
            slot: DisplaySlot::Sidebar,
            objective_name: "ghost".into(),
        }),
    );
    assert!(s.scoreboard.display_slots.is_empty());
}

#[test]
fn a_player_sits_on_one_team_at_a_time() {
    let mut s = state();
    run(&mut s, team("red", Some(TeamColor::Red), &["Alex"]));
    run(&mut s, team("blue", Some(TeamColor::Blue), &["Alex"]));
    assert_eq!(s.scoreboard.team_of("Alex"), Some("blue"));
    assert!(!s.scoreboard.teams["red"].players.contains("Alex"));
    run(
        &mut s,
        P::SetPlayerTeam(mcr_protocol::play::clientbound::ui::SetPlayerTeam {
            name: "red".into(),
            action: TeamAction::Leave(vec!["Alex".into()]),
        }),
    );
    assert_eq!(s.scoreboard.team_of("Alex"), Some("blue"));
    run(
        &mut s,
        P::SetPlayerTeam(mcr_protocol::play::clientbound::ui::SetPlayerTeam {
            name: "blue".into(),
            action: TeamAction::Remove,
        }),
    );
    assert_eq!(s.scoreboard.team_of("Alex"), None);
}

#[test]
fn number_formats_follow_score_then_objective_then_default() {
    assert_eq!(
        text(&format_score(
            Some(&NumberFormat::Blank),
            ScoreStyle::Sidebar,
            5
        )),
        ""
    );
    let fixed = NumberFormat::Fixed(Component::text("max"));
    assert_eq!(
        text(&format_score(Some(&fixed), ScoreStyle::Sidebar, 5)),
        "max"
    );
    let red = format_score(None, ScoreStyle::Sidebar, 12);
    assert_eq!(
        (text(&red), red.style.color),
        ("12".into(), Some(Color::Named(NamedColor::Red)))
    );
    let yellow = format_score(None, ScoreStyle::PlayerList, 1);
    assert_eq!(yellow.style.color, Some(Color::Named(NamedColor::Yellow)));
    let mut style = Compound::new();
    style.insert("color", "gold");
    style.insert("bold", true);
    let styled = NumberFormat::Styled(NetworkNbt(Value::Compound(style)));
    let gold = format_score(Some(&styled), ScoreStyle::Sidebar, 3);
    assert_eq!(text(&gold), "3");
    assert_eq!(gold.style.color, Some(Color::Named(NamedColor::Gold)));
    assert_eq!(gold.style.bold, Some(true));
}

#[test]
fn sidebar_uses_the_team_colour_slot_and_vanilla_ordering() {
    let mut s = state();
    run(&mut s, objective("kills"));
    run(&mut s, objective("reds"));
    for (owner, value) in [("bob", 3), ("Alice", 3), ("carl", 9), ("#hidden", 50)] {
        run(&mut s, score(owner, "kills", value));
    }
    run(
        &mut s,
        P::SetDisplayObjective(SetDisplayObjective {
            slot: DisplaySlot::Sidebar,
            objective_name: "kills".into(),
        }),
    );
    run(
        &mut s,
        P::SetDisplayObjective(SetDisplayObjective {
            slot: DisplaySlot::TeamRed,
            objective_name: "reds".into(),
        }),
    );
    assert_eq!(s.scoreboard.sidebar_objective("Bot"), Some("kills"));
    let rows: Vec<String> = s
        .scoreboard
        .sidebar_rows("kills")
        .iter()
        .map(|r| format!("{}={}", text(&r.name), text(&r.score)))
        .collect();
    assert_eq!(rows, ["carl=9", "Alice=3", "bob=3"]);
    run(&mut s, team("red", Some(TeamColor::Red), &["Bot", "carl"]));
    assert_eq!(s.scoreboard.sidebar_objective("Bot"), Some("reds"));
    let carl = s.scoreboard.format_member("carl", Component::text("carl"));
    assert_eq!(text(&carl), "[carl]");
    assert_eq!(carl.style.color, Some(Color::Named(NamedColor::Red)));
}

#[test]
fn sidebar_keeps_fifteen_rows() {
    let mut s = state();
    run(&mut s, objective("o"));
    for i in 0..40 {
        run(&mut s, score(&format!("p{i}"), "o", i));
    }
    let rows = s.scoreboard.sidebar_rows("o");
    assert_eq!(rows.len(), crate::social::MAX_SIDEBAR_ROWS);
    assert_eq!(text(&rows[0].score), "39");
}

#[test]
fn tab_list_sorts_by_order_then_spectators_then_team_then_name() {
    use mcr_protocol::play::clientbound::player::{
        PlayerInfoAction, PlayerInfoEntry, PlayerInfoUpdate,
    };
    let mut s = state();
    let entries: Vec<PlayerInfoEntry> = [
        ("zed", GameType::Survival, 0),
        ("Amy", GameType::Spectator, 0),
        ("bob", GameType::Survival, 0),
        ("Cat", GameType::Survival, 5),
        ("dan", GameType::Survival, 0),
    ]
    .iter()
    .enumerate()
    .map(|(i, (name, mode, order))| {
        let mut e = PlayerInfoEntry::new(Uuid::from_u128(i as u128 + 1));
        e.profile.name = (*name).into();
        e.game_mode = *mode;
        e.list_order = *order;
        e.listed = true;
        e
    })
    .collect();
    s.players.apply(&PlayerInfoUpdate::with_actions(
        &[
            PlayerInfoAction::AddPlayer,
            PlayerInfoAction::UpdateListed,
            PlayerInfoAction::UpdateGameMode,
            PlayerInfoAction::UpdateListOrder,
        ],
        entries,
    ));
    run(&mut s, team("a_team", None, &["zed"]));
    let names: Vec<&str> = s
        .players
        .tab_order(&s.scoreboard)
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["Cat", "bob", "dan", "zed", "Amy"]);
    let amy = s.players.iter().find(|p| p.name == "Amy").unwrap();
    assert_eq!(tab_name(amy, &s.scoreboard).style.italic, Some(true));
}

#[test]
fn animate_and_swing_are_separate_events() {
    let mut s = state();
    let crit = events(
        &mut s,
        P::Animate(Animate {
            entity_id: 3,
            action: 1,
        }),
    );
    assert_eq!(
        crit,
        [ClientEvent::EntityAnimation {
            entity_id: 3,
            action: 1
        }]
    );
    let swing = events(
        &mut s,
        P::SwingAnimation(EntitySwing {
            entity_id: 3,
            hand: Hand::Off,
            animation: SwingAnimation {
                kind: SwingAnimationType::Whack,
                duration: 6,
            },
        }),
    );
    assert_eq!(
        swing,
        [ClientEvent::EntitySwing {
            entity_id: 3,
            offhand: true,
            animation: SwingAnimation {
                kind: SwingAnimationType::Whack,
                duration: 6,
            },
        }]
    );
}

fn explosion(play_sound: bool) -> P<'static> {
    P::Explode(Explode {
        center: Vec3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        radius: 4.0,
        block_count: 12,
        player_knockback: None,
        particle: Particle::Simple(1),
        sound: IdOr::Id(5),
        block_particles: vec![
            ExplosionParticle {
                particle: Particle::Simple(2),
                scaling: 1.0,
                speed: 1.0,
                weight: 1,
            };
            100
        ],
        play_sound,
    })
}

#[test]
fn explosions_carry_their_sound_and_bounded_debris() {
    let mut s = state();
    let loud = events(&mut s, explosion(true));
    let Some(ClientEvent::Explosion {
        sound,
        block_count,
        block_particles,
        ..
    }) = loud.first()
    else {
        panic!("{loud:?}");
    };
    assert_eq!((sound, *block_count), (&Some(IdOr::Id(5)), 12));
    assert_eq!(
        block_particles.len(),
        crate::events::MAX_EXPLOSION_PARTICLE_KINDS
    );
    let quiet = events(&mut s, explosion(false));
    assert!(matches!(
        quiet.first(),
        Some(ClientEvent::Explosion { sound: None, .. })
    ));
}

fn game_event(event: u8, param: f32) -> P<'static> {
    P::GameEvent(GameEvent { event, param })
}

fn local_sound(events: &[ClientEvent]) -> Option<&'static str> {
    events.iter().find_map(|e| match e {
        ClientEvent::LocalSound { sound, .. } => Some(*sound),
        _ => None,
    })
}

#[test]
fn game_event_sounds_and_messages() {
    let mut s = state();
    assert_eq!(
        local_sound(&events(&mut s, game_event(6, 0.0))),
        Some("entity.arrow.hit_player")
    );
    assert_eq!(
        local_sound(&events(&mut s, game_event(9, 0.0))),
        Some("entity.puffer_fish.sting")
    );
    assert_eq!(
        local_sound(&events(&mut s, game_event(10, 1.0))),
        Some("entity.elder_guardian.curse")
    );
    assert_eq!(local_sound(&events(&mut s, game_event(10, 0.0))), None);
    let spawn = events(&mut s, game_event(0, 0.0));
    assert!(matches!(
        spawn.as_slice(),
        [ClientEvent::Chat { kind: ChatKind::System, message, .. }]
            if matches!(&message.content, mcr_protocol::types::text::Content::Translatable { key, .. } if key == "block.minecraft.spawn.not_valid")
    ));
    let demo = events(&mut s, game_event(5, 101.0));
    assert_eq!(demo.len(), 1);
    assert!(events(&mut s, game_event(5, 42.0)).is_empty());
}

#[test]
fn server_rotation_is_acknowledged_with_a_rotation_packet() {
    let mut s = state();
    let out = run(
        &mut s,
        P::PlayerRotation(PlayerRotation {
            y_rot: 90.0,
            relative_y: false,
            x_rot: 10.0,
            relative_x: false,
        }),
    );
    assert!(matches!(
        out.replies.as_slice(),
        [S::MovePlayerRot(r)] if r.y_rot == 90.0 && r.x_rot == 10.0 && !r.flags.on_ground
    ));
}

#[test]
fn boarding_a_boat_shows_the_dismount_hint_and_faces_forward() {
    let data = mcr_data::v26_3();
    let boat = data.entity_type_by_name("minecraft:oak_boat").unwrap() as i32;
    let mut s = state();
    let mut e = Entity::new(40, Uuid::from_u128(40), boat, [0.0, 64.0, 0.0]);
    e.yaw = 135.0;
    s.entities.insert(e);
    let ev = events(
        &mut s,
        P::SetPassengers(SetPassengers {
            vehicle: 40,
            passengers: vec![PLAYER],
        }),
    );
    assert_eq!(s.player.vehicle, Some(40));
    assert_eq!(s.player.physics.yaw, 135.0);
    let hint = s.titles.action_bar.as_ref().map(|c| c.content.clone());
    assert!(matches!(
        hint,
        Some(mcr_protocol::types::text::Content::Translatable { key, .. }) if key == "mount.onboard"
    ));
    assert!(ev.iter().any(|e| matches!(e, ClientEvent::PlayerRotated)));
    s.titles.action_bar = None;
    run(
        &mut s,
        P::SetPassengers(SetPassengers {
            vehicle: 40,
            passengers: vec![PLAYER],
        }),
    );
    assert!(
        s.titles.action_bar.is_none(),
        "no hint while already riding"
    );
}

#[test]
fn immediate_respawn_requests_a_respawn_on_death() {
    let mut s = state();
    run(&mut s, game_event(11, 1.0));
    assert!(!s.player.show_death_screen);
    let out = run(
        &mut s,
        P::PlayerCombatKill(PlayerCombatKill {
            player_id: PLAYER,
            message: Component::text("x"),
        }),
    );
    assert!(matches!(out.replies.as_slice(), [S::ClientCommand(_)]));
    let mut shown = state();
    let out = run(
        &mut shown,
        P::PlayerCombatKill(PlayerCombatKill {
            player_id: PLAYER,
            message: Component::text("x"),
        }),
    );
    assert!(out.replies.is_empty());
}

#[test]
fn only_web_links_are_trusted_and_report_details_are_kept() {
    let mut s = state();
    let link = |url: &str| ServerLink {
        label: ServerLinkLabel::Custom(Component::text("l")),
        url: url.into(),
    };
    run(
        &mut s,
        P::ServerLinks(mcr_protocol::common::ServerLinks {
            links: vec![
                link("https://example.org/rules"),
                link("javascript:alert(1)"),
                link("file:///etc/passwd"),
                link("HTTP://EXAMPLE.ORG"),
                link("https://bad host"),
            ],
        }),
    );
    let urls: Vec<&str> = s.server.links.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(urls, ["https://example.org/rules", "HTTP://EXAMPLE.ORG"]);
    run(
        &mut s,
        P::CustomReportDetails(mcr_protocol::common::CustomReportDetails {
            details: mcr_buf::LengthPrefixed(vec![ReportDetail {
                key: "shard".into(),
                value: "eu-3".into(),
            }]),
        }),
    );
    assert_eq!(s.server.report_details, [("shard".into(), "eu-3".into())]);
}

#[test]
fn low_disk_space_raises_a_toast() {
    let mut s = state();
    let ev = events(
        &mut s,
        P::LowDiskSpaceWarning(mcr_protocol::play::clientbound::player::LowDiskSpaceWarning),
    );
    assert_eq!(ev, [ClientEvent::Toast(ToastEvent::LowDiskSpace)]);
}

#[test]
fn frozen_worlds_hold_entities_time_and_border_still() {
    let mut s = state();
    let arrow = mcr_data::v26_3()
        .entity_type_by_name("minecraft:snowball")
        .unwrap() as i32;
    let mut e = Entity::new(50, Uuid::from_u128(50), arrow, [0.0, 100.0, 0.0]);
    e.sim = s.sim_kinds.get(arrow);
    e.velocity = [1.0, 0.0, 0.0];
    s.entities.insert(e);
    run(
        &mut s,
        P::TickingState(TickingState {
            tick_rate: 20.0,
            is_frozen: true,
        }),
    );
    run(
        &mut s,
        P::SetBorderLerpSize(SetBorderLerpSize {
            old_size: 100.0,
            new_size: 50.0,
            lerp_time: 100,
        }),
    );
    let time = s.world.game_time;
    for _ in 0..5 {
        s.tick();
    }
    assert!(!s.world.runs_normally);
    assert_eq!(s.world.game_time, time);
    assert_eq!(s.world.border.size, 100.0);
    assert_eq!(s.entities.get(50).unwrap().pos[0], 0.0);
    run(&mut s, P::TickingStep(TickingStep { tick_steps: 2 }));
    s.tick();
    s.tick();
    assert_eq!(s.world.game_time, time + 2);
    assert!(s.entities.get(50).unwrap().pos[0] > 0.0);
    s.tick();
    assert_eq!(s.world.game_time, time + 2);
}

#[test]
fn slow_tick_rates_stretch_the_client_tick_but_fast_ones_do_not() {
    let mut s = state();
    assert_eq!(s.world.tick_seconds(), 0.05);
    run(
        &mut s,
        P::TickingState(TickingState {
            tick_rate: 10.0,
            is_frozen: false,
        }),
    );
    assert!((s.world.tick_seconds() - 0.1).abs() < 1e-6);
    run(
        &mut s,
        P::TickingState(TickingState {
            tick_rate: 100.0,
            is_frozen: false,
        }),
    );
    assert_eq!(s.world.tick_seconds(), 0.05);
}

#[test]
fn border_lerps_over_ticks_and_warns_near_the_edge() {
    let mut s = state();
    run(
        &mut s,
        P::SetBorderLerpSize(SetBorderLerpSize {
            old_size: 100.0,
            new_size: 50.0,
            lerp_time: 200,
        }),
    );
    for _ in 0..100 {
        s.tick();
    }
    assert!((s.world.border.size - 75.0).abs() < 1e-9);
    assert!((s.world.border.lerp_speed() - 0.25).abs() < 1e-9);
    let b = &s.world.border;
    assert_eq!(b.warning_strength(0.0, 0.0), 0.0);
    assert!(b.warning_strength(37.0, 0.0) > 0.5);
    let mut b = b.clone();
    b.set_center(f64::NAN, 1.0);
    assert_eq!(b.center_x, 0.0);
    b.set_lerp(f64::INFINITY, -5.0, 10);
    assert!(b.size.is_finite() && b.target_size == 0.0);
}

#[test]
fn cooldown_overlay_shrinks_within_a_tick() {
    let mut s = state();
    let pearl = mcr_data::v26_3()
        .item_by_name("minecraft:ender_pearl")
        .unwrap() as i32;
    let stack = ItemStack::new(pearl, 1);
    run(
        &mut s,
        P::Cooldown(mcr_protocol::play::clientbound::ui::Cooldown {
            cooldown_group: Identifier::parse("minecraft:ender_pearl").unwrap(),
            duration: 20,
        }),
    );
    assert_eq!(s.cooldown_percent(&stack, 0.0), 1.0);
    assert!((s.cooldown_percent(&stack, 0.5) - 0.975).abs() < 1e-6);
    for _ in 0..20 {
        s.tick();
    }
    assert_eq!(s.cooldown_percent(&stack, 0.0), 0.0);
}

#[test]
fn set_camera_follows_a_known_entity_until_it_goes_away() {
    use mcr_protocol::play::clientbound::entity::RemoveEntities;
    use mcr_protocol::play::clientbound::world::SetCamera;
    let mut s = state();
    s.entities.insert(Entity::new(30, Uuid::nil(), 0, [0.0; 3]));
    run(&mut s, P::SetCamera(SetCamera { camera_id: 99 }));
    assert_eq!(s.camera_entity, None);
    run(&mut s, P::SetCamera(SetCamera { camera_id: 30 }));
    assert_eq!(s.camera_entity, Some(30));
    run(&mut s, P::SetCamera(SetCamera { camera_id: PLAYER }));
    assert_eq!(s.camera_entity, None);
    run(&mut s, P::SetCamera(SetCamera { camera_id: 30 }));
    run(
        &mut s,
        P::RemoveEntities(RemoveEntities {
            entity_ids: vec![30],
        }),
    );
    assert_eq!(s.camera_entity, None);
}

#[test]
fn post_effects_replace_the_list_and_stay_bounded() {
    use mcr_protocol::common::PostEffects;
    let mut s = state();
    let ids = |n: usize| {
        (0..n)
            .map(|i| Identifier::parse(&format!("minecraft:e{i}")).unwrap())
            .collect()
    };
    run(
        &mut s,
        P::PostEffects(PostEffects {
            post_effects: ids(2),
        }),
    );
    assert_eq!(s.post_effects, ["minecraft:e0", "minecraft:e1"]);
    run(
        &mut s,
        P::PostEffects(PostEffects {
            post_effects: ids(1000),
        }),
    );
    assert_eq!(s.post_effects.len(), crate::state::MAX_POST_EFFECTS);
    run(
        &mut s,
        P::PostEffects(PostEffects {
            post_effects: Vec::new(),
        }),
    );
    assert!(s.post_effects.is_empty());
}

#[test]
fn custom_chat_completions_add_remove_set_and_complete() {
    use mcr_protocol::play::clientbound::player::{CompletionAction, CustomChatCompletions};
    let mut s = state();
    let send = |s: &mut ClientState, action, names: &[&str]| {
        run(
            s,
            P::CustomChatCompletions(CustomChatCompletions {
                action,
                entries: names.iter().map(|n| (*n).to_owned()).collect(),
            }),
        );
    };
    send(
        &mut s,
        CompletionAction::Add,
        &["Alpha", "alpine", "Beta", "Alpha"],
    );
    assert_eq!(s.chat_completions.len(), 3);
    send(&mut s, CompletionAction::Remove, &["Beta"]);
    assert_eq!(s.chat_completions, ["Alpha", "alpine"]);
    let (start, found) = crate::complete_chat(&s.chat_completions, "hello al", 8);
    assert_eq!(
        (start, found),
        (6, vec!["Alpha".to_owned(), "alpine".to_owned()])
    );
    send(&mut s, CompletionAction::Set, &["Zed"]);
    assert_eq!(s.chat_completions, ["Zed"]);
    let flood: Vec<String> = (0..5000).map(|i| format!("n{i}")).collect();
    run(
        &mut s,
        P::CustomChatCompletions(CustomChatCompletions {
            action: CompletionAction::Set,
            entries: flood,
        }),
    );
    assert!(s.chat_completions.len() <= crate::state::MAX_CHAT_COMPLETIONS);
}

#[test]
fn transient_blocks_live_for_a_second_and_replace_by_position() {
    use mcr_buf::BlockPos;
    use mcr_protocol::play::clientbound::world::AddTransientBlock;
    let mut s = state();
    let add = |s: &mut ClientState, x, state| {
        run(
            s,
            P::AddTransientBlock(AddTransientBlock {
                pos: BlockPos { x, y: 64, z: 0 },
                block_state: state,
            }),
        );
    };
    add(&mut s, 1, 10);
    add(&mut s, 1, 11);
    add(&mut s, 2, 12);
    assert_eq!(s.transient_blocks.len(), 2);
    assert!(s.transient_blocks.iter().any(|b| b.state == 11));
    assert!(!s.transient_blocks.iter().any(|b| b.state == 10));
    for _ in 0..19 {
        s.tick();
    }
    assert_eq!(s.transient_blocks.len(), 2);
    s.tick();
    assert!(s.transient_blocks.is_empty());
    for x in 0..1000 {
        add(&mut s, x, 1);
    }
    assert!(s.transient_blocks.len() <= crate::state::MAX_TRANSIENT_BLOCKS);
}

#[test]
fn leash_knots_sit_on_the_fence_post() {
    use mcr_protocol::play::clientbound::entity::{AddEntity, LpVec3};
    let mut s = state();
    let knot = mcr_data::v26_3()
        .entity_type_by_name("minecraft:leash_knot")
        .unwrap() as i32;
    run(
        &mut s,
        P::AddEntity(AddEntity {
            entity_id: 60,
            uuid: Uuid::from_u128(60),
            entity_type: knot,
            x: 3.0,
            y: -60.0,
            z: -4.0,
            velocity: LpVec3::default(),
            x_rot: mcr_buf::Angle(0),
            y_rot: mcr_buf::Angle(0),
            y_head_rot: mcr_buf::Angle(0),
            data: 0,
        }),
    );
    assert_eq!(s.entities.get(60).unwrap().pos, [3.5, -59.625, -3.5]);
}
