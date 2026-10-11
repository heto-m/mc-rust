//! Server-pushed state the client keeps for its UI: recipe book, advancements, maps, trades, stats, waypoints.

use mcr_buf::BlockPos;
use mcr_protocol::play::clientbound::entity::{
    TrackedWaypoint, Waypoint, WaypointId, WaypointOperation,
};
use mcr_protocol::play::clientbound::ui::{
    Advancement, MapDecoration, MapItemData, MerchantOffer, MerchantOffers, PositionedAdvancement,
    RecipeBookAdd, RecipeBookEntry, RecipeBookSettings, RecipeDisplay, UpdateAdvancements,
};
use mcr_protocol::types::game::{Hand, SignSide};
use rustc_hash::FxHashMap;

use crate::slot_display::entry_weight;
use crate::toast::{MAX_TOASTS_PER_PACKET, ToastEvent, recipe_toast, stack_of};

pub const MAX_RECIPES: usize = 1 << 13;
pub const MAX_RECIPE_WEIGHT: usize = 1024;
pub const MAX_RECIPE_BOOK_WEIGHT: usize = 1 << 20;
pub const MAX_ADVANCEMENTS: usize = 4096;
pub const MAX_CRITERIA: usize = 1 << 14;
pub const MAX_CRITERIA_PER_ADVANCEMENT: usize = 64;
pub const MAX_MAPS: usize = 256;
pub const MAX_DECORATIONS: usize = 256;
pub const MAX_OFFERS: usize = 256;
pub const MAX_STATS: usize = 1 << 14;
pub const MAX_WAYPOINTS: usize = 1024;
pub const MAP_SIZE: usize = 128;

/// Inserts, evicting an arbitrary other entry when the map is full so the total stays bounded.
pub(crate) fn insert_bounded<K: Clone + Eq + std::hash::Hash, V>(
    map: &mut FxHashMap<K, V>,
    cap: usize,
    key: K,
    value: V,
) {
    if map.len() >= cap
        && !map.contains_key(&key)
        && let Some(victim) = map.keys().next().cloned()
    {
        map.remove(&victim);
    }
    map.insert(key, value);
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecipeBook {
    pub entries: FxHashMap<i32, RecipeBookEntry>,
    pub settings: RecipeBookSettings,
    pub revision: u64,
    weight: usize,
}

impl RecipeBook {
    pub fn add(&mut self, packet: RecipeBookAdd) -> Vec<ToastEvent> {
        self.revision += 1;
        if packet.replace {
            self.entries.clear();
            self.weight = 0;
        }
        let mut toasts = Vec::new();
        for e in packet.entries {
            let weight = entry_weight(&e.contents, MAX_RECIPE_WEIGHT);
            if weight >= MAX_RECIPE_WEIGHT {
                continue;
            }
            if toasts.len() < MAX_TOASTS_PER_PACKET
                && let Some(toast) = recipe_toast(&e)
            {
                toasts.push(toast);
            }
            self.take(e.contents.id);
            while (self.entries.len() >= MAX_RECIPES
                || self.weight + weight > MAX_RECIPE_BOOK_WEIGHT)
                && let Some(&victim) = self.entries.keys().next()
            {
                self.take(victim);
            }
            self.weight += weight;
            self.entries.insert(e.contents.id, e);
        }
        toasts
    }

    pub fn remove(&mut self, ids: impl IntoIterator<Item = i32>) {
        self.revision += 1;
        for id in ids {
            self.take(id);
        }
    }

    pub fn weight(&self) -> usize {
        self.weight
    }

    fn take(&mut self, id: i32) {
        if let Some(old) = self.entries.remove(&id) {
            self.weight -= entry_weight(&old.contents, MAX_RECIPE_WEIGHT);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Advancements {
    pub defs: FxHashMap<String, PositionedAdvancement>,
    pub progress: FxHashMap<String, FxHashMap<String, Option<i64>>>,
    pub selected_tab: Option<String>,
    pub show: bool,
    pub revision: u64,
    /// Definition ids in the order the server sent them, which is the tab order.
    pub order: Vec<String>,
}

impl Advancements {
    /// Returns toasts for advancements this update completed, never on the initial sync.
    pub fn apply(&mut self, p: UpdateAdvancements) -> Vec<ToastEvent> {
        let mut toasts = Vec::new();
        self.revision += 1;
        let reset = p.reset;
        let touched: Vec<(String, bool)> = p
            .progress
            .0
            .iter()
            .map(|(id, _)| (id.as_str().to_owned(), self.is_done(id.as_str())))
            .collect();
        if p.reset {
            self.defs.clear();
            self.progress.clear();
            self.order.clear();
        }
        for id in p.removed {
            self.defs.remove(id.as_str());
            self.progress.remove(id.as_str());
        }
        for a in p.added {
            let key = a.id.as_str().to_owned();
            if !self.defs.contains_key(&key) {
                self.order.push(key.clone());
            }
            insert_bounded(&mut self.defs, MAX_ADVANCEMENTS, key, a);
        }
        self.order.retain(|id| self.defs.contains_key(id));
        self.progress.retain(|id, _| self.defs.contains_key(id));
        let mut total: usize = self.progress.values().map(FxHashMap::len).sum();
        for (id, criteria) in p.progress.0 {
            let key = id.as_str().to_owned();
            if !self.defs.contains_key(&key) {
                continue;
            }
            let entry = self.progress.entry(key).or_default();
            for (name, obtained) in criteria.0 {
                let new = !entry.contains_key(&name);
                if new && (entry.len() >= MAX_CRITERIA_PER_ADVANCEMENT || total >= MAX_CRITERIA) {
                    continue;
                }
                total += usize::from(new);
                entry.insert(name, obtained);
            }
        }
        self.show = p.show_advancements;
        if !reset {
            for (id, was_done) in touched {
                if was_done || !self.is_done(&id) {
                    continue;
                }
                let Some(display) = self.advancement(&id).and_then(|a| a.display.as_ref()) else {
                    continue;
                };
                if display.show_toast && toasts.len() < MAX_TOASTS_PER_PACKET {
                    toasts.push(ToastEvent::Advancement {
                        title: display.title.clone(),
                        frame: display.frame,
                        icon: stack_of(&display.icon),
                    });
                }
            }
        }
        toasts
    }

    pub fn ordered_defs(&self) -> impl Iterator<Item = &PositionedAdvancement> {
        self.order.iter().filter_map(|id| self.defs.get(id))
    }

    pub fn advancement(&self, id: &str) -> Option<&Advancement> {
        self.defs.get(id).map(|a| &a.advancement)
    }

    pub fn is_done(&self, id: &str) -> bool {
        let Some(a) = self.advancement(id) else {
            return false;
        };
        let Some(progress) = self.progress.get(id) else {
            return false;
        };
        a.requirements.iter().all(|group| {
            group
                .iter()
                .any(|c| matches!(progress.get(c), Some(Some(_))))
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapData {
    pub scale: i8,
    pub locked: bool,
    pub decorations: Vec<MapDecoration>,
    pub colors: Vec<u8>,
}

impl Default for MapData {
    fn default() -> Self {
        Self {
            scale: 0,
            locked: false,
            decorations: Vec::new(),
            colors: vec![0; MAP_SIZE * MAP_SIZE],
        }
    }
}

impl MapData {
    pub fn apply(&mut self, p: MapItemData) {
        self.scale = p.scale;
        self.locked = p.locked;
        if let Some(d) = p.decorations {
            self.decorations = d.into_iter().take(MAX_DECORATIONS).collect();
        }
        if let Some(patch) = p.color_patch {
            let (w, h) = (usize::from(patch.width), usize::from(patch.height));
            let (sx, sy) = (usize::from(patch.start_x), usize::from(patch.start_y));
            for row in 0..h {
                for col in 0..w {
                    let (x, y) = (sx + col, sy + row);
                    if x < MAP_SIZE
                        && y < MAP_SIZE
                        && let Some(&c) = patch.map_colors.get(row * w + col)
                    {
                        self.colors[y * MAP_SIZE + x] = c;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Merchant {
    pub container_id: i32,
    pub offers: Vec<MerchantOffer>,
    pub villager_level: i32,
    pub villager_xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

impl From<MerchantOffers> for Merchant {
    fn from(p: MerchantOffers) -> Self {
        Self {
            container_id: p.container_id,
            offers: p.offers.into_iter().take(MAX_OFFERS).collect(),
            villager_level: p.villager_level,
            villager_xp: p.villager_xp,
            show_progress: p.show_progress,
            can_restock: p.can_restock,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Waypoints {
    pub tracked: FxHashMap<String, TrackedWaypoint>,
}

impl Waypoints {
    fn key(id: &WaypointId) -> String {
        match id {
            WaypointId::Uuid(u) => u.to_string(),
            WaypointId::Name(n) => format!("name:{n}"),
        }
    }

    pub fn apply(&mut self, p: Waypoint) {
        let key = Self::key(&p.waypoint.id);
        match p.operation {
            WaypointOperation::Untrack => {
                self.tracked.remove(&key);
            }
            _ => {
                insert_bounded(&mut self.tracked, MAX_WAYPOINTS, key, p.waypoint);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignEditor {
    pub pos: BlockPos,
    pub side: SignSide,
}

/// The recipe the server wants outlined in the open crafting grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GhostRecipe {
    pub revision: u64,
    pub container_id: i32,
    pub display: RecipeDisplay,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameplayState {
    pub recipe_book: RecipeBook,
    pub advancements: Advancements,
    pub maps: FxHashMap<i32, MapData>,
    pub merchant: Option<Merchant>,
    pub stats: FxHashMap<(i32, i32), i32>,
    pub stats_revision: u64,
    pub ghost_recipe: Option<GhostRecipe>,
    pub ghost_counter: u64,
    pub waypoints: Waypoints,
    pub sign_editor: Option<SignEditor>,
    pub book_editor: Option<Hand>,
}

impl GameplayState {
    pub fn apply_map(&mut self, p: MapItemData) {
        if !self.maps.contains_key(&p.map_id) {
            insert_bounded(&mut self.maps, MAX_MAPS, p.map_id, MapData::default());
        }
        if let Some(map) = self.maps.get_mut(&p.map_id) {
            map.apply(p);
        }
    }

    pub fn set_ghost_recipe(&mut self, container_id: i32, display: RecipeDisplay) {
        self.ghost_counter += 1;
        self.ghost_recipe = Some(GhostRecipe {
            revision: self.ghost_counter,
            container_id,
            display,
        });
    }

    pub fn set_stat(&mut self, kind: i32, stat: i32, value: i32) {
        insert_bounded(&mut self.stats, MAX_STATS, (kind, stat), value);
    }
}
