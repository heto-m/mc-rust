//! Screen-driven serverbound actions: signs, books, anvil rename, trades, beacon, menu buttons, recipes.

use mcr_buf::{BoundedString, LengthPrefixed, VarInt};
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::game::Hand;

use crate::client::Client;
use crate::error::Result;
use crate::inventory::PLAYER_MENU_ID;

pub const SIGN_LINE_MAX: usize = 384;
pub const BOOK_PAGE_MAX: usize = 1024;
pub const BOOK_PAGES_MAX: usize = 100;
pub const BOOK_TITLE_MAX: usize = 32;
pub const ITEM_NAME_MAX: usize = 50;

fn clamp_utf16(s: &str, max: usize) -> String {
    let mut units = 0;
    s.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max
        })
        .collect()
}

impl Client {
    fn open_container_id(&self) -> i32 {
        self.state
            .inventory
            .open
            .as_ref()
            .map_or(PLAYER_MENU_ID, |c| c.id)
    }

    /// Answers `open_sign_editor`; the editor state is cleared.
    pub fn submit_sign(&mut self, lines: [&str; 4]) -> Result<()> {
        let Some(editor) = self.state.gameplay.sign_editor.take() else {
            return Ok(());
        };
        self.send_play(&S::SignUpdate(sb::SignUpdate {
            pos: editor.pos,
            lines: lines.map(|l| BoundedString(clamp_utf16(l, SIGN_LINE_MAX))),
            side: editor.side,
        }))
    }

    pub fn cancel_sign_editor(&mut self) {
        self.state.gameplay.sign_editor = None;
    }

    /// Saves a writable book; `title` signs it. Slot is the held hotbar slot or 40 for the off hand.
    pub fn submit_book(&mut self, pages: &[String], title: Option<&str>) -> Result<()> {
        let Some(hand) = self.state.gameplay.book_editor.take() else {
            return Ok(());
        };
        let slot = match hand {
            Hand::Main => i32::from(self.state.player.held_slot),
            Hand::Off => 40,
        };
        self.send_play(&S::EditBook(sb::EditBook {
            slot,
            pages: LengthPrefixed(
                pages
                    .iter()
                    .take(BOOK_PAGES_MAX)
                    .map(|p| BoundedString(clamp_utf16(p, BOOK_PAGE_MAX)))
                    .collect(),
            ),
            title: title.map(|t| BoundedString(clamp_utf16(t, BOOK_TITLE_MAX))),
        }))
    }

    pub fn cancel_book_editor(&mut self) {
        self.state.gameplay.book_editor = None;
    }

    pub fn rename_item(&mut self, name: &str) -> Result<()> {
        self.send_play(&S::RenameItem(sb::RenameItem {
            name: clamp_utf16(name, ITEM_NAME_MAX),
        }))
    }

    pub fn select_trade(&mut self, index: i32) -> Result<()> {
        let count = self
            .state
            .gameplay
            .merchant
            .as_ref()
            .map_or(0, |m| m.offers.len());
        if usize::try_from(index).is_ok_and(|i| i < count) {
            self.send_play(&S::SelectTrade(sb::SelectTrade { item: index }))?;
        }
        Ok(())
    }

    pub fn select_bundle_item(&mut self, slot: i32, index: i32) -> Result<()> {
        self.send_play(&S::BundleItemSelected(sb::BundleItemSelected {
            slot_id: slot,
            selected_item_index: index.max(-1),
        }))
    }

    /// Effect ids are `mob_effect` registry ids.
    pub fn set_beacon(&mut self, primary: Option<i32>, secondary: Option<i32>) -> Result<()> {
        self.send_play(&S::SetBeacon(sb::SetBeacon {
            primary: primary.map(VarInt),
            secondary: secondary.map(VarInt),
        }))
    }

    pub fn request_stats(&mut self) -> Result<()> {
        self.send_play(&S::ClientCommand(sb::ClientCommand {
            action: sb::ClientCommandAction::RequestStats,
        }))
    }

    pub fn seen_advancements(&mut self, tab: Option<&str>) -> Result<()> {
        let packet = match tab.and_then(|t| mcr_buf::Identifier::parse(t).ok()) {
            Some(id) => sb::SeenAdvancements::OpenedTab(id),
            None => sb::SeenAdvancements::ClosedScreen,
        };
        self.send_play(&S::SeenAdvancements(packet))
    }

    pub fn place_recipe(&mut self, recipe: i32, use_max_items: bool) -> Result<()> {
        let container_id = self.open_container_id();
        self.send_play(&S::PlaceRecipe(sb::PlaceRecipe {
            container_id,
            recipe,
            use_max_items,
        }))
    }

    pub fn mark_recipe_seen(&mut self, recipe: i32) -> Result<()> {
        if let Some(e) = self.state.gameplay.recipe_book.entries.get_mut(&recipe) {
            e.flags = 0;
        }
        self.send_play(&S::RecipeBookSeenRecipe(sb::RecipeBookSeenRecipe {
            recipe,
        }))
    }

    pub fn set_recipe_book_settings(
        &mut self,
        book_type: sb::RecipeBookType,
        open: bool,
        filtering: bool,
    ) -> Result<()> {
        let book = &mut self.state.gameplay.recipe_book;
        let slot = match book_type {
            sb::RecipeBookType::Crafting => &mut book.settings.crafting,
            sb::RecipeBookType::Furnace => &mut book.settings.furnace,
            sb::RecipeBookType::BlastFurnace => &mut book.settings.blast_furnace,
            sb::RecipeBookType::Smoker => &mut book.settings.smoker,
        };
        slot.open = open;
        slot.filtering = filtering;
        book.revision += 1;
        self.send_play(&S::RecipeBookChangeSettings(sb::RecipeBookChangeSettings {
            book_type,
            open,
            filtering,
        }))
    }
}
