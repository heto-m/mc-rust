//! What the server tells the player worth a toast: finished advancements and unlocked recipes.

use mcr_protocol::play::clientbound::ui::{
    AdvancementFrame, RecipeBookEntry, RecipeDisplay, SlotDisplay,
};
use mcr_protocol::types::Component;
use mcr_protocol::types::item::{ItemStack, ItemStackTemplate};

pub const MAX_TOASTS_PER_PACKET: usize = 8;
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum ToastEvent {
    Advancement {
        title: Component,
        frame: AdvancementFrame,
        icon: ItemStack,
    },
    Recipe {
        station: ItemStack,
        result: ItemStack,
    },
    /// The server ran low on disk space and may fail to save the world.
    LowDiskSpace,
}

pub fn stack_of(template: &ItemStackTemplate) -> ItemStack {
    ItemStack {
        item: template.item,
        count: template.count,
        components: template.components.clone(),
    }
}

/// The first concrete stack a slot display shows; tag and fuel displays need registries and stay empty.
pub fn first_stack(display: &SlotDisplay) -> ItemStack {
    fn walk(display: &SlotDisplay, depth: usize) -> Option<ItemStack> {
        if depth > MAX_DEPTH {
            return None;
        }
        match display {
            SlotDisplay::Item(id) => Some(ItemStack::new(*id, 1)),
            SlotDisplay::ItemStack(template) => Some(stack_of(template)),
            SlotDisplay::WithAnyPotion(inner) => walk(inner, depth + 1),
            SlotDisplay::OnlyWithComponent { source, .. } => walk(source, depth + 1),
            SlotDisplay::Dyed { target, .. } => walk(target, depth + 1),
            SlotDisplay::SmithingTrim { base, .. } => walk(base, depth + 1),
            SlotDisplay::WithRemainder { input, .. } => walk(input, depth + 1),
            SlotDisplay::Composite(all) => all.iter().find_map(|d| walk(d, depth + 1)),
            SlotDisplay::Empty | SlotDisplay::AnyFuel | SlotDisplay::Tag(_) => None,
        }
    }
    walk(display, 0).unwrap_or_default()
}

pub fn recipe_toast(entry: &RecipeBookEntry) -> Option<ToastEvent> {
    if !entry.notification() {
        return None;
    }
    let (result, station) = match &entry.contents.display {
        RecipeDisplay::CraftingShapeless {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::CraftingShaped {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Furnace {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Stonecutter {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Smithing {
            result,
            crafting_station,
            ..
        } => (result, crafting_station),
    };
    Some(ToastEvent::Recipe {
        station: first_stack(station),
        result: first_stack(result),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_takes_the_first_concrete_stack() {
        let display = SlotDisplay::Composite(vec![
            SlotDisplay::Empty,
            SlotDisplay::WithRemainder {
                input: Box::new(SlotDisplay::Item(7)),
                remainder: Box::new(SlotDisplay::Item(9)),
            },
        ]);
        assert_eq!(first_stack(&display), ItemStack::new(7, 1));
        assert!(first_stack(&SlotDisplay::AnyFuel).is_empty());
    }

    #[test]
    fn deep_nesting_is_cut_off() {
        let mut display = SlotDisplay::Item(1);
        for _ in 0..64 {
            display = SlotDisplay::WithAnyPotion(Box::new(display));
        }
        assert!(first_stack(&display).is_empty());
    }
}
