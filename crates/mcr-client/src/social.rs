use std::collections::VecDeque;
use std::time::Instant;

use mcr_nbt::Value;
use mcr_protocol::play::clientbound::player::{
    PackedSignature, PlayerInfoAction, PlayerInfoUpdate,
};
use mcr_protocol::play::clientbound::ui::{
    BossBarColor, BossBarFlags, BossBarOverlay, DisplaySlot, NumberFormat, ObjectiveDisplay,
    ObjectiveRenderType, TeamAction, TeamColor, TeamParameters,
};
use mcr_protocol::types::Component;
use mcr_protocol::types::game::{GameType, MessageSignature};
use mcr_protocol::types::text::{Color, NamedColor, Style};
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

pub const MAX_TAB_ENTRIES: usize = 80;

impl PlayerList {
    /// Listed players in vanilla's tab order: list order (highest first), spectators last, team name, then name ignoring case.
    pub fn tab_order<'a>(&'a self, board: &Scoreboard) -> Vec<&'a PlayerInfo> {
        let team = |p: &PlayerInfo| board.team_of(&p.name).unwrap_or("");
        let order = |a: &&PlayerInfo, b: &&PlayerInfo| {
            b.list_order
                .cmp(&a.list_order)
                .then_with(|| {
                    (a.game_mode == GameType::Spectator).cmp(&(b.game_mode == GameType::Spectator))
                })
                .then_with(|| team(a).cmp(team(b)))
                .then_with(|| caseless_cmp(&a.name, &b.name))
        };
        let mut list: Vec<&PlayerInfo> = self.players.values().filter(|p| p.listed).collect();
        if list.len() > MAX_TAB_ENTRIES {
            list.select_nth_unstable_by(MAX_TAB_ENTRIES, order);
            list.truncate(MAX_TAB_ENTRIES);
        }
        list.sort_by(order);
        list
    }
}

/// The tab list name: the server's display name, else the team-formatted profile name; spectators in italics.
pub fn tab_name(info: &PlayerInfo, board: &Scoreboard) -> Component {
    let name = match &info.display_name {
        Some(d) => d.clone(),
        None => board.format_member(&info.name, Component::text(info.name.clone())),
    };
    if info.game_mode == GameType::Spectator {
        Component::text("")
            .styled(Style {
                italic: Some(true),
                ..Style::default()
            })
            .with(name)
    } else {
        name
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabList {
    pub header: Component,
    pub footer: Component,
}

pub const OVERLAY_TICKS: i32 = 60;
const DEFAULT_TITLE_TIMES: (i32, i32, i32) = (10, 70, 20);

/// Title, subtitle and action bar with vanilla's tick-driven timing.
#[derive(Debug, Clone, PartialEq)]
pub struct Titles {
    pub title: Option<Component>,
    pub subtitle: Option<Component>,
    pub action_bar: Option<Component>,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
    /// Ticks left on screen; the title shows only while this is positive.
    pub title_time: i32,
    pub overlay_time: i32,
}

impl Default for Titles {
    fn default() -> Self {
        let (fade_in, stay, fade_out) = DEFAULT_TITLE_TIMES;
        Self {
            title: None,
            subtitle: None,
            action_bar: None,
            fade_in,
            stay,
            fade_out,
            title_time: 0,
            overlay_time: 0,
        }
    }
}

impl Titles {
    fn total(&self) -> i32 {
        self.fade_in
            .saturating_add(self.stay)
            .saturating_add(self.fade_out)
    }

    pub fn set_title(&mut self, title: Component) {
        self.title = Some(title);
        self.title_time = self.total();
    }

    pub fn set_subtitle(&mut self, subtitle: Component) {
        self.subtitle = Some(subtitle);
    }

    pub fn set_overlay(&mut self, text: Component) {
        self.action_bar = Some(text);
        self.overlay_time = OVERLAY_TICKS;
    }

    pub fn set_times(&mut self, fade_in: i32, stay: i32, fade_out: i32) {
        if fade_in >= 0 {
            self.fade_in = fade_in;
        }
        if stay >= 0 {
            self.stay = stay;
        }
        if fade_out >= 0 {
            self.fade_out = fade_out;
        }
        if self.title_time > 0 {
            self.title_time = self.total();
        }
    }

    pub fn clear(&mut self, reset_times: bool) {
        self.title = None;
        self.subtitle = None;
        self.title_time = 0;
        if reset_times {
            (self.fade_in, self.stay, self.fade_out) = DEFAULT_TITLE_TIMES;
        }
    }

    pub fn tick(&mut self) {
        if self.overlay_time > 0 {
            self.overlay_time -= 1;
        }
        if self.title_time > 0 {
            self.title_time -= 1;
            if self.title_time <= 0 {
                self.title = None;
                self.subtitle = None;
            }
        }
    }

    /// Title opacity this frame, `None` when nothing is shown.
    pub fn title_alpha(&self, partial: f32) -> Option<u8> {
        if self.title.is_none() || self.title_time <= 0 {
            return None;
        }
        let t = self.title_time as f32 - partial;
        let mut alpha = 255.0;
        if self.title_time > self.fade_out.saturating_add(self.stay) {
            alpha = (self.total() as f32 - t) * 255.0 / self.fade_in as f32;
        }
        if self.title_time <= self.fade_out {
            alpha = t * 255.0 / self.fade_out as f32;
        }
        let alpha = (alpha as i32).clamp(0, 255) as u8;
        (alpha > 0).then_some(alpha)
    }

    pub fn overlay_alpha(&self, partial: f32) -> Option<u8> {
        if self.action_bar.is_none() || self.overlay_time <= 0 {
            return None;
        }
        let t = self.overlay_time as f32 - partial;
        let alpha = ((t * 255.0 / 20.0) as i32).min(255);
        (alpha > 0).then_some(alpha as u8)
    }
}

const BOSS_LERP_MILLIS: f32 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub struct BossBar {
    pub name: Component,
    /// Target progress as last sent by the server.
    pub progress: f32,
    pub color: BossBarColor,
    pub overlay: BossBarOverlay,
    pub flags: BossBarFlags,
    /// Position in vanilla's insertion-ordered overlay.
    pub order: u64,
    pub prev_progress: f32,
    pub changed_at: Option<Instant>,
}

impl BossBar {
    pub fn new(
        name: Component,
        progress: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        flags: BossBarFlags,
    ) -> Self {
        Self {
            name,
            progress,
            color,
            overlay,
            flags,
            order: 0,
            prev_progress: progress,
            changed_at: None,
        }
    }

    pub fn displayed_progress(&self, now: Instant) -> f32 {
        let Some(at) = self.changed_at else {
            return self.progress;
        };
        let t = (now.saturating_duration_since(at).as_secs_f32() * 1000.0 / BOSS_LERP_MILLIS)
            .clamp(0.0, 1.0);
        self.prev_progress + (self.progress - self.prev_progress) * t
    }

    pub fn set_progress(&mut self, progress: f32, now: Instant) {
        self.prev_progress = self.displayed_progress(now);
        self.progress = progress;
        self.changed_at = Some(now);
    }
}

/// Re-adding a known id keeps its slot, like vanilla's linked map.
pub fn boss_bars_insert(bars: &mut FxHashMap<Uuid, BossBar>, id: Uuid, mut bar: BossBar) -> bool {
    match bars.get(&id) {
        Some(old) => bar.order = old.order,
        None if bars.len() >= MAX_BOSS_BARS => return false,
        None => bar.order = bars.values().map(|b| b.order + 1).max().unwrap_or(0),
    }
    bars.insert(id, bar);
    true
}

/// Boss bars in the order vanilla stacks them.
pub fn boss_bars_ordered(bars: &FxHashMap<Uuid, BossBar>) -> Vec<(&Uuid, &BossBar)> {
    let mut list: Vec<_> = bars.iter().collect();
    list.sort_by_key(|(_, b)| b.order);
    list
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
    member_team: FxHashMap<String, String>,
}

/// The style a score falls back to when neither the score nor its objective sets a number format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreStyle {
    Sidebar,
    PlayerList,
    Plain,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SidebarRow {
    pub name: Component,
    pub score: Component,
}

pub const MAX_SIDEBAR_ROWS: usize = 15;
const SIDEBAR_SLOT: u8 = 1;
const FIRST_TEAM_SLOT: u8 = 3;

fn bounded(name: &str) -> bool {
    name.len() <= MAX_NAME_LEN
}

fn caseless_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

pub fn team_named_color(color: TeamColor) -> NamedColor {
    match color {
        TeamColor::Black => NamedColor::Black,
        TeamColor::DarkBlue => NamedColor::DarkBlue,
        TeamColor::DarkGreen => NamedColor::DarkGreen,
        TeamColor::DarkAqua => NamedColor::DarkAqua,
        TeamColor::DarkRed => NamedColor::DarkRed,
        TeamColor::DarkPurple => NamedColor::DarkPurple,
        TeamColor::Gold => NamedColor::Gold,
        TeamColor::Gray => NamedColor::Gray,
        TeamColor::DarkGray => NamedColor::DarkGray,
        TeamColor::Blue => NamedColor::Blue,
        TeamColor::Green => NamedColor::Green,
        TeamColor::Aqua => NamedColor::Aqua,
        TeamColor::Red => NamedColor::Red,
        TeamColor::LightPurple => NamedColor::LightPurple,
        TeamColor::Yellow => NamedColor::Yellow,
        TeamColor::White => NamedColor::White,
    }
}

/// Applies a score's number format; `None` falls back to `style`.
pub fn format_score(format: Option<&NumberFormat>, style: ScoreStyle, value: i32) -> Component {
    let styled = |color: Option<NamedColor>| {
        Component::text(value.to_string()).styled(Style {
            color: color.map(Color::Named),
            ..Style::default()
        })
    };
    match format {
        Some(NumberFormat::Blank) => Component::text(""),
        Some(NumberFormat::Fixed(c)) => c.clone(),
        Some(NumberFormat::Styled(nbt)) => match &nbt.0 {
            Value::Compound(style) => {
                let mut c = style.clone();
                c.insert("text", value.to_string());
                Component::from_nbt_lenient(&Value::Compound(c))
            }
            _ => styled(None),
        },
        None => styled(match style {
            ScoreStyle::Sidebar => Some(NamedColor::Red),
            ScoreStyle::PlayerList => Some(NamedColor::Yellow),
            ScoreStyle::Plain => None,
        }),
    }
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

    /// Vanilla ignores a change to an objective it never saw added.
    pub fn change_objective(&mut self, name: &str, display: &ObjectiveDisplay) {
        if let Some(o) = self.objectives.get_mut(name) {
            *o = display.into();
        }
    }

    pub fn set_display(&mut self, slot: DisplaySlot, objective: &str) {
        if self.objectives.contains_key(objective) {
            self.display_slots.insert(slot as u8, objective.to_owned());
        } else {
            self.display_slots.remove(&(slot as u8));
        }
    }

    pub fn set_score(&mut self, owner: &str, objective: &str, score: Score) {
        if !self.objectives.contains_key(objective) {
            return;
        }
        let key = (owner.to_owned(), objective.to_owned());
        if bounded(owner) && (self.scores.contains_key(&key) || self.scores.len() < MAX_SCORES) {
            self.scores.insert(key, score);
        }
    }

    /// The sidebar objective for `player`: their team colour's slot wins over the plain sidebar.
    pub fn sidebar_objective(&self, player: &str) -> Option<&str> {
        let team_slot = self
            .team_of(player)
            .and_then(|t| self.teams.get(t)?.parameters.color)
            .and_then(|c| self.display_slots.get(&(FIRST_TEAM_SLOT + c as u8)));
        team_slot
            .or_else(|| self.display_slots.get(&SIDEBAR_SLOT))
            .filter(|o| self.objectives.contains_key(*o))
            .map(String::as_str)
    }

    /// Prefix, name and suffix under the team colour, like vanilla's `PlayerTeam.formatNameForTeam`.
    pub fn format_member(&self, member: &str, name: Component) -> Component {
        let Some(team) = self.team_of(member).and_then(|t| self.teams.get(t)) else {
            return name;
        };
        let p = &team.parameters;
        Component::text("")
            .styled(Style {
                color: p.color.map(|c| Color::Named(team_named_color(c))),
                ..Style::default()
            })
            .with(p.player_prefix.clone())
            .with(name)
            .with(p.player_suffix.clone())
    }

    pub fn format_value(
        &self,
        owner: &str,
        objective: &str,
        style: ScoreStyle,
    ) -> Option<Component> {
        let o = self.objectives.get(objective)?;
        let s = self.scores.get(&(owner.to_owned(), objective.to_owned()))?;
        Some(format_score(
            s.number_format.as_ref().or(o.number_format.as_ref()),
            style,
            s.value,
        ))
    }

    /// The top sidebar rows: highest score first, then owner name ignoring case; `#` owners stay hidden.
    pub fn sidebar_rows(&self, objective: &str) -> Vec<SidebarRow> {
        let Some(o) = self.objectives.get(objective) else {
            return Vec::new();
        };
        let mut rows: Vec<(&str, &Score)> = self
            .scores
            .iter()
            .filter(|((owner, obj), _)| obj == objective && !owner.starts_with('#'))
            .map(|((owner, _), s)| (owner.as_str(), s))
            .collect();
        let order = |a: &(&str, &Score), b: &(&str, &Score)| {
            b.1.value
                .cmp(&a.1.value)
                .then_with(|| caseless_cmp(a.0, b.0))
        };
        if rows.len() > MAX_SIDEBAR_ROWS {
            rows.select_nth_unstable_by(MAX_SIDEBAR_ROWS, order);
            rows.truncate(MAX_SIDEBAR_ROWS);
        }
        rows.sort_by(order);
        rows.into_iter()
            .map(|(owner, s)| {
                let shown = s.display.clone().unwrap_or_else(|| Component::text(owner));
                SidebarRow {
                    name: self.format_member(owner, shown),
                    score: format_score(
                        s.number_format.as_ref().or(o.number_format.as_ref()),
                        ScoreStyle::Sidebar,
                        s.value,
                    ),
                }
            })
            .collect()
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
                self.remove_team(name);
                self.teams.insert(
                    name.to_owned(),
                    Team {
                        parameters: parameters.clone(),
                        players: FxHashSet::default(),
                    },
                );
                self.join(name, players);
            }
            TeamAction::Remove => self.remove_team(name),
            TeamAction::Change(parameters) => {
                if let Some(t) = self.teams.get_mut(name) {
                    t.parameters = parameters.clone();
                }
            }
            TeamAction::Join(players) => {
                if self.teams.contains_key(name) {
                    self.join(name, players);
                }
            }
            TeamAction::Leave(players) => {
                for p in players {
                    if self.member_team.get(p).is_some_and(|t| t == name) {
                        self.member_team.remove(p);
                        if let Some(t) = self.teams.get_mut(name) {
                            t.players.remove(p);
                        }
                    }
                }
            }
        }
    }

    /// A player belongs to one team at most; joining another moves them.
    fn join(&mut self, team: &str, players: &[String]) {
        for p in players {
            if !bounded(p) {
                continue;
            }
            let previous = self.member_team.get(p).cloned();
            if previous.is_none() && self.member_team.len() >= MAX_TEAM_MEMBERS {
                continue;
            }
            if let Some(old) = previous.and_then(|old| self.teams.get_mut(&old)) {
                old.players.remove(p);
            }
            self.member_team.insert(p.clone(), team.to_owned());
            if let Some(t) = self.teams.get_mut(team) {
                t.players.insert(p.clone());
            }
        }
    }

    fn remove_team(&mut self, name: &str) {
        if let Some(team) = self.teams.remove(name) {
            for p in &team.players {
                self.member_team.remove(p);
            }
        }
    }

    pub fn team_of(&self, player: &str) -> Option<&str> {
        self.member_team.get(player).map(String::as_str)
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
