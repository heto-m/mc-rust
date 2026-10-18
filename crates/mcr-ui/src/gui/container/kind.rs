#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceKind {
    Furnace,
    Smoker,
    BlastFurnace,
}

impl FurnaceKind {
    pub fn texture(self) -> &'static str {
        match self {
            Self::Furnace => "container/furnace",
            Self::Smoker => "container/smoker",
            Self::BlastFurnace => "container/blast_furnace",
        }
    }

    pub fn sprite_dir(self) -> &'static str {
        match self {
            Self::Furnace => "container/furnace",
            Self::Smoker => "container/smoker",
            Self::BlastFurnace => "container/blast_furnace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Generic {
        rows: u8,
    },
    Dispenser,
    Crafter,
    Anvil,
    Beacon,
    Furnace(FurnaceKind),
    BrewingStand,
    Crafting,
    Enchantment,
    Grindstone,
    Hopper,
    Lectern,
    Loom,
    Merchant,
    ShulkerBox,
    Smithing,
    CartographyTable,
    Stonecutter,
    PlayerInventory,
    /// Horse, donkey, mule, llama, camel and the like; `chest_columns` is 0 without a chest.
    Mount {
        chest_columns: u8,
        saddle: bool,
        armor: bool,
        llama: bool,
    },
    Nautilus {
        saddle: bool,
        armor: bool,
    },
}

/// 26.3 `minecraft:menu` registry order.
const MENU_NAMES: [&str; 25] = [
    "generic_9x1",
    "generic_9x2",
    "generic_9x3",
    "generic_9x4",
    "generic_9x5",
    "generic_9x6",
    "generic_3x3",
    "crafter_3x3",
    "anvil",
    "beacon",
    "blast_furnace",
    "brewing_stand",
    "crafting",
    "enchantment",
    "furnace",
    "grindstone",
    "hopper",
    "lectern",
    "loom",
    "merchant",
    "shulker_box",
    "smithing",
    "smoker",
    "cartography_table",
    "stonecutter",
];

impl MenuKind {
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Some(match name {
            "generic_9x1" => Self::Generic { rows: 1 },
            "generic_9x2" => Self::Generic { rows: 2 },
            "generic_9x3" => Self::Generic { rows: 3 },
            "generic_9x4" => Self::Generic { rows: 4 },
            "generic_9x5" => Self::Generic { rows: 5 },
            "generic_9x6" => Self::Generic { rows: 6 },
            "generic_3x3" => Self::Dispenser,
            "crafter_3x3" => Self::Crafter,
            "anvil" => Self::Anvil,
            "beacon" => Self::Beacon,
            "blast_furnace" => Self::Furnace(FurnaceKind::BlastFurnace),
            "brewing_stand" => Self::BrewingStand,
            "crafting" => Self::Crafting,
            "enchantment" => Self::Enchantment,
            "furnace" => Self::Furnace(FurnaceKind::Furnace),
            "grindstone" => Self::Grindstone,
            "hopper" => Self::Hopper,
            "lectern" => Self::Lectern,
            "loom" => Self::Loom,
            "merchant" => Self::Merchant,
            "shulker_box" => Self::ShulkerBox,
            "smithing" => Self::Smithing,
            "smoker" => Self::Furnace(FurnaceKind::Smoker),
            "cartography_table" => Self::CartographyTable,
            "stonecutter" => Self::Stonecutter,
            _ => return None,
        })
    }

    /// Network id from the `open_screen` packet.
    pub fn from_registry_id(id: i32) -> Option<Self> {
        Self::from_name(MENU_NAMES.get(usize::try_from(id).ok()?)?)
    }

    pub fn menu_names() -> &'static [&'static str] {
        &MENU_NAMES
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Normal,
    Result,
    Locked,
    Armor(u8),
    Offhand,
    Saddle,
    MountArmor,
    Crafter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotSpec {
    pub x: i32,
    pub y: i32,
    pub group: u8,
    pub kind: SlotKind,
    pub active: bool,
    pub icon: Option<&'static str>,
    /// False in menus whose `canTakeItemForPickAll` refuses (the merchant).
    pub pick_all: bool,
}

impl SlotSpec {
    fn new(x: i32, y: i32, group: u8) -> Self {
        Self {
            x,
            y,
            group,
            kind: SlotKind::Normal,
            active: true,
            icon: None,
            pick_all: true,
        }
    }

    fn kind(mut self, kind: SlotKind) -> Self {
        self.kind = kind;
        self
    }

    fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn may_pickup(&self) -> bool {
        self.kind != SlotKind::Locked
    }

    pub fn takes_pick_all(&self) -> bool {
        self.pick_all && !matches!(self.kind, SlotKind::Result | SlotKind::Locked)
    }
}

pub const GROUP_MAIN: u8 = 0;
pub const GROUP_PLAYER: u8 = 1;
pub const GROUP_RESULT: u8 = 2;
pub const GROUP_GRID: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct ContainerLayout {
    pub kind: MenuKind,
    pub width: i32,
    pub height: i32,
    pub slots: Vec<SlotSpec>,
    pub texture: &'static str,
    pub title_pos: (i32, i32),
    pub title_centered: bool,
    pub inventory_label: Option<(i32, i32)>,
}

fn player_slots(slots: &mut Vec<SlotSpec>, left: i32, top: i32) {
    for row in 0..3 {
        for col in 0..9 {
            slots.push(SlotSpec::new(left + col * 18, top + row * 18, GROUP_PLAYER));
        }
    }
    for col in 0..9 {
        slots.push(SlotSpec::new(left + col * 18, top + 58, GROUP_PLAYER));
    }
}

fn grid(slots: &mut Vec<SlotSpec>, group: u8, left: i32, top: i32, cols: i32, rows: i32) {
    for y in 0..rows {
        for x in 0..cols {
            slots.push(SlotSpec::new(left + x * 18, top + y * 18, group));
        }
    }
}

fn one(x: i32, y: i32) -> SlotSpec {
    SlotSpec::new(x, y, GROUP_MAIN)
}

fn result(x: i32, y: i32) -> SlotSpec {
    SlotSpec::new(x, y, GROUP_RESULT).kind(SlotKind::Result)
}

impl ContainerLayout {
    pub fn new(kind: MenuKind) -> Self {
        let mut slots = Vec::new();
        let (mut width, mut height) = (176, 166);
        let mut texture = "container/dispenser";
        let mut title_pos = (8, 6);
        let mut title_centered = false;
        let mut inventory_label = Some((8, 72));
        match kind {
            MenuKind::Generic { rows } => {
                let rows = i32::from(rows.clamp(1, 6));
                grid(&mut slots, GROUP_MAIN, 8, 18, 9, rows);
                player_slots(&mut slots, 8, 18 + rows * 18 + 13);
                height = 114 + rows * 18;
                texture = "container/generic_54";
                inventory_label = Some((8, height - 94));
            }
            MenuKind::Dispenser => {
                grid(&mut slots, GROUP_MAIN, 62, 17, 3, 3);
                player_slots(&mut slots, 8, 84);
                title_centered = true;
            }
            MenuKind::Hopper => {
                grid(&mut slots, GROUP_MAIN, 44, 20, 5, 1);
                player_slots(&mut slots, 8, 51);
                height = 133;
                texture = "container/hopper";
                inventory_label = Some((8, height - 94));
            }
            MenuKind::ShulkerBox => {
                grid(&mut slots, GROUP_MAIN, 8, 18, 9, 3);
                player_slots(&mut slots, 8, 84);
                height = 167;
                texture = "container/shulker_box";
                inventory_label = Some((8, height - 94));
            }
            MenuKind::Crafting => {
                slots.push(result(124, 35));
                grid(&mut slots, GROUP_GRID, 30, 17, 3, 3);
                player_slots(&mut slots, 8, 84);
                texture = "container/crafting_table";
                title_pos = (29, 6);
            }
            MenuKind::Crafter => {
                for y in 0..3 {
                    for x in 0..3 {
                        slots.push(
                            SlotSpec::new(26 + x * 18, 17 + y * 18, GROUP_GRID)
                                .kind(SlotKind::Crafter),
                        );
                    }
                }
                player_slots(&mut slots, 8, 84);
                slots.push(SlotSpec::new(134, 35, GROUP_RESULT).kind(SlotKind::Locked));
                texture = "container/crafter";
                title_centered = true;
            }
            MenuKind::PlayerInventory => {
                slots.push(result(154, 28));
                grid(&mut slots, GROUP_GRID, 98, 18, 2, 2);
                let armor_icons = [
                    "container/slot/helmet",
                    "container/slot/chestplate",
                    "container/slot/leggings",
                    "container/slot/boots",
                ];
                for (i, icon) in armor_icons.into_iter().enumerate() {
                    slots.push(
                        SlotSpec::new(8, 8 + i as i32 * 18, GROUP_PLAYER)
                            .kind(SlotKind::Armor(i as u8))
                            .icon(icon),
                    );
                }
                player_slots(&mut slots, 8, 84);
                slots.push(
                    SlotSpec::new(77, 62, GROUP_PLAYER)
                        .kind(SlotKind::Offhand)
                        .icon("container/slot/shield"),
                );
                texture = "container/inventory";
                title_pos = (97, 8);
                inventory_label = None;
            }
            MenuKind::Furnace(furnace) => {
                slots.push(one(56, 17));
                slots.push(one(56, 53));
                slots.push(result(116, 35));
                player_slots(&mut slots, 8, 84);
                texture = furnace.texture();
                title_centered = true;
            }
            MenuKind::BrewingStand => {
                slots.push(one(56, 51));
                slots.push(one(79, 58));
                slots.push(one(102, 51));
                slots.push(one(79, 17));
                slots.push(one(17, 17));
                player_slots(&mut slots, 8, 84);
                texture = "container/brewing_stand";
                title_centered = true;
            }
            MenuKind::Enchantment => {
                slots.push(one(15, 47));
                slots.push(one(35, 47).icon("container/slot/lapis_lazuli"));
                player_slots(&mut slots, 8, 84);
                texture = "container/enchanting_table";
            }
            MenuKind::Anvil => {
                slots.push(one(27, 47));
                slots.push(one(76, 47));
                slots.push(result(134, 47));
                player_slots(&mut slots, 8, 84);
                texture = "container/anvil";
                title_pos = (60, 6);
            }
            MenuKind::Beacon => {
                slots.push(one(136, 110));
                player_slots(&mut slots, 36, 137);
                width = 230;
                height = 219;
                texture = "container/beacon";
                inventory_label = None;
                title_pos = (-1000, -1000);
            }
            MenuKind::Grindstone => {
                slots.push(one(49, 19));
                slots.push(one(49, 40));
                slots.push(result(129, 34));
                player_slots(&mut slots, 8, 84);
                texture = "container/grindstone";
            }
            MenuKind::CartographyTable => {
                slots.push(one(15, 15));
                slots.push(one(15, 52));
                slots.push(result(145, 39));
                player_slots(&mut slots, 8, 84);
                texture = "container/cartography_table";
                title_pos = (8, 4);
            }
            MenuKind::Loom => {
                slots.push(one(13, 26).icon("container/slot/banner"));
                slots.push(one(33, 26).icon("container/slot/dye"));
                slots.push(one(23, 45).icon("container/slot/banner_pattern"));
                slots.push(result(143, 57));
                player_slots(&mut slots, 8, 84);
                texture = "container/loom";
                title_pos = (8, 4);
            }
            MenuKind::Stonecutter => {
                slots.push(one(20, 33));
                slots.push(result(143, 33));
                player_slots(&mut slots, 8, 84);
                texture = "container/stonecutter";
                title_pos = (8, 5);
            }
            MenuKind::Smithing => {
                slots.push(one(8, 48).icon("container/slot/smithing_template_armor_trim"));
                slots.push(one(26, 48).icon("container/slot/helmet"));
                slots.push(one(44, 48).icon("container/slot/ingot"));
                slots.push(result(98, 48));
                player_slots(&mut slots, 8, 84);
                texture = "container/smithing";
                title_pos = (44, 15);
            }
            MenuKind::Merchant => {
                slots.push(one(136, 37));
                slots.push(one(162, 37));
                slots.push(result(220, 37));
                player_slots(&mut slots, 108, 84);
                slots.iter_mut().for_each(|s| s.pick_all = false);
                width = 276;
                texture = "container/villager";
                inventory_label = Some((107, 72));
                title_pos = (-1000, -1000);
            }
            MenuKind::Lectern => {
                slots.push(one(0, 0).kind(SlotKind::Locked).active(false));
                texture = "book";
                width = 192;
                height = 192;
                inventory_label = None;
                title_pos = (-1000, -1000);
            }
            MenuKind::Mount {
                chest_columns,
                saddle,
                armor,
                llama,
            } => {
                slots.push(
                    one(8, 18)
                        .kind(SlotKind::Saddle)
                        .icon("container/slot/saddle")
                        .active(saddle),
                );
                slots.push(
                    one(8, 36)
                        .kind(SlotKind::MountArmor)
                        .icon(if llama {
                            "container/slot/llama_armor"
                        } else {
                            "container/slot/horse_armor"
                        })
                        .active(armor || llama),
                );
                grid(
                    &mut slots,
                    GROUP_MAIN,
                    80,
                    18,
                    i32::from(chest_columns.min(5)),
                    3,
                );
                player_slots(&mut slots, 8, 84);
                texture = "container/horse";
            }
            MenuKind::Nautilus { saddle, armor } => {
                slots.push(
                    one(8, 18)
                        .kind(SlotKind::Saddle)
                        .icon("container/slot/saddle")
                        .active(saddle),
                );
                slots.push(
                    one(8, 36)
                        .kind(SlotKind::MountArmor)
                        .icon("container/slot/nautilus_armor_inventory")
                        .active(armor),
                );
                player_slots(&mut slots, 8, 84);
                texture = "container/nautilus";
            }
        }
        Self {
            kind,
            width,
            height,
            slots,
            texture,
            title_pos,
            title_centered,
            inventory_label,
        }
    }

    pub fn player_start(&self) -> usize {
        self.slots
            .iter()
            .position(|s| s.group == GROUP_PLAYER && !matches!(s.kind, SlotKind::Armor(_)))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(name: &str) -> ContainerLayout {
        ContainerLayout::new(MenuKind::from_name(name).unwrap())
    }

    #[test]
    fn registry_order_covers_all_menu_types() {
        for (id, name) in MenuKind::menu_names().iter().enumerate() {
            let kind = MenuKind::from_registry_id(id as i32).unwrap();
            assert_eq!(Some(kind), MenuKind::from_name(name), "{name}");
        }
        assert!(MenuKind::from_registry_id(25).is_none());
        assert!(MenuKind::from_registry_id(-1).is_none());
    }

    #[test]
    fn chest_rows_layout() {
        for rows in 1..=6usize {
            let l = layout(&format!("generic_9x{rows}"));
            assert_eq!(l.slots.len(), rows * 9 + 36);
            assert_eq!(l.height, 114 + rows as i32 * 18);
            assert_eq!(l.slots[0], SlotSpec::new(8, 18, GROUP_MAIN));
            let p = l.player_start();
            assert_eq!(p, rows * 9);
            assert_eq!((l.slots[p].x, l.slots[p].y), (8, rows as i32 * 18 + 31));
            let hotbar = &l.slots[p + 27];
            assert_eq!((hotbar.x, hotbar.y), (8, rows as i32 * 18 + 31 + 58));
        }
        assert_eq!(layout("generic_9x3").height, 168);
    }

    #[test]
    fn player_inventory_slot_order() {
        let l = ContainerLayout::new(MenuKind::PlayerInventory);
        assert_eq!(l.slots.len(), 46);
        assert_eq!(l.slots[0].kind, SlotKind::Result);
        assert_eq!((l.slots[0].x, l.slots[0].y), (154, 28));
        assert_eq!((l.slots[1].x, l.slots[1].y), (98, 18));
        assert_eq!((l.slots[4].x, l.slots[4].y), (116, 36));
        assert_eq!(l.slots[5].kind, SlotKind::Armor(0));
        assert_eq!((l.slots[8].x, l.slots[8].y), (8, 62));
        assert_eq!((l.slots[9].x, l.slots[9].y), (8, 84));
        assert_eq!((l.slots[36].x, l.slots[36].y), (8, 142));
        assert_eq!(l.slots[45].kind, SlotKind::Offhand);
        assert_eq!((l.slots[45].x, l.slots[45].y), (77, 62));
    }

    #[test]
    fn crafting_table_slot_order() {
        let l = layout("crafting");
        assert_eq!(l.slots.len(), 46);
        assert_eq!((l.slots[0].x, l.slots[0].y), (124, 35));
        assert_eq!((l.slots[1].x, l.slots[1].y), (30, 17));
        assert_eq!((l.slots[9].x, l.slots[9].y), (66, 53));
        assert_eq!(l.player_start(), 10);
    }

    #[test]
    fn crafter_has_result_after_inventory() {
        let l = layout("crafter_3x3");
        assert_eq!(l.slots.len(), 46);
        assert_eq!(l.slots[45].kind, SlotKind::Locked);
        assert_eq!(l.player_start(), 9);
    }

    #[test]
    fn slot_counts_match_vanilla() {
        let expect = [
            ("generic_3x3", 9 + 36),
            ("anvil", 3 + 36),
            ("beacon", 1 + 36),
            ("furnace", 3 + 36),
            ("smoker", 3 + 36),
            ("blast_furnace", 3 + 36),
            ("brewing_stand", 5 + 36),
            ("enchantment", 2 + 36),
            ("grindstone", 3 + 36),
            ("hopper", 5 + 36),
            ("loom", 4 + 36),
            ("merchant", 3 + 36),
            ("shulker_box", 27 + 36),
            ("smithing", 4 + 36),
            ("cartography_table", 3 + 36),
            ("stonecutter", 2 + 36),
        ];
        for (name, count) in expect {
            assert_eq!(layout(name).slots.len(), count, "{name}");
        }
    }

    #[test]
    fn merchant_slots_refuse_pick_all() {
        let l = layout("merchant");
        assert!(l.slots.iter().all(|s| !s.takes_pick_all()));
        assert!(layout("generic_9x3").slots[0].takes_pick_all());
    }

    #[test]
    fn mount_chest_slots_follow_equipment() {
        let l = ContainerLayout::new(MenuKind::Mount {
            chest_columns: 5,
            saddle: true,
            armor: false,
            llama: false,
        });
        assert_eq!(l.slots.len(), 2 + 15 + 36);
        assert!(l.slots[0].active && !l.slots[1].active);
        assert_eq!((l.slots[2].x, l.slots[2].y), (80, 18));
        assert_eq!(l.player_start(), 17);
    }

    #[test]
    fn merchant_is_wide_and_shifts_inventory() {
        let l = layout("merchant");
        assert_eq!(l.width, 276);
        let p = l.player_start();
        assert_eq!((l.slots[p].x, l.slots[p].y), (108, 84));
    }
}
