use std::cell::Cell;

use mcr_buf::{
    Decode, DecodeError, Encode, IdOr, Identifier, Result, VarInt, read_varint, write_varint,
};

use super::common::{HolderSet, ItemStackTemplate, Pairs, optional_varint};
use crate::{packet, types::Component};

const MAX_SLOT_DISPLAY_DEPTH: u32 = 64;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> Result<Self> {
        DEPTH.with(|depth| {
            if depth.get() >= MAX_SLOT_DISPLAY_DEPTH {
                return Err(DecodeError::Other("slot display nested too deeply".into()));
            }
            depth.set(depth.get() + 1);
            Ok(Self)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TrimPattern {
    pub asset_id: Identifier,
    pub description: Component,
    pub decal: bool,
}

/// Variants follow the `minecraft:slot_display` registry order.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotDisplay {
    Empty,
    AnyFuel,
    WithAnyPotion(Box<SlotDisplay>),
    OnlyWithComponent {
        source: Box<SlotDisplay>,
        component: i32,
    },
    Item(i32),
    ItemStack(ItemStackTemplate),
    Tag(HolderSet),
    Dyed {
        dye: Box<SlotDisplay>,
        target: Box<SlotDisplay>,
    },
    SmithingTrim {
        base: Box<SlotDisplay>,
        material: Box<SlotDisplay>,
        pattern: Box<IdOr<TrimPattern>>,
    },
    WithRemainder {
        input: Box<SlotDisplay>,
        remainder: Box<SlotDisplay>,
    },
    Composite(Vec<SlotDisplay>),
}

impl Encode for SlotDisplay {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Empty => write_varint(out, 0),
            Self::AnyFuel => write_varint(out, 1),
            Self::WithAnyPotion(display) => {
                write_varint(out, 2);
                display.encode(out);
            }
            Self::OnlyWithComponent { source, component } => {
                write_varint(out, 3);
                source.encode(out);
                write_varint(out, *component);
            }
            Self::Item(item) => {
                write_varint(out, 4);
                write_varint(out, *item);
            }
            Self::ItemStack(stack) => {
                write_varint(out, 5);
                stack.encode(out);
            }
            Self::Tag(items) => {
                write_varint(out, 6);
                items.encode(out);
            }
            Self::Dyed { dye, target } => {
                write_varint(out, 7);
                dye.encode(out);
                target.encode(out);
            }
            Self::SmithingTrim {
                base,
                material,
                pattern,
            } => {
                write_varint(out, 8);
                base.encode(out);
                material.encode(out);
                pattern.encode(out);
            }
            Self::WithRemainder { input, remainder } => {
                write_varint(out, 9);
                input.encode(out);
                remainder.encode(out);
            }
            Self::Composite(contents) => {
                write_varint(out, 10);
                contents.encode(out);
            }
        }
    }
}

impl Decode<'_> for SlotDisplay {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let _guard = DepthGuard::enter()?;
        let id = read_varint(input)?;
        Ok(match id {
            0 => Self::Empty,
            1 => Self::AnyFuel,
            2 => Self::WithAnyPotion(Box::decode(input)?),
            3 => Self::OnlyWithComponent {
                source: Box::decode(input)?,
                component: read_varint(input)?,
            },
            4 => Self::Item(read_varint(input)?),
            5 => Self::ItemStack(ItemStackTemplate::decode(input)?),
            6 => Self::Tag(HolderSet::decode(input)?),
            7 => Self::Dyed {
                dye: Box::decode(input)?,
                target: Box::decode(input)?,
            },
            8 => Self::SmithingTrim {
                base: Box::decode(input)?,
                material: Box::decode(input)?,
                pattern: Box::new(IdOr::decode(input)?),
            },
            9 => Self::WithRemainder {
                input: Box::decode(input)?,
                remainder: Box::decode(input)?,
            },
            10 => Self::Composite(Vec::decode(input)?),
            _ => {
                return Err(DecodeError::InvalidValue {
                    what: "slot display",
                    value: id.into(),
                });
            }
        })
    }
}

/// Variants follow the `minecraft:recipe_display` registry order.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum RecipeDisplay {
    CraftingShapeless {
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    CraftingShaped {
        #[mcr(varint)]
        width: i32,
        #[mcr(varint)]
        height: i32,
        ingredients: Vec<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Furnace {
        ingredient: SlotDisplay,
        fuel: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
        #[mcr(varint)]
        duration: i32,
        experience: f32,
    },
    Stonecutter {
        input: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Smithing {
        template: SlotDisplay,
        base: SlotDisplay,
        addition: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RecipeDisplayEntry {
    #[mcr(varint)]
    pub id: i32,
    pub display: RecipeDisplay,
    #[mcr(with = optional_varint)]
    pub group: Option<i32>,
    #[mcr(varint)]
    pub category: i32,
    pub crafting_requirements: Option<Vec<HolderSet>>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RecipeBookEntry {
    pub contents: RecipeDisplayEntry,
    pub flags: u8,
}

impl RecipeBookEntry {
    pub const FLAG_NOTIFICATION: u8 = 1;
    pub const FLAG_HIGHLIGHT: u8 = 2;

    pub fn notification(&self) -> bool {
        self.flags & Self::FLAG_NOTIFICATION != 0
    }

    pub fn highlight(&self) -> bool {
        self.flags & Self::FLAG_HIGHLIGHT != 0
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RecipeBookAdd {
    pub entries: Vec<RecipeBookEntry>,
    pub replace: bool,
}

packet!(RecipeBookAdd, "recipe_book_add", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RecipeBookRemove {
    pub recipes: Vec<VarInt>,
}

packet!(RecipeBookRemove, "recipe_book_remove", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct RecipeBookTypeSettings {
    pub open: bool,
    pub filtering: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct RecipeBookSettings {
    pub crafting: RecipeBookTypeSettings,
    pub furnace: RecipeBookTypeSettings,
    pub blast_furnace: RecipeBookTypeSettings,
    pub smoker: RecipeBookTypeSettings,
}

packet!(RecipeBookSettings, "recipe_book_settings", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PlaceGhostRecipe {
    #[mcr(varint)]
    pub container_id: i32,
    pub recipe_display: RecipeDisplay,
}

packet!(PlaceGhostRecipe, "place_ghost_recipe", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct StonecutterRecipe {
    pub input: HolderSet,
    pub option_display: SlotDisplay,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UpdateRecipes {
    pub item_sets: Pairs<Identifier, Vec<VarInt>>,
    pub stonecutter_recipes: Vec<StonecutterRecipe>,
}

packet!(UpdateRecipes, "update_recipes", Clientbound);

#[cfg(test)]
mod tests {
    use mcr_buf::to_bytes;

    use super::*;
    use crate::{play::clientbound::ui::common::ComponentPatch, test_util::roundtrip};

    fn item(id: i32) -> SlotDisplay {
        SlotDisplay::Item(id)
    }

    #[test]
    fn slot_display_variants() {
        assert_eq!(
            roundtrip(&SlotDisplay::Composite(vec![item(5), SlotDisplay::Empty])),
            [10, 2, 4, 5, 0]
        );
        let nested = SlotDisplay::Dyed {
            dye: Box::new(SlotDisplay::WithAnyPotion(Box::new(SlotDisplay::AnyFuel))),
            target: Box::new(SlotDisplay::OnlyWithComponent {
                source: Box::new(SlotDisplay::Tag(HolderSet::Direct(vec![1, 2]))),
                component: 9,
            }),
        };
        assert_eq!(roundtrip(&nested), [7, 2, 1, 3, 6, 3, 1, 2, 9]);
        let trim = SlotDisplay::SmithingTrim {
            base: Box::new(item(1)),
            material: Box::new(item(2)),
            pattern: Box::new(IdOr::Id(3)),
        };
        assert_eq!(roundtrip(&trim), [8, 4, 1, 4, 2, 4]);
        roundtrip(&SlotDisplay::WithRemainder {
            input: Box::new(item(1)),
            remainder: Box::new(SlotDisplay::Empty),
        });
    }

    #[test]
    fn slot_display_rejects_unknown_id_and_deep_nesting() {
        assert!(SlotDisplay::decode(&mut &[11u8][..]).is_err());
        let deep = vec![2u8; 10_000];
        assert!(SlotDisplay::decode(&mut deep.as_slice()).is_err());
        assert!(SlotDisplay::decode(&mut &[0u8][..]).is_ok());
    }

    #[test]
    fn recipe_display_variants() {
        let shaped = RecipeDisplay::CraftingShaped {
            width: 2,
            height: 1,
            ingredients: vec![item(1), SlotDisplay::Empty],
            result: item(3),
            crafting_station: item(4),
        };
        assert_eq!(roundtrip(&shaped), [1, 2, 1, 2, 4, 1, 0, 4, 3, 4, 4]);
        let furnace = RecipeDisplay::Furnace {
            ingredient: item(1),
            fuel: SlotDisplay::AnyFuel,
            result: item(2),
            crafting_station: SlotDisplay::Empty,
            duration: 200,
            experience: 0.5,
        };
        assert_eq!(
            roundtrip(&furnace),
            [2, 4, 1, 1, 4, 2, 0, 0xc8, 1, 0x3f, 0, 0, 0]
        );
        roundtrip(&RecipeDisplay::Stonecutter {
            input: item(1),
            result: item(2),
            crafting_station: item(3),
        });
        roundtrip(&RecipeDisplay::Smithing {
            template: item(1),
            base: item(2),
            addition: item(3),
            result: item(4),
            crafting_station: item(5),
        });
        roundtrip(&RecipeDisplay::CraftingShapeless {
            ingredients: vec![item(1)],
            result: item(2),
            crafting_station: item(3),
        });
        assert!(RecipeDisplay::decode(&mut &[5u8][..]).is_err());
    }

    #[test]
    fn recipe_book_add_bytes() {
        let packet = RecipeBookAdd {
            entries: vec![RecipeBookEntry {
                contents: RecipeDisplayEntry {
                    id: 7,
                    display: RecipeDisplay::Stonecutter {
                        input: item(1),
                        result: item(2),
                        crafting_station: SlotDisplay::Empty,
                    },
                    group: Some(0),
                    category: 3,
                    crafting_requirements: Some(vec![HolderSet::Direct(vec![9])]),
                },
                flags: 3,
            }],
            replace: true,
        };
        assert!(packet.entries[0].notification() && packet.entries[0].highlight());
        #[rustfmt::skip]
        assert_eq!(roundtrip(&packet), [
            1,
            7, 3, 4, 1, 4, 2, 0, 1, 3, 1, 1, 2, 9, 3,
            1,
        ]);
    }

    #[test]
    fn recipe_book_small_packets() {
        assert_eq!(
            roundtrip(&RecipeBookRemove {
                recipes: vec![VarInt(1), VarInt(300)],
            }),
            [2, 1, 0xac, 2]
        );
        let settings = RecipeBookSettings {
            crafting: RecipeBookTypeSettings {
                open: true,
                filtering: false,
            },
            smoker: RecipeBookTypeSettings {
                open: false,
                filtering: true,
            },
            ..Default::default()
        };
        assert_eq!(roundtrip(&settings), [1, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn update_recipes_bytes() {
        let packet = UpdateRecipes {
            item_sets: Pairs(vec![(
                Identifier::parse("smithing_base").unwrap(),
                vec![VarInt(4)],
            )]),
            stonecutter_recipes: vec![StonecutterRecipe {
                input: HolderSet::Direct(vec![2]),
                option_display: item(8),
            }],
        };
        let mut expected = vec![1, 23];
        expected.extend_from_slice(b"minecraft:smithing_base");
        expected.extend_from_slice(&[1, 4, 1, 2, 2, 4, 8]);
        assert_eq!(roundtrip(&packet), expected);
        assert_eq!(
            to_bytes(&UpdateRecipes {
                item_sets: Pairs(vec![]),
                stonecutter_recipes: vec![],
            }),
            [0, 0]
        );
    }

    #[test]
    fn place_ghost_recipe_roundtrip() {
        roundtrip(&PlaceGhostRecipe {
            container_id: 2,
            recipe_display: RecipeDisplay::Stonecutter {
                input: item(1),
                result: item(2),
                crafting_station: item(3),
            },
        });
    }

    #[test]
    #[ignore = "needs data component patch"]
    fn item_stack_slot_display_roundtrip() {
        roundtrip(&SlotDisplay::ItemStack(ItemStackTemplate {
            item: 1,
            count: 1,
            components: ComponentPatch::default(),
        }));
    }
}
