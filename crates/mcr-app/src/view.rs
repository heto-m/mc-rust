//! Adapters between the client state and the UI crate's traits.

use std::cell::RefCell;
use std::rc::Rc;

use mcr_client::{Client, ClientState, Merchant, SuggestSource};
use mcr_protocol::play::clientbound::ui::MerchantOffer;
use mcr_protocol::play::clientbound::ui::{
    PositionedAdvancement, RecipeBookEntry, RecipeBookSettings, RecipeDisplay, SlotDisplay,
};
use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::component::HolderSet;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::item::ItemStack;
use mcr_ui::{
    CommandSuggester, GameView, MapInfo, MerchantInfo, RegistryRef, Suggestion, SuggestionSet,
    TooltipFlags, TooltipLines, TooltipSource, TradeOffer,
};
use rustc_hash::FxHashMap;

pub struct StateView<'a> {
    pub state: &'a ClientState,
    pub operator_items: bool,
    pub advanced_tooltips: bool,
    pub keys: mcr_ui::ScreenKeys,
    pub cache: &'a ViewCache,
}

const MAX_TRADE_OFFERS: usize = 256;
const PLAYER_SCORE_DATA_INDEX: u8 = 16;

/// Owned data the `GameView` borrows, rebuilt only when its source changes.
#[derive(Default)]
pub struct ViewCache {
    merchant_key: Option<(u64, i32)>,
    merchant: Option<MerchantInfo>,
    stonecutter_key: Option<(i32, i32, u64)>,
    stonecutter: Vec<ItemStack>,
    trim_materials: Vec<String>,
    tooltip: RefCell<Option<CachedTooltip>>,
}

/// The tooltip of the last hovered stack; it is rebuilt only when the stack or the flags change.
struct CachedTooltip {
    stack: ItemStack,
    flags: TooltipFlags,
    lines: TooltipLines,
}

/// Registry and map lookups for tooltips over the client state.
pub struct TooltipCx<'a>(pub &'a ClientState);

impl TooltipSource for TooltipCx<'_> {
    fn entry(&self, registry: &str, id: i32) -> Option<RegistryRef<'_>> {
        let e = self.0.registries.get(registry)?.get(id)?;
        Some(RegistryRef {
            id,
            name: e.id.as_str(),
            data: e.data.as_ref(),
        })
    }

    fn entry_named(&self, registry: &str, name: &str) -> Option<RegistryRef<'_>> {
        let r = self.0.registries.get(registry)?;
        let id = r.id_of(name)?;
        let e = r.get(i32::try_from(id).ok()?)?;
        Some(RegistryRef {
            id: id as i32,
            name: e.id.as_str(),
            data: e.data.as_ref(),
        })
    }

    fn tag(&self, registry: &str, tag: &str) -> Option<&[i32]> {
        self.0.registries.tag(registry, tag)
    }

    fn map_info(&self, id: i32) -> Option<MapInfo> {
        let map = self.0.gameplay.maps.get(&id)?;
        Some(MapInfo {
            scale: map.scale,
            locked: map.locked,
        })
    }

    fn tick_rate(&self) -> f32 {
        self.0.world.tick_rate
    }
}

impl ViewCache {
    pub fn refresh(&mut self, state: &ClientState) {
        self.refresh_trim_materials(state);
        let Some(open) = state.inventory.open.as_ref() else {
            self.merchant_key = None;
            self.merchant = None;
            self.stonecutter_key = None;
            self.stonecutter.clear();
            return;
        };
        let key = (state.gameplay.merchant_revision, open.id);
        if self.merchant_key != Some(key) {
            self.merchant_key = Some(key);
            self.merchant = state
                .gameplay
                .merchant
                .as_ref()
                .filter(|m| m.container_id == open.id)
                .map(|m| merchant_info(m, state.data));
        }
        if let (Some(info), Some(m)) = (self.merchant.as_mut(), state.gameplay.merchant.as_ref()) {
            info.future_xp = future_xp(m, open.slots.get(2));
        }
        match state.stonecutter_input() {
            Some(input) => {
                let key = (open.id, input, state.registries.recipes_revision);
                if self.stonecutter_key != Some(key) {
                    self.stonecutter_key = Some(key);
                    self.stonecutter = state.stonecutter_options(input);
                }
            }
            None => {
                self.stonecutter_key = None;
                self.stonecutter.clear();
            }
        }
    }

    fn refresh_trim_materials(&mut self, state: &ClientState) {
        let Some(registry) = state.registries.get("minecraft:trim_material") else {
            return;
        };
        if registry.len() != self.trim_materials.len() {
            self.trim_materials = registry
                .entries
                .iter()
                .map(|e| e.id.as_str().to_owned())
                .collect();
        }
    }
}

fn offer_stack(item: i32, count: i32, components: &[DataComponent]) -> ItemStack {
    let mut stack = ItemStack::new(item, count);
    stack.components.added = components.to_vec();
    stack
}

/// `MerchantOffer#getModifiedCostCount`: demand and the special price move the first cost.
fn modified_cost_count(offer: &MerchantOffer, max_stack: i32) -> i32 {
    let base = offer.cost_a.count;
    let demand =
        ((base as f32 * offer.demand as f32 * offer.price_multiplier).floor() as i32).max(0);
    (base + demand + offer.special_price_diff).clamp(1, max_stack.max(1))
}

fn merchant_info(m: &Merchant, data: &mcr_data::Version) -> MerchantInfo {
    let offers = m
        .offers
        .iter()
        .take(MAX_TRADE_OFFERS)
        .map(|o| {
            let max_stack = u32::try_from(o.cost_a.item)
                .ok()
                .and_then(|i| data.item_info(i))
                .map_or(64, |i| i32::from(i.max_stack_size));
            let a = &o.cost_a;
            TradeOffer {
                base_cost_a: offer_stack(a.item, a.count, &a.components),
                cost_a: offer_stack(a.item, modified_cost_count(o, max_stack), &a.components),
                cost_b: o
                    .cost_b
                    .as_ref()
                    .map(|b| offer_stack(b.item, b.count, &b.components))
                    .unwrap_or_default(),
                result: o.result.clone(),
                out_of_stock: o.out_of_stock,
            }
        })
        .collect();
    MerchantInfo {
        offers,
        level: m.villager_level,
        xp: m.villager_xp,
        future_xp: 0,
        show_progress: m.show_progress,
        can_restock: m.can_restock,
    }
}

fn future_xp(m: &Merchant, result: Option<&ItemStack>) -> i32 {
    let Some(result) = result.filter(|r| !r.is_empty()) else {
        return 0;
    };
    m.offers
        .iter()
        .take(MAX_TRADE_OFFERS)
        .find(|o| o.result.item == result.item && !o.out_of_stock)
        .map_or(0, |o| o.xp)
}

impl StateView<'_> {
    fn container(&self) -> &mcr_client::Container {
        self.state
            .inventory
            .open
            .as_ref()
            .unwrap_or(&self.state.inventory.player)
    }
}

impl GameView for StateView<'_> {
    fn slot(&self, index: usize) -> Option<&ItemStack> {
        self.container().slots.get(index)
    }

    fn cooldown(&self, stack: &ItemStack) -> f32 {
        self.state.cooldown_percent(stack, 0.0)
    }

    fn carried(&self) -> Option<&ItemStack> {
        Some(&self.state.inventory.cursor)
    }

    fn offhand(&self) -> Option<&ItemStack> {
        self.state
            .inventory
            .player
            .slots
            .get(mcr_client::OFFHAND_MENU_SLOT)
    }

    fn data(&self, index: usize) -> i32 {
        i16::try_from(index)
            .ok()
            .and_then(|i| self.container().data.get(&i))
            .map_or(0, |&v| i32::from(v))
    }

    fn creative(&self) -> bool {
        self.state.player.instabuild
    }

    fn book_pages(&self) -> Vec<mcr_protocol::types::text::Component> {
        self.container()
            .slots
            .first()
            .and_then(crate::editors::pages_of_stack)
            .map(|(_, pages)| pages)
            .unwrap_or_default()
    }

    fn item_stack(&self, id: &str, count: i32) -> Option<ItemStack> {
        let name = format!("minecraft:{}", id.strip_prefix("minecraft:").unwrap_or(id));
        let item = mcr_data::v26_3().item_by_name(&name)?;
        Some(ItemStack::new(i32::try_from(item).ok()?, count))
    }

    fn spectator(&self) -> bool {
        self.state.player.game_mode == GameType::Spectator
    }

    fn screen_keys(&self) -> mcr_ui::ScreenKeys {
        self.keys
    }

    fn may_build(&self) -> bool {
        matches!(
            self.state.player.game_mode,
            GameType::Survival | GameType::Creative
        )
    }

    fn sleeping(&self) -> bool {
        self.state.player.is_sleeping()
    }

    fn merchant(&self) -> Option<&MerchantInfo> {
        self.cache.merchant.as_ref()
    }

    fn stonecutter_options(&self) -> &[ItemStack] {
        &self.cache.stonecutter
    }

    fn loom_pattern_count(&self) -> usize {
        self.state.loom_pattern_count()
    }

    fn trim_materials(&self) -> &[String] {
        &self.cache.trim_materials
    }

    fn active_effects(&self) -> Vec<mcr_ui::ActiveEffect> {
        let Some(registry) = self.state.data.registry("minecraft:mob_effect") else {
            return Vec::new();
        };
        self.state
            .player
            .effects
            .iter()
            .filter_map(|(&id, e)| {
                Some(mcr_ui::ActiveEffect {
                    effect: registry.name_of(u32::try_from(id).ok()?)?.to_owned(),
                    amplifier: e.amplifier,
                    duration: e.duration_ticks,
                    ambient: e.flags & 1 != 0,
                })
            })
            .collect()
    }

    fn tick_rate(&self) -> f32 {
        self.state.world.tick_rate
    }

    fn map(&self, id: i32) -> Option<mcr_ui::MapView<'_>> {
        let map = self.state.gameplay.maps.get(&id)?;
        Some(mcr_ui::MapView {
            colors: &map.colors,
            scale: map.scale,
            locked: map.locked,
        })
    }

    fn item_tooltip(&self, stack: &ItemStack) -> Option<TooltipLines> {
        let flags = TooltipFlags {
            advanced: self.advanced_tooltips,
            creative: self.state.player.instabuild,
        };
        let mut cache = self.cache.tooltip.borrow_mut();
        if let Some(c) = cache.as_ref()
            && c.flags == flags
            && c.stack == *stack
        {
            return Some(c.lines.clone());
        }
        let lines = mcr_ui::item_tooltip(stack, &TooltipCx(self.state), flags);
        *cache = Some(CachedTooltip {
            stack: stack.clone(),
            flags,
            lines: lines.clone(),
        });
        Some(lines)
    }

    fn score(&self) -> i32 {
        match self.state.player.data.get(&PLAYER_SCORE_DATA_INDEX) {
            Some(EntityDataValue::Int(v)) => *v,
            _ => 0,
        }
    }

    fn enchant_clue(&self, option: usize) -> Option<(String, i32, i32)> {
        if option >= 3 {
            return None;
        }
        let id = self.data(4 + option);
        let level = self.data(7 + option);
        let entry = self
            .state
            .registries
            .get("minecraft:enchantment")?
            .get(id)?;
        let (namespace, path) = entry.id.as_str().split_once(':')?;
        let max_level = entry
            .data
            .as_ref()
            .and_then(|d| d.get_i32("max_level"))
            .unwrap_or(1);
        Some((
            format!("enchantment.{namespace}.{}", path.replace('/', ".")),
            level,
            max_level,
        ))
    }

    fn xp_level(&self) -> i32 {
        self.state.player.xp_level
    }

    fn enabled_features(&self) -> &[String] {
        &self.state.registries.enabled_features
    }

    fn operator_tab(&self) -> bool {
        self.operator_items && self.state.player.can_use_game_master_blocks()
    }

    fn recipe_book_revision(&self) -> u64 {
        self.state.gameplay.recipe_book.revision
    }

    fn recipe_entries(&self) -> Vec<&RecipeBookEntry> {
        self.state.gameplay.recipe_book.entries.values().collect()
    }

    fn recipe_settings(&self) -> RecipeBookSettings {
        self.state.gameplay.recipe_book.settings
    }

    fn resolve_display(&self, display: &SlotDisplay) -> Vec<ItemStack> {
        mcr_client::resolve_slot_display(display, &self.state.registries, self.state.data)
    }

    fn holder_items(&self, set: &HolderSet) -> Vec<i32> {
        mcr_client::holder_items(set, &self.state.registries)
    }

    fn ghost_recipe(&self) -> Option<(u64, &RecipeDisplay)> {
        let id = self.container().id;
        self.state
            .gameplay
            .ghost_recipe
            .as_ref()
            .filter(|g| g.container_id == id)
            .map(|g| (g.revision, &g.display))
    }

    fn advancements_revision(&self) -> u64 {
        self.state.gameplay.advancements.revision
    }

    fn advancement_defs(&self) -> Vec<&PositionedAdvancement> {
        self.state.gameplay.advancements.ordered_defs().collect()
    }

    fn advancement_progress(&self, id: &str) -> Option<&FxHashMap<String, Option<i64>>> {
        self.state.gameplay.advancements.progress.get(id)
    }

    fn advancement_selected_tab(&self) -> Option<&str> {
        self.state.gameplay.advancements.selected_tab.as_deref()
    }

    fn stats_revision(&self) -> u64 {
        self.state.gameplay.stats_revision
    }

    fn stat(&self, stat_type: i32, id: i32) -> i32 {
        self.state
            .gameplay
            .stats
            .get(&(stat_type, id))
            .copied()
            .unwrap_or(0)
    }

    fn item_tags_matching(&self, query: &str) -> Vec<(String, Vec<i32>)> {
        let matches = |tag: &str| match query.split_once(':') {
            None => tag.split_once(':').map_or(tag, |t| t.1).contains(query),
            Some((ns, path)) => {
                let (tag_ns, tag_path) = tag.split_once(':').unwrap_or(("minecraft", tag));
                tag_ns.contains(ns.trim()) && tag_path.contains(path.trim())
            }
        };
        let registries = &self.state.registries;
        registries
            .tag_names("minecraft:item")
            .filter(|t| matches(t))
            .filter_map(|t| Some((t.to_owned(), registries.tag("minecraft:item", t)?.to_vec())))
            .collect()
    }
}

struct Source<'a>(&'a ClientState);

impl SuggestSource for Source<'_> {
    fn players(&self) -> Vec<String> {
        self.0.players.iter().map(|p| p.name.clone()).collect()
    }

    fn registry(&self, id: &str) -> Vec<String> {
        const LIMIT: usize = 4096;
        if let Some(r) = self.0.registries.get(id) {
            return r
                .entries
                .iter()
                .take(LIMIT)
                .map(|e| e.id.as_str().to_owned())
                .collect();
        }
        let Some(r) = self.0.data.registry(id) else {
            return Vec::new();
        };
        (0..r.len().min(LIMIT) as u32)
            .filter_map(|i| r.name_of(i))
            .map(str::to_owned)
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct Bridge {
    request: Option<(String, usize)>,
    result: Option<SuggestionSet>,
}

/// The chat screen's side of the suggestion pipeline; [`SuggestEngine`] answers from the client.
pub struct BridgeSuggester(pub Rc<RefCell<Bridge>>);

impl CommandSuggester for BridgeSuggester {
    fn request(&mut self, text: &str, cursor: usize) {
        self.0.borrow_mut().request = Some((text.to_owned(), cursor));
    }

    fn poll(&mut self) -> Option<SuggestionSet> {
        self.0.borrow_mut().result.take()
    }
}

struct Waiting {
    id: i32,
    start: usize,
    cursor: usize,
    local: Vec<String>,
}

#[derive(Default)]
pub struct SuggestEngine {
    pub bridge: Rc<RefCell<Bridge>>,
    next_id: i32,
    waiting: Option<Waiting>,
}

impl SuggestEngine {
    pub fn suggester(&self) -> Box<dyn CommandSuggester> {
        Box::new(BridgeSuggester(self.bridge.clone()))
    }

    pub fn pump(&mut self, client: &mut Client) {
        if let Some(w) = &self.waiting
            && let Some(reply) = client.state.command_reply.as_ref().filter(|r| r.id == w.id)
        {
            let mut suggestions: Vec<Suggestion> = reply
                .items
                .iter()
                .map(|(text, tooltip)| Suggestion {
                    start: reply.start,
                    end: reply.start + reply.length,
                    text: text.clone(),
                    tooltip: tooltip.clone(),
                })
                .collect();
            for text in &w.local {
                if !suggestions.iter().any(|s| s.text == *text) {
                    suggestions.push(Suggestion {
                        start: w.start,
                        end: w.cursor,
                        text: text.clone(),
                        tooltip: None,
                    });
                }
            }
            self.bridge.borrow_mut().result = Some(SuggestionSet {
                suggestions,
                usage: Vec::new(),
            });
            self.waiting = None;
        }
        let Some((text, cursor)) = self.bridge.borrow_mut().request.take() else {
            return;
        };
        self.waiting = None;
        if !text.starts_with('/') {
            let (start, matches) =
                mcr_client::complete_chat(&client.state.chat_completions, &text, cursor);
            let suggestions = matches
                .into_iter()
                .map(|text| Suggestion {
                    start,
                    end: cursor,
                    text,
                    tooltip: None,
                })
                .collect();
            self.bridge.borrow_mut().result = Some(SuggestionSet {
                suggestions,
                usage: Vec::new(),
            });
            return;
        }
        let completion = client
            .state
            .commands
            .complete(&text, cursor, &Source(&client.state));
        if completion.ask_server {
            self.next_id = self.next_id.wrapping_add(1);
            let sent: String = text.chars().take(cursor).collect();
            if client
                .request_command_suggestions(self.next_id, &sent)
                .is_ok()
            {
                self.waiting = Some(Waiting {
                    id: self.next_id,
                    start: completion.start,
                    cursor,
                    local: completion.local,
                });
                return;
            }
        }
        let suggestions: Vec<Suggestion> = completion
            .local
            .into_iter()
            .map(|text| Suggestion {
                start: completion.start,
                end: cursor,
                text,
                tooltip: None,
            })
            .collect();
        let usage = if suggestions.is_empty() {
            completion.usage
        } else {
            Vec::new()
        };
        self.bridge.borrow_mut().result = Some(SuggestionSet { suggestions, usage });
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::play::clientbound::ui::{ItemCost, MerchantOffers};
    use mcr_protocol::types::text::Component;
    use uuid::Uuid;

    use super::*;

    fn item(name: &str) -> i32 {
        mcr_data::v26_3().item_by_name(name).unwrap() as i32
    }

    fn offer(demand: i32, special: i32) -> MerchantOffer {
        MerchantOffer {
            cost_a: ItemCost {
                item: item("minecraft:emerald"),
                count: 10,
                components: Vec::new(),
            },
            result: ItemStack::new(item("minecraft:bread"), 4),
            cost_b: None,
            out_of_stock: false,
            uses: 0,
            max_uses: 12,
            xp: 5,
            special_price_diff: special,
            price_multiplier: 0.05,
            demand,
        }
    }

    fn trading_state() -> ClientState {
        let mut s = ClientState::new(&mcr_client::ClientConfig::default(), Uuid::nil());
        s.inventory.open_container(7, 19, Component::text(""));
        s.inventory
            .set_content(7, 0, vec![ItemStack::default(); 39], ItemStack::default());
        s.gameplay.merchant = Some(
            MerchantOffers {
                container_id: 7,
                offers: vec![offer(20, -3), offer(0, 0)],
                villager_level: 2,
                villager_xp: 12,
                show_progress: true,
                can_restock: true,
            }
            .into(),
        );
        s.gameplay.merchant_revision += 1;
        s
    }

    fn view<'a>(state: &'a ClientState, cache: &'a ViewCache) -> StateView<'a> {
        StateView {
            state,
            operator_items: false,
            advanced_tooltips: false,
            keys: mcr_ui::ScreenKeys::default(),
            cache,
        }
    }

    #[test]
    fn state_view_returns_the_merchant_offers_with_adjusted_prices() {
        let state = trading_state();
        let mut cache = ViewCache::default();
        cache.refresh(&state);
        let view = view(&state, &cache);
        let info = view.merchant().unwrap();
        assert_eq!((info.level, info.xp), (2, 12));
        assert_eq!(info.offers.len(), 2);
        assert_eq!(info.offers[0].base_cost_a.count, 10);
        assert_eq!(info.offers[0].cost_a.count, 10 + 10 - 3);
        assert_eq!(info.offers[1].cost_a.count, 10);
    }

    #[test]
    fn merchant_offers_of_another_container_are_not_shown() {
        let mut state = trading_state();
        state.inventory.open.as_mut().unwrap().id = 8;
        let mut cache = ViewCache::default();
        cache.refresh(&state);
        assert!(view(&state, &cache).merchant().is_none());
    }

    #[test]
    fn future_xp_follows_the_result_slot() {
        let mut state = trading_state();
        let mut cache = ViewCache::default();
        cache.refresh(&state);
        assert_eq!(cache.merchant.as_ref().unwrap().future_xp, 0);
        state.inventory.open.as_mut().unwrap().slots[2] =
            ItemStack::new(item("minecraft:bread"), 4);
        cache.refresh(&state);
        assert_eq!(cache.merchant.as_ref().unwrap().future_xp, 5);
    }

    #[test]
    fn state_view_reports_spectators_and_the_synced_score() {
        let mut state = trading_state();
        let cache = ViewCache::default();
        assert!(!view(&state, &cache).spectator());
        state.player.game_mode = GameType::Spectator;
        state
            .player
            .data
            .insert(PLAYER_SCORE_DATA_INDEX, EntityDataValue::Int(42));
        let v = view(&state, &cache);
        assert!(v.spectator());
        assert_eq!(v.score(), 42);
    }
}
