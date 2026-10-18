//! What container screens read from the open menu beyond its slots.

use mcr_protocol::types::ItemStack;

use crate::menu::Rules;
use crate::state::ClientState;

const MAX_STONECUTTER_OPTIONS: usize = 1024;

impl ClientState {
    fn open_menu_name(&self) -> Option<&str> {
        let id = u32::try_from(self.inventory.open.as_ref()?.menu_type?).ok()?;
        let name = self.data.registry("minecraft:menu")?.name_of(id)?;
        Some(name.strip_prefix("minecraft:").unwrap_or(name))
    }

    fn open_slot(&self, index: usize) -> Option<&ItemStack> {
        self.inventory.open.as_ref()?.slots.get(index)
    }

    /// Selectable banner patterns of the open loom: none until it holds a banner and a dye.
    pub fn loom_pattern_count(&self) -> usize {
        if self.open_menu_name() != Some("loom") {
            return 0;
        }
        let filled = |i| self.open_slot(i).is_some_and(|s| !s.is_empty());
        if !filled(0) || !filled(1) {
            return 0;
        }
        let rules = Rules {
            data: self.data,
            registries: &self.registries,
            creative: false,
        };
        let none = ItemStack::default();
        rules.loom_pattern_count(self.open_slot(2).unwrap_or(&none))
    }

    /// Item the open stonecutter's recipes are looked up for.
    pub fn stonecutter_input(&self) -> Option<i32> {
        if self.open_menu_name() != Some("stonecutter") {
            return None;
        }
        self.open_slot(0).filter(|s| !s.is_empty()).map(|s| s.item)
    }

    /// One stack per synced stonecutter recipe taking `input`, in the server's list order.
    pub fn stonecutter_options(&self, input: i32) -> Vec<ItemStack> {
        self.registries
            .stonecutter_matches(input)
            .take(MAX_STONECUTTER_OPTIONS)
            .map(|display| {
                crate::resolve_slot_display(display, &self.registries, self.data)
                    .into_iter()
                    .next()
                    .unwrap_or_default()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{Identifier, VarInt};
    use mcr_protocol::common::UpdateTags;
    use mcr_protocol::play::clientbound::ui::{
        Pairs, SlotDisplay, StonecutterRecipe, UpdateRecipes,
    };
    use mcr_protocol::types::component::HolderSet;
    use mcr_protocol::types::registry::{IdList, RegistryTags, TagEntries};
    use mcr_protocol::types::text::Component;
    use uuid::Uuid;

    use super::*;
    use crate::config::ClientConfig;

    fn item(name: &str) -> i32 {
        mcr_data::v26_3().item_by_name(name).unwrap() as i32
    }

    fn menu_id(name: &str) -> i32 {
        mcr_data::v26_3()
            .registry("minecraft:menu")
            .unwrap()
            .id_of(name)
            .unwrap() as i32
    }

    fn state_with_menu(name: &str, slots: usize) -> ClientState {
        let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
        s.inventory
            .open_container(1, menu_id(name), Component::text(""));
        s.inventory.set_content(
            1,
            0,
            vec![ItemStack::default(); slots],
            ItemStack::default(),
        );
        s
    }

    fn tags(s: &mut ClientState, registry: &str, entries: &[(&str, Vec<i32>)]) {
        s.registries.apply_tags(&UpdateTags {
            registries: vec![RegistryTags {
                registry: Identifier::parse(registry).unwrap(),
                tags: entries
                    .iter()
                    .map(|(name, ids)| TagEntries {
                        name: Identifier::parse(name).unwrap(),
                        ids: IdList(ids.clone()),
                    })
                    .collect(),
            }],
        });
    }

    #[test]
    fn loom_offers_the_no_item_required_patterns_for_a_banner_and_dye() {
        let mut s = state_with_menu("minecraft:loom", 40);
        tags(
            &mut s,
            "minecraft:banner_pattern",
            &[("minecraft:no_item_required", vec![0, 1, 2, 3, 4])],
        );
        assert_eq!(s.loom_pattern_count(), 0);
        let open = s.inventory.open.as_mut().unwrap();
        open.slots[0] = ItemStack::new(item("minecraft:white_banner"), 1);
        assert_eq!(s.loom_pattern_count(), 0);
        let open = s.inventory.open.as_mut().unwrap();
        open.slots[1] = ItemStack::new(item("minecraft:red_dye"), 1);
        assert_eq!(s.loom_pattern_count(), 5);
    }

    #[test]
    fn loom_pattern_item_lists_its_own_tag() {
        let mut s = state_with_menu("minecraft:loom", 40);
        tags(
            &mut s,
            "minecraft:banner_pattern",
            &[
                ("minecraft:no_item_required", vec![0, 1, 2]),
                ("minecraft:pattern_item/globe", vec![7]),
            ],
        );
        let open = s.inventory.open.as_mut().unwrap();
        open.slots[0] = ItemStack::new(item("minecraft:white_banner"), 1);
        open.slots[1] = ItemStack::new(item("minecraft:red_dye"), 1);
        open.slots[2] = ItemStack::new(item("minecraft:globe_banner_pattern"), 1);
        assert_eq!(s.loom_pattern_count(), 1);
    }

    #[test]
    fn stonecutter_lists_the_displays_of_recipes_taking_the_input() {
        let mut s = state_with_menu("minecraft:stonecutter", 38);
        let stone = item("minecraft:stone");
        let slab = item("minecraft:stone_slab");
        let stairs = item("minecraft:stone_stairs");
        let recipe = |input: i32, result: i32| StonecutterRecipe {
            input: HolderSet::Ids(vec![input]),
            option_display: SlotDisplay::Item(result),
        };
        s.registries.apply_recipes(&UpdateRecipes {
            item_sets: Pairs(vec![(
                Identifier::parse("minecraft:furnace_input").unwrap(),
                vec![VarInt(stone)],
            )]),
            stonecutter_recipes: vec![
                recipe(stone, slab),
                recipe(item("minecraft:granite"), stairs),
                recipe(stone, stairs),
            ],
        });
        assert_eq!(s.stonecutter_input(), None);
        s.inventory.open.as_mut().unwrap().slots[0] = ItemStack::new(stone, 1);
        let input = s.stonecutter_input().unwrap();
        let options: Vec<i32> = s
            .stonecutter_options(input)
            .iter()
            .map(|o| o.item)
            .collect();
        assert_eq!(options, [slab, stairs]);
    }
}
