//! Adapters between the client state and the UI crate's traits.

use std::cell::RefCell;
use std::rc::Rc;

use mcr_client::{Client, ClientState, SuggestSource};
use mcr_protocol::play::clientbound::ui::{
    PositionedAdvancement, RecipeBookEntry, RecipeBookSettings, RecipeDisplay, SlotDisplay,
};
use mcr_protocol::types::component::HolderSet;
use mcr_protocol::types::item::ItemStack;
use mcr_ui::{CommandSuggester, GameView, Suggestion, SuggestionSet};
use rustc_hash::FxHashMap;

pub struct StateView<'a> {
    pub state: &'a ClientState,
    pub operator_items: bool,
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
            self.bridge.borrow_mut().result = Some(SuggestionSet::default());
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
