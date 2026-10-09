use mcr_buf::{BoundedString, Decode, DecodeError, Encode, LengthPrefixed, Result, read_varint};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode)]
pub struct BundleItemSelected {
    #[mcr(varint)]
    pub slot_id: i32,
    #[mcr(varint)]
    pub selected_item_index: i32,
}

impl Decode<'_> for BundleItemSelected {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let slot_id = read_varint(input)?;
        let selected_item_index = read_varint(input)?;
        if selected_item_index < -1 {
            return Err(DecodeError::InvalidValue {
                what: "selected bundle item index",
                value: selected_item_index.into(),
            });
        }
        Ok(Self {
            slot_id,
            selected_item_index,
        })
    }
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
    use mcr_buf::{VarInt, to_bytes};

    use super::*;
    use crate::play::serverbound::shared::roundtrips;
    use crate::test_util::roundtrip;
    use crate::types::item::{HashedComponent, HashedItem, HashedPatchMap};

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
    fn click_bytes() {
        let changed = ChangedSlot {
            slot: 3,
            item: HashedStack::Item(HashedItem {
                item: 5,
                count: 64,
                components: HashedPatchMap {
                    added: LengthPrefixed(vec![HashedComponent {
                        type_id: 2,
                        hash: 0x0102_0304,
                    }]),
                    removed: LengthPrefixed(vec![VarInt(7)]),
                },
            }),
        };
        let packet = ContainerClick::new(
            1,
            300,
            36,
            1,
            ContainerInput::QuickMove,
            vec![changed],
            HashedStack::Empty,
        );
        assert_eq!(
            roundtrip(&packet),
            [
                1, 0xac, 0x02, 0, 36, 1, 1, 1, 0, 3, 1, 5, 64, 1, 2, 1, 2, 3, 4, 1, 7, 0
            ]
        );
    }

    #[test]
    fn creative_slot_bytes() {
        assert_eq!(
            roundtrip(&SetCreativeModeSlot {
                slot_num: 36,
                item: ItemStack::default(),
            }),
            [0, 36, 0]
        );
    }

    #[test]
    fn edit_book_bytes_and_limits() {
        let packet = EditBook {
            slot: 2,
            pages: LengthPrefixed(vec![BoundedString("ab".into()), BoundedString::default()]),
            title: Some(BoundedString("t".into())),
        };
        assert_eq!(roundtrip(&packet), [2, 2, 2, b'a', b'b', 0, 1, 1, b't']);

        let mut too_many = vec![0, 101];
        too_many.resize(2 + 101, 0);
        too_many.push(0);
        assert_eq!(
            EditBook::decode(&mut too_many.as_slice()),
            Err(DecodeError::TooLong { len: 101, max: 100 })
        );
        let long_title = to_bytes(&EditBook {
            slot: 0,
            pages: LengthPrefixed::default(),
            title: Some(BoundedString("a".repeat(33))),
        });
        assert!(EditBook::decode(&mut long_title.as_slice()).is_err());
    }

    #[test]
    fn bundle_index_validated() {
        assert_eq!(
            roundtrip(&BundleItemSelected {
                slot_id: 9,
                selected_item_index: -1
            }),
            [9, 0xff, 0xff, 0xff, 0xff, 0x0f]
        );
        assert!(matches!(
            BundleItemSelected::decode(&mut &[9, 0xfe, 0xff, 0xff, 0xff, 0x0f][..]),
            Err(DecodeError::InvalidValue { value: -2, .. })
        ));
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
        assert_eq!(to_bytes(&ContainerClose { container_id: 2 }), [2]);
        assert_eq!(
            to_bytes(&ContainerSlotStateChanged {
                slot_id: 4,
                container_id: 1,
                new_state: true
            }),
            [4, 1, 1]
        );
        assert_eq!(
            to_bytes(&PlaceRecipe {
                container_id: 1,
                recipe: 300,
                use_max_items: true
            }),
            [1, 0xac, 0x02, 1]
        );
        assert_eq!(
            to_bytes(&RecipeBookChangeSettings {
                book_type: RecipeBookType::Smoker,
                open: true,
                filtering: false
            }),
            [3, 1, 0]
        );
        assert_eq!(to_bytes(&RecipeBookSeenRecipe { recipe: 5 }), [5]);
        assert_eq!(to_bytes(&RenameItem { name: "S".into() }), [1, b'S']);
        assert_eq!(to_bytes(&SelectTrade { item: 2 }), [2]);
    }
}
