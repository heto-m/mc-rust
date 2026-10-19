use mcr_buf::BlockPos;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::player::{
    PlayerInfoAction, PlayerInfoEntry, PlayerInfoUpdate,
};
use mcr_protocol::play::clientbound::ui::{
    CollisionRule, NameTagVisibility, TeamAction, TeamColor, TeamParameters,
};
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::text::Component;
use uuid::Uuid;

use crate::client::Client;
use crate::combat::CombatKeys;
use crate::spectator::{MenuItem, SpectatorMenu};
use crate::use_item::UseTarget;

fn player(c: &mut Client, id: u128, name: &str, mode: GameType) {
    let mut entry = PlayerInfoEntry::new(Uuid::from_u128(id));
    entry.profile.name = name.into();
    entry.game_mode = mode;
    entry.listed = true;
    c.state.players.apply(&PlayerInfoUpdate::with_actions(
        &[
            PlayerInfoAction::AddPlayer,
            PlayerInfoAction::UpdateGameMode,
            PlayerInfoAction::UpdateListed,
        ],
        vec![entry],
    ));
}

fn team(c: &mut Client, name: &str, players: &[&str]) {
    c.state.scoreboard.apply_team(
        name,
        &TeamAction::Add {
            parameters: TeamParameters {
                display_name: Component::text(name),
                player_prefix: Component::text(""),
                player_suffix: Component::text(""),
                name_tag_visibility: NameTagVisibility::Always,
                collision_rule: CollisionRule::Always,
                color: Some(TeamColor::Red),
                options: 0,
            },
            players: players.iter().map(|p| (*p).to_owned()).collect(),
        },
    );
}

fn spectator() -> Client {
    let mut c = Client::for_test();
    c.state.player.game_mode = GameType::Spectator;
    c
}

fn teleports(sent: &[S<'_>]) -> Vec<Uuid> {
    sent.iter()
        .filter_map(|p| match p {
            S::TeleportToEntity(t) => Some(t.target),
            _ => None,
        })
        .collect()
}

#[test]
fn menu_teleports_to_a_player_in_two_presses() {
    let mut c = spectator();
    player(&mut c, 3, "Steve", GameType::Survival);
    player(&mut c, 2, "Alex", GameType::Creative);
    player(&mut c, 1, "Ghost", GameType::Spectator);
    c.spectator_hotbar_key(4).unwrap();
    let menu = c.spectator_menu().unwrap();
    assert_eq!(
        menu.selected_slot(),
        -1,
        "the first key only opens the menu"
    );
    assert!(matches!(
        menu.item(0),
        MenuItem::TeleportToPlayer { enabled: true }
    ));
    assert!(matches!(
        menu.item(1),
        MenuItem::TeleportToTeam { enabled: false }
    ));
    assert_eq!(menu.item(8), MenuItem::Close);

    c.spectator_hotbar_key(0).unwrap();
    c.spectator_hotbar_key(0).unwrap();
    let menu = c.spectator_menu().unwrap();
    let names: Vec<String> = (0..3)
        .map(|s| menu.item(s).name().to_plain_string())
        .collect();
    assert_eq!(
        names,
        ["Alex", "Steve", ""],
        "spectators are left out, sorted by UUID"
    );

    c.spectator_hotbar_key(1).unwrap();
    assert!(teleports(&c.take_sent()).is_empty());
    c.spectator_hotbar_key(1).unwrap();
    assert_eq!(teleports(&c.take_sent()), [Uuid::from_u128(3)]);

    c.spectator_hotbar_key(8).unwrap();
    c.spectator_hotbar_key(8).unwrap();
    assert!(c.spectator_menu().is_none());
}

#[test]
fn menu_pages_hold_six_items_after_the_first() {
    let mut c = spectator();
    for i in 0..15u128 {
        player(&mut c, i + 1, &format!("P{i:02}"), GameType::Survival);
    }
    let mut menu = SpectatorMenu::new(&c.state.players, &c.state.scoreboard);
    menu.select_slot(0);
    menu.select_slot(0);
    let name = |m: &SpectatorMenu, s| m.item(s).name().to_plain_string();
    assert_eq!(name(&menu, 0), "P00");
    assert_eq!(name(&menu, 6), "P06");
    assert_eq!(menu.item(7), MenuItem::ScrollRight { enabled: true });
    menu.select_slot(7);
    menu.select_slot(7);
    assert_eq!(menu.item(0), MenuItem::ScrollLeft);
    assert_eq!(name(&menu, 1), "P07");
    assert_eq!(name(&menu, 6), "P12");
    menu.select_slot(7);
    assert_eq!(name(&menu, 1), "P13");
    assert_eq!(name(&menu, 2), "P14");
    assert_eq!(menu.item(3), MenuItem::Empty);
    assert_eq!(menu.item(7), MenuItem::ScrollRight { enabled: false });
}

#[test]
fn team_category_lists_teams_with_players_then_their_members() {
    let mut c = spectator();
    player(&mut c, 1, "Alex", GameType::Survival);
    player(&mut c, 2, "Ghost", GameType::Spectator);
    team(&mut c, "red", &["Alex", "Nobody"]);
    team(&mut c, "empty", &["Ghost"]);
    let mut menu = SpectatorMenu::new(&c.state.players, &c.state.scoreboard);
    assert!(menu.item(1).enabled());
    menu.select_slot(1);
    menu.select_slot(1);
    assert!(matches!(menu.item(0), MenuItem::Team(t) if t.color == Some(0xFF5555)));
    assert_eq!(menu.item(1), MenuItem::Empty);
    menu.select_slot(0);
    menu.select_slot(0);
    assert_eq!(menu.item(0).name().to_plain_string(), "Alex");
}

#[test]
fn scroll_moves_the_menu_or_changes_fly_speed() {
    let mut c = spectator();
    c.state.player.physics.abilities.flying_speed = 0.05;
    c.spectator_scroll(1);
    c.spectator_scroll(1);
    assert!((c.state.player.physics.abilities.flying_speed - 0.06).abs() < 1e-6);
    for _ in 0..100 {
        c.spectator_scroll(-1);
    }
    assert_eq!(c.state.player.physics.abilities.flying_speed, 0.0);

    player(&mut c, 1, "Alex", GameType::Survival);
    c.spectator_hotbar_action().unwrap();
    assert!(
        c.spectator_scroll(-1),
        "wheel down selects the first enabled slot"
    );
    assert_eq!(c.spectator_menu().unwrap().selected_slot(), 0);
    assert!(c.spectator_scroll(-1), "the disabled team slot is skipped");
    assert_eq!(c.spectator_menu().unwrap().selected_slot(), 8);
    assert!(!c.spectator_scroll(-1));
    assert_eq!(c.state.player.physics.abilities.flying_speed, 0.0);
}

#[test]
fn left_click_spectates_and_right_click_looks_into_blocks() {
    let mut c = spectator();
    let click = |attack: bool| CombatKeys {
        attack_clicks: u8::from(attack),
        use_clicks: u8::from(!attack),
        ..CombatKeys::default()
    };
    let entity = UseTarget::Entity {
        id: 9,
        location: [0.0, 1.0, 0.0],
    };
    c.handle_keybinds(click(true), entity, false).unwrap();
    c.handle_keybinds(click(true), UseTarget::Air, false)
        .unwrap();
    let sent = c.take_sent();
    let spectates: Vec<Option<i32>> = sent
        .iter()
        .filter_map(|p| match p {
            S::SpectatorAction(a) => Some(a.spectate_entity_id),
            _ => None,
        })
        .collect();
    assert_eq!(spectates, [Some(9), None]);
    assert!(!sent.iter().any(|p| matches!(p, S::Attack(_) | S::Punch(_))));

    let chest = UseTarget::Block {
        pos: BlockPos::new(1, 64, 1),
        face: Facing::Up,
        cursor: [0.5, 1.0, 0.5],
        inside: false,
    };
    c.handle_keybinds(click(false), chest, false).unwrap();
    c.handle_keybinds(click(false), entity, false).unwrap();
    let sent = c.take_sent();
    assert_eq!(
        sent.iter().filter(|p| matches!(p, S::UseItemOn(_))).count(),
        1
    );
    assert_eq!(
        sent.iter().filter(|p| matches!(p, S::Interact(_))).count(),
        2
    );
    assert!(!sent.iter().any(|p| matches!(p, S::UseItem(_))));
}
