//! Turns server-sent slot displays into the stacks a screen can draw.

use mcr_data::Version;
use mcr_protocol::play::clientbound::ui::{RecipeDisplay, RecipeDisplayEntry, SlotDisplay};
use mcr_protocol::types::component::HolderSet;
use mcr_protocol::types::item::ItemStack;

use crate::registry::Registries;
use crate::toast::stack_of;

pub const MAX_RESOLVED: usize = 256;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 4096;
const FUEL_ITEMS: [&str; 8] = [
    "coal",
    "charcoal",
    "lava_bucket",
    "blaze_rod",
    "coal_block",
    "dried_kelp_block",
    "bamboo",
    "stick",
];
const FUEL_TAGS: [&str; 2] = ["minecraft:planks", "minecraft:logs_that_burn"];

pub fn holder_items(set: &HolderSet, registries: &Registries) -> Vec<i32> {
    match set {
        HolderSet::Ids(ids) => ids.iter().copied().take(MAX_RESOLVED * 16).collect(),
        HolderSet::Tag(tag) => registries
            .tag("minecraft:item", tag.as_str())
            .map(|ids| ids.iter().copied().take(MAX_RESOLVED * 16).collect())
            .unwrap_or_default(),
    }
}

/// Stored size of a recipe entry in display nodes and ids, saturating at `limit`.
pub fn entry_weight(entry: &RecipeDisplayEntry, limit: usize) -> usize {
    let mut stack: Vec<&SlotDisplay> = match &entry.display {
        RecipeDisplay::CraftingShapeless {
            ingredients,
            result,
            crafting_station,
        }
        | RecipeDisplay::CraftingShaped {
            ingredients,
            result,
            crafting_station,
            ..
        } => ingredients
            .iter()
            .chain([result, crafting_station])
            .collect(),
        RecipeDisplay::Furnace {
            ingredient,
            fuel,
            result,
            crafting_station,
            ..
        } => vec![ingredient, fuel, result, crafting_station],
        RecipeDisplay::Stonecutter {
            input,
            result,
            crafting_station,
        } => vec![input, result, crafting_station],
        RecipeDisplay::Smithing {
            template,
            base,
            addition,
            result,
            crafting_station,
        } => vec![template, base, addition, result, crafting_station],
    };
    let mut weight = entry
        .crafting_requirements
        .iter()
        .flatten()
        .map(holder_weight)
        .fold(0usize, usize::saturating_add);
    while let Some(d) = stack.pop() {
        weight = weight.saturating_add(1);
        if weight >= limit {
            return limit;
        }
        match d {
            SlotDisplay::Tag(set) => weight = weight.saturating_add(holder_weight(set)),
            SlotDisplay::WithAnyPotion(inner) => stack.push(inner),
            SlotDisplay::OnlyWithComponent { source, .. } => stack.push(source),
            SlotDisplay::Dyed { dye, target } => stack.extend([&**dye, &**target]),
            SlotDisplay::SmithingTrim { base, material, .. } => {
                stack.extend([&**base, &**material]);
            }
            SlotDisplay::WithRemainder { input, remainder } => {
                stack.extend([&**input, &**remainder]);
            }
            SlotDisplay::Composite(all) => stack.extend(all),
            SlotDisplay::Empty
            | SlotDisplay::AnyFuel
            | SlotDisplay::Item(_)
            | SlotDisplay::ItemStack(_) => {}
        }
    }
    weight.min(limit)
}

fn holder_weight(set: &HolderSet) -> usize {
    match set {
        HolderSet::Tag(_) => 1,
        HolderSet::Ids(ids) => ids.len(),
    }
}

/// Every stack the display cycles through; potion, dye and trim variants collapse to their base item.
pub fn resolve(display: &SlotDisplay, registries: &Registries, data: &Version) -> Vec<ItemStack> {
    let mut out = Vec::new();
    let mut budget = MAX_NODES;
    walk(display, registries, data, 0, &mut budget, &mut out);
    out
}

fn walk(
    display: &SlotDisplay,
    registries: &Registries,
    data: &Version,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<ItemStack>,
) {
    if depth > MAX_DEPTH || out.len() >= MAX_RESOLVED || *budget == 0 {
        return;
    }
    *budget -= 1;
    let push = |stack: ItemStack, out: &mut Vec<ItemStack>| {
        if out.len() < MAX_RESOLVED {
            out.push(stack);
        }
    };
    match display {
        SlotDisplay::Empty => {}
        SlotDisplay::Item(id) => push(ItemStack::new(*id, 1), out),
        SlotDisplay::ItemStack(template) => push(stack_of(template), out),
        SlotDisplay::Tag(set) => {
            for id in holder_items(set, registries) {
                push(ItemStack::new(id, 1), out);
            }
        }
        SlotDisplay::AnyFuel => {
            for name in FUEL_ITEMS {
                if let Some(id) = data.item_by_name(&format!("minecraft:{name}")) {
                    push(ItemStack::new(id as i32, 1), out);
                }
            }
            for tag in FUEL_TAGS {
                for &id in registries.tag("minecraft:item", tag).unwrap_or_default() {
                    push(ItemStack::new(id, 1), out);
                }
            }
        }
        SlotDisplay::WithAnyPotion(inner) => walk(inner, registries, data, depth + 1, budget, out),
        SlotDisplay::OnlyWithComponent { source, .. } => {
            walk(source, registries, data, depth + 1, budget, out);
        }
        SlotDisplay::Dyed { target, .. } => walk(target, registries, data, depth + 1, budget, out),
        SlotDisplay::SmithingTrim { base, .. } => {
            walk(base, registries, data, depth + 1, budget, out)
        }
        SlotDisplay::WithRemainder { input, .. } => {
            walk(input, registries, data, depth + 1, budget, out)
        }
        SlotDisplay::Composite(all) => {
            for d in all.iter().take(MAX_RESOLVED) {
                walk(d, registries, data, depth + 1, budget, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composites_flatten_and_stay_bounded() {
        let registries = Registries::default();
        let data = mcr_data::v26_3();
        let display = SlotDisplay::Composite(vec![
            SlotDisplay::Item(1),
            SlotDisplay::Empty,
            SlotDisplay::WithRemainder {
                input: Box::new(SlotDisplay::Item(2)),
                remainder: Box::new(SlotDisplay::Item(3)),
            },
        ]);
        let items = resolve(&display, &registries, data);
        assert_eq!(items, vec![ItemStack::new(1, 1), ItemStack::new(2, 1)]);
        let wide = SlotDisplay::Composite(vec![SlotDisplay::Item(1); 5000]);
        assert_eq!(resolve(&wide, &registries, data).len(), MAX_RESOLVED);
    }

    #[test]
    fn wide_and_deep_empty_trees_cost_a_bounded_walk() {
        let mut display = SlotDisplay::Empty;
        for _ in 0..6 {
            display = SlotDisplay::Composite(vec![display; 8]);
        }
        let items = resolve(&display, &Registries::default(), mcr_data::v26_3());
        assert!(items.is_empty());
    }

    #[test]
    fn unknown_tags_resolve_to_nothing() {
        let tag = HolderSet::Tag(mcr_buf::Identifier::parse("minecraft:nope").unwrap());
        let items = resolve(
            &SlotDisplay::Tag(tag),
            &Registries::default(),
            mcr_data::v26_3(),
        );
        assert!(items.is_empty());
    }
}
