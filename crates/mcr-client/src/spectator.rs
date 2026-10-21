//! Vanilla `SpectatorGui` and `SpectatorMenu`: the spectator's hotbar of players and teams to
//! teleport to, plus the spectate action and the fly-speed scroll.

use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::text::Component;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::client::Client;
use crate::error::Result;
use crate::social::{PlayerInfo, PlayerList, Scoreboard, team_named_color};

pub const MENU_SLOTS: i32 = 9;
const ITEMS_PER_PAGE: usize = 6;
const MAX_ENTRIES: usize = 1024;
const MAX_FLY_SPEED: f32 = 0.2;
const FLY_SPEED_STEP: f32 = 0.005;

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerEntry {
    pub uuid: Uuid,
    pub name: String,
    pub spectator: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamEntry {
    pub display_name: Component,
    /// The team colour as 0xRRGGBB, drawn behind the icon.
    pub color: Option<u32>,
    /// The member whose face is the team's icon, picked at random when the menu opens.
    pub icon: Uuid,
    pub players: Vec<PlayerEntry>,
}

#[derive(Debug, Clone, PartialEq)]
enum Category {
    Root {
        players: Vec<PlayerEntry>,
        teams: Vec<TeamEntry>,
    },
    Players(Vec<PlayerEntry>),
    Teams(Vec<TeamEntry>),
}

impl Category {
    fn len(&self) -> usize {
        match self {
            Self::Root { .. } => 2,
            Self::Players(p) => p.len(),
            Self::Teams(t) => t.len(),
        }
    }

    fn item(&self, index: usize) -> MenuItem<'_> {
        match self {
            Self::Root { players, teams } => match index {
                0 => MenuItem::TeleportToPlayer {
                    enabled: !players.is_empty(),
                },
                _ => MenuItem::TeleportToTeam {
                    enabled: !teams.is_empty(),
                },
            },
            Self::Players(p) => MenuItem::Player(&p[index]),
            Self::Teams(t) => MenuItem::Team(&t[index]),
        }
    }

    fn prompt(&self) -> Component {
        Component::translatable(
            match self {
                Self::Root { .. } => "spectatorMenu.root.prompt",
                Self::Players(_) => "spectatorMenu.teleport.prompt",
                Self::Teams(_) => "spectatorMenu.team_teleport.prompt",
            },
            vec![],
        )
    }
}

/// One slot of the menu bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuItem<'a> {
    TeleportToPlayer { enabled: bool },
    TeleportToTeam { enabled: bool },
    Player(&'a PlayerEntry),
    Team(&'a TeamEntry),
    ScrollLeft,
    ScrollRight { enabled: bool },
    Close,
    Empty,
}

impl MenuItem<'_> {
    pub fn enabled(&self) -> bool {
        match *self {
            Self::TeleportToPlayer { enabled }
            | Self::TeleportToTeam { enabled }
            | Self::ScrollRight { enabled } => enabled,
            Self::Player(p) => !p.spectator,
            Self::Team(_) | Self::ScrollLeft | Self::Close => true,
            Self::Empty => false,
        }
    }

    pub fn name(&self) -> Component {
        let key = match self {
            Self::TeleportToPlayer { .. } => "spectatorMenu.teleport",
            Self::TeleportToTeam { .. } => "spectatorMenu.team_teleport",
            Self::Player(p) => return Component::text(p.name.clone()),
            Self::Team(t) => return t.display_name.clone(),
            Self::ScrollLeft => "spectatorMenu.previous_page",
            Self::ScrollRight { .. } => "spectatorMenu.next_page",
            Self::Close => "spectatorMenu.close",
            Self::Empty => return Component::text(""),
        };
        Component::translatable(key, vec![])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Teleport(Uuid),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpectatorMenu {
    category: Category,
    selected: i32,
    page: usize,
}

impl SpectatorMenu {
    /// The root menu over a snapshot of the tab list and the scoreboard teams.
    pub fn new(players: &PlayerList, board: &Scoreboard) -> Self {
        let mut listed: Vec<PlayerEntry> = players
            .iter()
            .filter(|p| p.listed && p.game_mode != GameType::Spectator)
            .take(MAX_ENTRIES)
            .map(entry)
            .collect();
        listed.sort_by_key(|p| p.uuid);
        let by_name: FxHashMap<&str, &PlayerInfo> = players
            .iter()
            .take(MAX_ENTRIES)
            .map(|p| (p.name.as_str(), p))
            .collect();
        let names = fastutil_order(board.teams.keys().take(MAX_ENTRIES).map(String::as_str));
        let teams = names
            .into_iter()
            .filter_map(|name| {
                let team = &board.teams[name.as_str()];
                let mut members: Vec<&PlayerInfo> = team
                    .players
                    .iter()
                    .take(MAX_ENTRIES)
                    .filter_map(|m| by_name.get(m.as_str()).copied())
                    .filter(|p| p.game_mode != GameType::Spectator)
                    .collect();
                members.sort_by_key(|p| p.uuid);
                let icon = members.get(random_below(members.len()))?.uuid;
                let players: Vec<PlayerEntry> = members.into_iter().map(entry).collect();
                Some(TeamEntry {
                    display_name: team.parameters.display_name.clone(),
                    color: team.parameters.color.map(|c| team_named_color(c).rgb()),
                    icon,
                    players,
                })
            })
            .collect();
        Self {
            category: Category::Root {
                players: listed,
                teams,
            },
            selected: -1,
            page: 0,
        }
    }

    /// `SpectatorMenu#getItem`: pages after the first give slot 0 to the back arrow.
    pub fn item(&self, slot: i32) -> MenuItem<'_> {
        let index = slot + (self.page * ITEMS_PER_PAGE) as i32;
        let len = self.category.len();
        if self.page > 0 && slot == 0 {
            MenuItem::ScrollLeft
        } else if slot == 7 {
            MenuItem::ScrollRight {
                enabled: usize::try_from(index).is_ok_and(|i| i < len),
            }
        } else if slot == 8 {
            MenuItem::Close
        } else {
            match usize::try_from(index) {
                Ok(i) if i < len => self.category.item(i),
                _ => MenuItem::Empty,
            }
        }
    }

    pub fn selected_slot(&self) -> i32 {
        self.selected
    }

    /// The line above the bar: the selected item's name, or the category's prompt.
    pub fn action_text(&self) -> Component {
        match self.item(self.selected) {
            MenuItem::Empty => self.category.prompt(),
            item => item.name(),
        }
    }

    /// `SpectatorMenu#selectSlot`: the first press selects, a second press on it activates.
    pub fn select_slot(&mut self, slot: i32) -> Option<MenuAction> {
        let item = self.item(slot);
        if item == MenuItem::Empty {
            return None;
        }
        if self.selected != slot || !item.enabled() {
            self.selected = slot;
            return None;
        }
        let next = match item {
            MenuItem::TeleportToPlayer { .. } => match &self.category {
                Category::Root { players, .. } => Category::Players(players.clone()),
                _ => return None,
            },
            MenuItem::TeleportToTeam { .. } => match &self.category {
                Category::Root { teams, .. } => Category::Teams(teams.clone()),
                _ => return None,
            },
            MenuItem::Team(t) => Category::Players(t.players.clone()),
            MenuItem::Player(p) => return Some(MenuAction::Teleport(p.uuid)),
            MenuItem::ScrollLeft => {
                self.page = self.page.saturating_sub(1);
                return None;
            }
            MenuItem::ScrollRight { .. } => {
                self.page += 1;
                return None;
            }
            MenuItem::Close => return Some(MenuAction::Close),
            MenuItem::Empty => return None,
        };
        self.category = next;
        self.selected = -1;
        self.page = 0;
        None
    }

    /// `SpectatorGui#onMouseScrolled`: moves to the next enabled slot; false when there is none.
    pub fn scroll(&mut self, wheel: i32) -> bool {
        if wheel == 0 {
            return false;
        }
        let step = wheel.signum();
        let mut slot = self.selected + step;
        while (0..MENU_SLOTS).contains(&slot) && !self.item(slot).enabled() {
            slot += step;
        }
        if !(0..MENU_SLOTS).contains(&slot) {
            return false;
        }
        self.select_slot(slot);
        true
    }
}

fn random_below(n: usize) -> usize {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    nanos as usize % n.max(1)
}

fn java_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

fn slot_of(name: &str, mask: usize) -> usize {
    let h = java_hash(name).wrapping_mul(0x9E37_79B9u32 as i32) as u32;
    ((h ^ (h >> 16)) as usize) & mask
}

fn place<'a>(table: &mut [Option<&'a str>], name: &'a str) {
    let mask = table.len() - 1;
    let mut at = slot_of(name, mask);
    while table[at].is_some() {
        at = (at + 1) & mask;
    }
    table[at] = Some(name);
}

/// Iteration order of a fastutil `Object2ObjectOpenHashMap` filled in name order, which is how
/// vanilla's `Scoreboard#getPlayerTeams` lists teams.
pub(crate) fn fastutil_order<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut sorted: Vec<&str> = names.collect();
    sorted.sort_unstable();
    let mut table: Vec<Option<&str>> = vec![None; 32];
    for (count, name) in sorted.into_iter().enumerate() {
        place(&mut table, name);
        if count + 1 > table.len() / 4 * 3 {
            let grown = vec![None; table.len() * 2];
            let old = std::mem::replace(&mut table, grown);
            for name in old.into_iter().rev().flatten() {
                place(&mut table, name);
            }
        }
    }
    table
        .into_iter()
        .rev()
        .flatten()
        .map(str::to_owned)
        .collect()
}

fn entry(p: &PlayerInfo) -> PlayerEntry {
    PlayerEntry {
        uuid: p.uuid,
        name: p.name.clone(),
        spectator: p.game_mode == GameType::Spectator,
    }
}

impl Client {
    pub fn spectator_menu(&self) -> Option<&SpectatorMenu> {
        self.spectator_menu.as_ref()
    }

    pub fn close_spectator_menu(&mut self) {
        self.spectator_menu = None;
    }

    fn open_spectator_menu(&mut self) {
        self.spectator_menu = Some(SpectatorMenu::new(
            &self.state.players,
            &self.state.scoreboard,
        ));
    }

    /// `SpectatorGui#onHotbarSelected`: a number key opens the menu, then picks its slots.
    pub fn spectator_hotbar_key(&mut self, slot: u8) -> Result<()> {
        match self.spectator_menu.as_mut() {
            Some(menu) => {
                let action = menu.select_slot(i32::from(slot));
                self.run_menu_action(action)
            }
            None => {
                self.open_spectator_menu();
                Ok(())
            }
        }
    }

    /// `SpectatorGui#onHotbarActionKeyPressed`: the spectator hotbar key activates the selection.
    pub fn spectator_hotbar_action(&mut self) -> Result<()> {
        match self.spectator_menu.as_mut() {
            Some(menu) => {
                let slot = menu.selected_slot();
                let action = if slot != -1 {
                    menu.select_slot(slot)
                } else {
                    None
                };
                self.run_menu_action(action)
            }
            None => {
                self.open_spectator_menu();
                Ok(())
            }
        }
    }

    fn run_menu_action(&mut self, action: Option<MenuAction>) -> Result<()> {
        match action {
            Some(MenuAction::Teleport(target)) => {
                self.send_play(&S::TeleportToEntity(sb::TeleportToEntity { target }))
            }
            Some(MenuAction::Close) => {
                self.spectator_menu = None;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// `MouseHandler#onScroll` for spectators: the open menu takes the wheel, otherwise it changes
    /// the fly speed. True when the menu moved its selection.
    pub fn spectator_scroll(&mut self, wheel: i32) -> bool {
        if let Some(menu) = self.spectator_menu.as_mut() {
            return menu.scroll(-wheel);
        }
        let abilities = &mut self.state.player.physics.abilities;
        abilities.flying_speed =
            (abilities.flying_speed + wheel as f32 * FLY_SPEED_STEP).clamp(0.0, MAX_FLY_SPEED);
        false
    }

    /// `MultiPlayerGameMode#spectate` (on an entity) or `spectatorNoAction`.
    pub(crate) fn spectate(&mut self, entity_id: Option<i32>) -> Result<()> {
        self.send_play(&S::SpectatorAction(sb::SpectatorAction {
            spectate_entity_id: entity_id,
        }))
    }
}
