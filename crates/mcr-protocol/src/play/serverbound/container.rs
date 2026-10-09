use mcr_buf::{BoundedString, Decode, Encode, LengthPrefixed};

use crate::packet;
use crate::types::{HashedStack, ItemStack};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum ContainerInput {
    Pickup,
    QuickMove,
    Swap,
    Clone,
    Throw,
    QuickCraft,
    PickupAll,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ChangedSlot {
    pub slot: i16,
    pub item: HashedStack,
}

pub const MAX_CHANGED_SLOTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ContainerClick {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub state_id: i32,
    pub slot_num: i16,
    pub button_num: i8,
    pub input: ContainerInput,
    pub changed_slots: LengthPrefixed<ChangedSlot, MAX_CHANGED_SLOTS>,
    pub carried_item: HashedStack,
}

impl ContainerClick {
    pub fn new(
        container_id: i32,
        state_id: i32,
        slot_num: i16,
        button_num: i8,
        input: ContainerInput,
        changed_slots: Vec<ChangedSlot>,
        carried_item: HashedStack,
    ) -> Self {
        Self {
            container_id,
            state_id,
            slot_num,
            button_num,
            input,
            changed_slots: LengthPrefixed(changed_slots),
            carried_item,
        }
    }
}

packet!(ContainerClick, "container_click", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ContainerButtonClick {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub button_id: i32,
}

packet!(ContainerButtonClick, "container_button_click", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ContainerClose {
    #[mcr(varint)]
    pub container_id: i32,
}

packet!(ContainerClose, "container_close", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct ContainerSlotStateChanged {
    #[mcr(varint)]
    pub slot_id: i32,
    #[mcr(varint)]
    pub container_id: i32,
    pub new_state: bool,
}

packet!(
    ContainerSlotStateChanged,
    "container_slot_state_changed",
    Serverbound
);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetCreativeModeSlot {
    pub slot_num: i16,
    pub item: ItemStack,
}

packet!(SetCreativeModeSlot, "set_creative_mode_slot", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SetCarriedItem {
    pub slot: i16,
}

packet!(SetCarriedItem, "set_carried_item", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct PlaceRecipe {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub recipe: i32,
    pub use_max_items: bool,
}

packet!(PlaceRecipe, "place_recipe", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum RecipeBookType {
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct RecipeBookChangeSettings {
    pub book_type: RecipeBookType,
    pub open: bool,
    pub filtering: bool,
}

packet!(
    RecipeBookChangeSettings,
    "recipe_book_change_settings",
    Serverbound
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct RecipeBookSeenRecipe {
    #[mcr(varint)]
    pub recipe: i32,
}

packet!(RecipeBookSeenRecipe, "recipe_book_seen_recipe", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct RenameItem {
    pub name: String,
}

packet!(RenameItem, "rename_item", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct SelectTrade {
    #[mcr(varint)]
    pub item: i32,
}

packet!(SelectTrade, "select_trade", Serverbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct BundleItemSelected {
    #[mcr(varint)]
    pub slot_id: i32,
    #[mcr(varint)]
    pub selected_item_index: i32,
}

packet!(BundleItemSelected, "bundle_item_selected", Serverbound);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EditBook {
    #[mcr(varint)]
    pub slot: i32,
    pub pages: LengthPrefixed<BoundedString<1024>, 100>,
    pub title: Option<BoundedString<32>>,
}

packet!(EditBook, "edit_book", Serverbound);

#[cfg(test)]
mod tests {
    use mcr_buf::{DecodeError, to_bytes};

    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;

    roundtrips! {
        button: ContainerButtonClick { container_id: 2, button_id: 300 },
        close: ContainerClose { container_id: 2 },
        slot_state: ContainerSlotStateChanged { slot_id: 4, container_id: 1, new_state: true },
        carried: SetCarriedItem { slot: 8 },
        place: PlaceRecipe { container_id: 1, recipe: 77, use_max_items: true },
        settings: RecipeBookChangeSettings { book_type: RecipeBookType::Smoker, open: true, filtering: false },
        seen: RecipeBookSeenRecipe { recipe: 5 },
        rename: RenameItem { name: "Sword".into() },
        trade: SelectTrade { item: 2 },
        bundle: BundleItemSelected { slot_id: 9, selected_item_index: -1 },
        book: EditBook {
            slot: 0,
            pages: LengthPrefixed(vec![BoundedString("page".into())]),
            title: Some(BoundedString("t".into())),
        },
    }

    #[test]
    #[ignore = "needs HashedStack"]
    fn click_roundtrip() {
        roundtrip(&ContainerClick::new(
            1,
            2,
            36,
            0,
            ContainerInput::QuickMove,
            vec![ChangedSlot {
                slot: 3,
                item: HashedStack::default(),
            }],
            HashedStack::default(),
        ));
    }

    #[test]
    #[ignore = "needs ItemStack"]
    fn creative_slot_roundtrip() {
        roundtrip(&SetCreativeModeSlot {
            slot_num: 36,
            item: ItemStack::default(),
        });
    }

    #[test]
    fn click_slot_limit() {
        let bytes = [1, 0, 0, 36, 0, 0, 0x81, 0x01];
        assert_eq!(
            ContainerClick::decode(&mut &bytes[..]),
            Err(DecodeError::TooLong { len: 129, max: 128 })
        );
    }

    #[test]
    fn small_packet_bytes() {
        assert_eq!(to_bytes(&ContainerInput::Swap), [2]);
        assert_eq!(
            to_bytes(&ContainerButtonClick {
                container_id: 1,
                button_id: 300
            }),
            [1, 0xac, 0x02]
        );
        assert_eq!(to_bytes(&SetCarriedItem { slot: 8 }), [0, 8]);
    }
}
