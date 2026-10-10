use std::collections::VecDeque;

use mcr_protocol::play::clientbound::player::{
    PackedSignature, PlayerInfoAction, PlayerInfoUpdate,
};
use mcr_protocol::play::clientbound::ui::{
    BossBarColor, BossBarFlags, BossBarOverlay, DisplaySlot, NumberFormat, ObjectiveDisplay,
    ObjectiveRenderType, TeamAction, TeamParameters,
};
use mcr_protocol::types::Component;
use mcr_protocol::types::game::{GameType, MessageSignature};
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

use crate::events::ChatKind;

const MAX_PLAYERS: usize = 8192;
const MAX_BOSS_BARS: usize = 256;
const MAX_OBJECTIVES: usize = 1024;
const MAX_SCORES: usize = 1 << 16;
const MAX_TEAMS: usize = 4096;
const MAX_TEAM_MEMBERS: usize = 1 << 16;
const MAX_NAME_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub kind: ChatKind,
    pub sender: Option<Uuid>,
    pub content: Component,
    /// Signature as received; never verified in offline mode.
    pub signature: Option<Box<MessageSignature>>,
    pub timestamp_millis: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub history: VecDeque<ChatMessage>,
    capacity: usize,
}

impl ChatState {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            history: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, message: ChatMessage) {
        if self.capacity == 0 {
            self.capacity = 256;
        }
        if self.history.len() >= self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(message);
    }

    pub fn delete(&mut self, signature: &PackedSignature) {
        if let PackedSignature::Full(full) = signature {
            self.history
                .retain(|m| m.signature.as_deref().is_none_or(|s| s.0 != full.0));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub uuid: Uuid,
    pub name: String,
    pub game_mode: GameType,
    pub listed: bool,
    pub latency: i32,
    pub display_name: Option<Component>,
    pub list_order: i32,
    pub show_hat: bool,
    pub has_chat_session: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerList {
    players: FxHashMap<Uuid, PlayerInfo>,
}

impl PlayerList {
    pub fn get(&self, uuid: Uuid) -> Option<&PlayerInfo> {
        self.players.get(&uuid)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.players.values()
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn name_of(&self, uuid: Uuid) -> Option<&str> {
        self.players.get(&uuid).map(|p| p.name.as_str())
    }

    pub fn clear(&mut self) {
        self.players.clear();
    }

    pub fn remove(&mut self, uuids: &[Uuid]) {
        for id in uuids {
            self.players.remove(id);
        }
    }

    pub fn apply(&mut self, update: &PlayerInfoUpdate) {
        let has = |a: PlayerInfoAction| update.actions.get(a.bit());
        for entry in &update.entries {
            if has(PlayerInfoAction::AddPlayer) {
                if !self.players.contains_key(&entry.uuid) && self.players.len() >= MAX_PLAYERS {
                    continue;
                }
                self.players.insert(
                    entry.uuid,
                    PlayerInfo {
                        uuid: entry.uuid,
                        name: entry.profile.name.clone(),
                        game_mode: GameType::Survival,
                        listed: false,
                        latency: 0,
                        display_name: None,
                        list_order: 0,
                        show_hat: false,
                        has_chat_session: false,
                    },
                );
            }
            let Some(info) = self.players.get_mut(&entry.uuid) else {
                continue;
            };
            if has(PlayerInfoAction::InitializeChat) {
                info.has_chat_session = entry.chat_session.is_some();
            }
            if has(PlayerInfoAction::UpdateGameMode) {
                info.game_mode = entry.game_mode;
            }
            if has(PlayerInfoAction::UpdateListed) {
                info.listed = entry.listed;
            }
            if has(PlayerInfoAction::UpdateLatency) {
                info.latency = entry.latency;
            }
            if has(PlayerInfoAction::UpdateDisplayName) {
                info.display_name = entry.display_name.clone();
            }
            if has(PlayerInfoAction::UpdateListOrder) {
                info.list_order = entry.list_order;
            }
            if has(PlayerInfoAction::UpdateHat) {
                info.show_hat = entry.show_hat;
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabList {
    pub header: Component,
    pub footer: Component,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Titles {
    pub title: Option<Component>,
    pub subtitle: Option<Component>,
    pub action_bar: Option<Component>,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

impl Default for Titles {
    fn default() -> Self {
        Self {
            title: None,
            subtitle: None,
            action_bar: None,
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        }
    }
}

impl Titles {
    pub fn clear(&mut self, reset_times: bool) {
        self.title = None;
        self.subtitle = None;
        self.action_bar = None;
        if reset_times {
            self.fade_in = 10;
            self.stay = 70;
            self.fade_out = 20;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossBar {
    pub name: Component,
    pub progress: f32,
    pub color: BossBarColor,
    pub overlay: BossBarOverlay,
    pub flags: BossBarFlags,
}

pub fn boss_bars_insert(bars: &mut FxHashMap<Uuid, BossBar>, id: Uuid, bar: BossBar) -> bool {
    if !bars.contains_key(&id) && bars.len() >= MAX_BOSS_BARS {
        return false;
    }
    bars.insert(id, bar);
    true
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub display_name: Component,
    pub render_type: ObjectiveRenderType,
    pub number_format: Option<NumberFormat>,
}

impl From<&ObjectiveDisplay> for Objective {
    fn from(d: &ObjectiveDisplay) -> Self {
        Self {
            display_name: d.display_name.clone(),
            render_type: d.render_type,
            number_format: d.number_format.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub value: i32,
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub parameters: TeamParameters,
    pub players: FxHashSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Scoreboard {
    pub objectives: FxHashMap<String, Objective>,
    pub scores: FxHashMap<(String, String), Score>,
    pub teams: FxHashMap<String, Team>,
    pub display_slots: FxHashMap<u8, String>,
}

fn bounded(name: &str) -> bool {
    name.len() <= MAX_NAME_LEN
}

impl Scoreboard {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn set_objective(&mut self, name: &str, display: Option<&ObjectiveDisplay>) {
        match display {
            Some(d) => {
                if bounded(name)
                    && (self.objectives.contains_key(name)
                        || self.objectives.len() < MAX_OBJECTIVES)
                {
                    self.objectives.insert(name.to_owned(), d.into());
                }
            }
            None => {
                self.objectives.remove(name);
                self.scores.retain(|(_, objective), _| objective != name);
                self.display_slots.retain(|_, o| o != name);
            }
        }
    }

    pub fn set_display(&mut self, slot: DisplaySlot, objective: &str) {
        if objective.is_empty() {
            self.display_slots.remove(&(slot as u8));
        } else if bounded(objective) {
            self.display_slots.insert(slot as u8, objective.to_owned());
        }
    }

    pub fn set_score(&mut self, owner: &str, objective: &str, score: Score) {
        let key = (owner.to_owned(), objective.to_owned());
        if bounded(owner)
            && bounded(objective)
            && (self.scores.contains_key(&key) || self.scores.len() < MAX_SCORES)
        {
            self.scores.insert(key, score);
        }
    }

    pub fn reset_score(&mut self, owner: &str, objective: Option<&str>) {
        match objective {
            Some(o) => {
                self.scores.remove(&(owner.to_owned(), o.to_owned()));
            }
            None => self.scores.retain(|(who, _), _| who != owner),
        }
    }

    pub fn apply_team(&mut self, name: &str, action: &TeamAction) {
        if !bounded(name) {
            return;
        }
        match action {
            TeamAction::Add {
                parameters,
                players,
            } => {
                if self.teams.len() >= MAX_TEAMS && !self.teams.contains_key(name) {
                    return;
                }
                let mut team = Team {
                    parameters: parameters.clone(),
                    players: FxHashSet::default(),
                };
                for p in players.iter().take(MAX_TEAM_MEMBERS) {
                    team.players.insert(p.clone());
                }
                self.teams.insert(name.to_owned(), team);
            }
            TeamAction::Remove => {
                self.teams.remove(name);
            }
            TeamAction::Change(parameters) => {
                if let Some(t) = self.teams.get_mut(name) {
                    t.parameters = parameters.clone();
                }
            }
            TeamAction::Join(players) => {
                if let Some(t) = self.teams.get_mut(name) {
                    for p in players {
                        if t.players.len() < MAX_TEAM_MEMBERS {
                            t.players.insert(p.clone());
                        }
                    }
                }
            }
            TeamAction::Leave(players) => {
                if let Some(t) = self.teams.get_mut(name) {
                    for p in players {
                        t.players.remove(p);
                    }
                }
            }
        }
    }

    pub fn team_of(&self, player: &str) -> Option<&str> {
        self.teams
            .iter()
            .find(|(_, t)| t.players.contains(player))
            .map(|(name, _)| name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_protocol::play::clientbound::player::PlayerInfoEntry;

    fn msg(text: &str) -> ChatMessage {
        ChatMessage {
            kind: ChatKind::System,
            sender: None,
            content: Component::text(text),
            signature: None,
            timestamp_millis: None,
        }
    }

    #[test]
    fn chat_history_is_bounded() {
        let mut chat = ChatState::with_capacity(3);
        for i in 0..10 {
            chat.push(msg(&i.to_string()));
        }
        assert_eq!(chat.history.len(), 3);
        assert_eq!(chat.history[0].content.to_plain_string(), "7");
    }

    #[test]
    fn player_info_applies_only_flagged_fields() {
        let mut list = PlayerList::default();
        let mut entry = PlayerInfoEntry::new(Uuid::from_u128(1));
        entry.profile.name = "Alex".into();
        list.apply(&PlayerInfoUpdate::with_actions(
            &[PlayerInfoAction::AddPlayer, PlayerInfoAction::UpdateListed],
            vec![PlayerInfoEntry {
                listed: true,
                ..entry.clone()
            }],
        ));
        let mut latency = entry.clone();
        latency.latency = 42;
        latency.listed = false;
        list.apply(&PlayerInfoUpdate::with_actions(
            &[PlayerInfoAction::UpdateLatency],
            vec![latency],
        ));
        let info = list.get(Uuid::from_u128(1)).unwrap();
        assert_eq!(
            (info.latency, info.listed, info.name.as_str()),
            (42, true, "Alex")
        );
        list.remove(&[Uuid::from_u128(1)]);
        assert!(list.is_empty());
    }

    #[test]
    fn removing_an_objective_drops_its_scores_and_slots() {
        let mut sb = Scoreboard::default();
        let display = ObjectiveDisplay {
            display_name: Component::text("Kills"),
            render_type: ObjectiveRenderType::Integer,
            number_format: None,
        };
        sb.set_objective("kills", Some(&display));
        sb.set_score(
            "Bot",
            "kills",
            Score {
                value: 3,
                display: None,
                number_format: None,
            },
        );
        sb.set_display(DisplaySlot::Sidebar, "kills");
        sb.set_objective("kills", None);
        assert!(sb.scores.is_empty() && sb.display_slots.is_empty());
    }
}
