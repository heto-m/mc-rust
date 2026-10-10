//! Slot layouts and `quickMoveStack` routing for every menu type.

use std::ops::Range;

use mcr_data::Version;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::EquipmentSlot;
use rustc_hash::FxHashMap;

use super::{Menu, Rules, split};

const PLAYER_SLOTS: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountInfo {
    pub columns: usize,
    pub entity_type: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FurnaceKind {
    Furnace,
    Blast,
    Smoker,
}

impl FurnaceKind {
    fn input_set(self) -> &'static str {
        match self {
            Self::Furnace => "minecraft:furnace_input",
            Self::Blast => "minecraft:blast_furnace_input",
            Self::Smoker => "minecraft:smoker_input",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultKind {
    Crafting {
        grid: Range<usize>,
        width: usize,
    },
    Furnace,
    Smithing,
    Grindstone,
    Stonecutter,
    Loom,
    Cartography,
    /// Anvil and merchant results depend on state the client does not model.
    Unpredicted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotKind {
    Normal,
    Armor(EquipmentSlot),
    MountArmor(EquipmentSlot),
    Result(ResultKind),
    Fuel,
    BrewPotion,
    BrewReagent,
    BrewFuel,
    Lapis,
    EnchantItem,
    SmithTemplate,
    SmithBase,
    SmithAddition,
    Grind,
    Banner,
    Dye,
    Pattern,
    MapItem,
    MapMaterial,
    Beacon,
    Crafter(usize),
    NoShulker,
    /// Display-only slots nobody can place into or take from.
    Locked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Player,
    Crafting,
    /// Container slots followed by the 36 player slots; covers chests, dispensers, hoppers and shulker boxes.
    Chest {
        container: usize,
        shulker: bool,
    },
    Furnace(FurnaceKind),
    Brewing,
    Enchantment,
    Anvil,
    Smithing,
    Grindstone,
    Cartography,
    Loom,
    Stonecutter,
    Beacon,
    Merchant,
    Crafter,
    Mount(MountInfo),
    /// Only the trailing 36 player slots are understood.
    Opaque,
}

impl Layout {
    pub fn from_menu(
        data: &Version,
        id: i32,
        menu_type: Option<i32>,
        slots: usize,
        mount: Option<MountInfo>,
    ) -> Self {
        let layout = Self::classify(data, id, menu_type, slots, mount);
        if layout.slot_count().is_some_and(|n| n != slots) {
            Self::Opaque
        } else {
            layout
        }
    }

    fn slot_count(self) -> Option<usize> {
        Some(match self {
            Self::Player => 46,
            Self::Crafting => 46,
            Self::Chest { container, .. } => container + PLAYER_SLOTS,
            Self::Furnace(_) => 3 + PLAYER_SLOTS,
            Self::Brewing => 5 + PLAYER_SLOTS,
            Self::Enchantment => 2 + PLAYER_SLOTS,
            Self::Anvil | Self::Grindstone | Self::Cartography | Self::Merchant => 3 + PLAYER_SLOTS,
            Self::Smithing | Self::Loom => 4 + PLAYER_SLOTS,
            Self::Stonecutter => 2 + PLAYER_SLOTS,
            Self::Beacon => 1 + PLAYER_SLOTS,
            Self::Crafter => 10 + PLAYER_SLOTS,
            Self::Mount(m) => 2 + 3 * m.columns + PLAYER_SLOTS,
            Self::Opaque => return None,
        })
    }

    fn classify(
        data: &Version,
        id: i32,
        menu_type: Option<i32>,
        slots: usize,
        mount: Option<MountInfo>,
    ) -> Self {
        if id == 0 {
            return Self::Player;
        }
        if menu_type == Some(-1) {
            return mount.map_or(Self::Opaque, Self::Mount);
        }
        let Some(name) = menu_type
            .and_then(|t| u32::try_from(t).ok())
            .and_then(|t| data.registry("minecraft:menu")?.name_of(t))
        else {
            return Self::Opaque;
        };
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        let chest = |container, shulker| Self::Chest { container, shulker };
        match name {
            "crafting" => Self::Crafting,
            "generic_3x3" => chest(9, false),
            "hopper" => chest(5, false),
            "shulker_box" => chest(27, true),
            "furnace" => Self::Furnace(FurnaceKind::Furnace),
            "blast_furnace" => Self::Furnace(FurnaceKind::Blast),
            "smoker" => Self::Furnace(FurnaceKind::Smoker),
            "brewing_stand" => Self::Brewing,
            "enchantment" => Self::Enchantment,
            "anvil" => Self::Anvil,
            "smithing" => Self::Smithing,
            "grindstone" => Self::Grindstone,
            "cartography_table" => Self::Cartography,
            "loom" => Self::Loom,
            "stonecutter" => Self::Stonecutter,
            "beacon" => Self::Beacon,
            "merchant" => Self::Merchant,
            "crafter_3x3" => Self::Crafter,
            n => n
                .strip_prefix("generic_9x")
                .and_then(|r| r.parse::<usize>().ok())
                .filter(|r| (1..=6).contains(r) && slots >= r * 9)
                .map_or(Self::Opaque, |r| chest(r * 9, false)),
        }
    }

    pub(super) fn kind(self, slot: usize, total: usize) -> SlotKind {
        use SlotKind as K;
        let inventory_start = total.saturating_sub(PLAYER_SLOTS);
        let normal = K::Normal;
        match self {
            Self::Player => match slot {
                0 => K::Result(ResultKind::Crafting {
                    grid: 1..5,
                    width: 2,
                }),
                5 => K::Armor(EquipmentSlot::Head),
                6 => K::Armor(EquipmentSlot::Chest),
                7 => K::Armor(EquipmentSlot::Legs),
                8 => K::Armor(EquipmentSlot::Feet),
                _ => normal,
            },
            Self::Crafting => {
                if slot == 0 {
                    K::Result(ResultKind::Crafting {
                        grid: 1..10,
                        width: 3,
                    })
                } else {
                    normal
                }
            }
            Self::Chest { container, shulker } => {
                if shulker && slot < container {
                    K::NoShulker
                } else {
                    normal
                }
            }
            Self::Furnace(_) => match slot {
                1 => K::Fuel,
                2 => K::Result(ResultKind::Furnace),
                _ => normal,
            },
            Self::Brewing => match slot {
                0..=2 => K::BrewPotion,
                3 => K::BrewReagent,
                4 => K::BrewFuel,
                _ => normal,
            },
            Self::Enchantment => match slot {
                0 => K::EnchantItem,
                1 => K::Lapis,
                _ => normal,
            },
            Self::Anvil => {
                if slot == 2 {
                    K::Result(ResultKind::Unpredicted)
                } else {
                    normal
                }
            }
            Self::Smithing => match slot {
                0 => K::SmithTemplate,
                1 => K::SmithBase,
                2 => K::SmithAddition,
                3 => K::Result(ResultKind::Smithing),
                _ => normal,
            },
            Self::Grindstone => match slot {
                0 | 1 => K::Grind,
                2 => K::Result(ResultKind::Grindstone),
                _ => normal,
            },
            Self::Cartography => match slot {
                0 => K::MapItem,
                1 => K::MapMaterial,
                2 => K::Result(ResultKind::Cartography),
                _ => normal,
            },
            Self::Loom => match slot {
                0 => K::Banner,
                1 => K::Dye,
                2 => K::Pattern,
                3 => K::Result(ResultKind::Loom),
                _ => normal,
            },
            Self::Stonecutter => {
                if slot == 1 {
                    K::Result(ResultKind::Stonecutter)
                } else {
                    normal
                }
            }
            Self::Beacon => {
                if slot == 0 {
                    K::Beacon
                } else {
                    normal
                }
            }
            Self::Merchant => {
                if slot == 2 {
                    K::Result(ResultKind::Unpredicted)
                } else {
                    normal
                }
            }
            Self::Crafter => match slot {
                0..=8 => K::Crafter(slot),
                45 => K::Locked,
                _ => normal,
            },
            Self::Mount(_) => match slot {
                0 => K::MountArmor(EquipmentSlot::Saddle),
                1 => K::MountArmor(EquipmentSlot::Body),
                _ => normal,
            },
            Self::Opaque => {
                if total >= PLAYER_SLOTS && slot >= inventory_start {
                    normal
                } else {
                    K::Unknown
                }
            }
        }
    }

    pub(super) fn may_place(
        self,
        rules: &Rules<'_>,
        data: &FxHashMap<i16, i16>,
        kind: SlotKind,
        s: &ItemStack,
    ) -> bool {
        use SlotKind as K;
        match kind {
            K::Normal | K::EnchantItem => true,
            K::Result(_) | K::Locked | K::Unknown => false,
            K::Armor(slot) => rules.equip(s).is_some_and(|e| e.slot == slot),
            K::MountArmor(slot) => self.mount_may_place(rules, slot, s),
            K::Fuel => rules.is_fuel(s) || rules.is_bucket(s),
            K::BrewPotion => rules.is_potion_input(s),
            K::BrewReagent => rules.in_recipe_set("minecraft:brewing_reagent", s),
            K::BrewFuel => rules.has(s, "minecraft:brewing_fuel"),
            K::Lapis => rules.is_item(s, "minecraft:lapis_lazuli"),
            K::SmithTemplate => rules.in_recipe_set("minecraft:smithing_template", s),
            K::SmithBase => rules.in_recipe_set("minecraft:smithing_base", s),
            K::SmithAddition => rules.in_recipe_set("minecraft:smithing_addition", s),
            K::Grind => rules.is_damageable(s) || rules.is_enchanted(s),
            K::Banner => rules.is_banner(s),
            K::Dye => rules.is_loom_dye(s),
            K::Pattern => rules.is_loom_pattern(s),
            K::MapItem => rules.has(s, "minecraft:map_id"),
            K::MapMaterial => {
                rules.is_item(s, "minecraft:paper")
                    || rules.is_item(s, "minecraft:map")
                    || rules.is_item(s, "minecraft:glass_pane")
            }
            K::Beacon => rules.in_tag(s, "minecraft:beacon_payment_items"),
            K::Crafter(i) => data.get(&(i as i16)).copied() != Some(1),
            K::NoShulker => !rules.is_shulker(s),
        }
    }

    fn mount_may_place(self, rules: &Rules<'_>, slot: EquipmentSlot, s: &ItemStack) -> bool {
        let Self::Mount(mount) = self else {
            return false;
        };
        let Some(equip) = rules.equip(s).filter(|e| e.slot == slot) else {
            return false;
        };
        if equip
            .allowed_entities
            .is_some_and(|set| !rules.entity_in_set(set, mount.entity_type))
        {
            return false;
        }
        let in_tag = |tag: &str| {
            rules
                .registries
                .is_in_tag("minecraft:entity_type", tag, mount.entity_type)
        };
        let llama = rules
            .data
            .entity_type_name(mount.entity_type as u32)
            .is_some_and(|n| n.ends_with("llama"));
        match slot {
            EquipmentSlot::Saddle => in_tag("minecraft:can_equip_saddle"),
            _ => {
                in_tag("minecraft:can_wear_horse_armor")
                    || llama
                    || in_tag("minecraft:can_wear_nautilus_armor")
            }
        }
    }

    pub(super) fn may_pickup(self, rules: &Rules<'_>, kind: SlotKind, s: &ItemStack) -> bool {
        match kind {
            SlotKind::Locked => false,
            SlotKind::Armor(_) | SlotKind::MountArmor(_) => {
                s.is_empty() || rules.creative || !rules.has_binding_curse(s)
            }
            _ => true,
        }
    }

    pub(super) fn can_take_for_pickup_all(self, kind: SlotKind) -> bool {
        !matches!(
            kind,
            SlotKind::Result(
                ResultKind::Crafting { .. }
                    | ResultKind::Stonecutter
                    | ResultKind::Smithing
                    | ResultKind::Cartography
                    | ResultKind::Unpredicted
            )
        )
    }
}

enum Flow {
    /// The vanilla method ran its common tail: compare counts, `onTake`, return the clicked copy.
    Tail,
    ReturnEmpty,
    ReturnClicked,
}

impl Menu<'_> {
    pub(super) fn quick_move(&mut self, i: usize) -> ItemStack {
        if self.slots[i].is_empty() {
            return ItemStack::default();
        }
        let original = self.slots[i].clone();
        let mut stack = original.clone();
        let flow = self.route(i, &mut stack, &original);
        // The vanilla stack is the slot's own object, so partial moves land in the slot even on early returns.
        self.slots[i] = if stack.is_empty() {
            ItemStack::default()
        } else {
            stack.clone()
        };
        match flow {
            Flow::ReturnEmpty => ItemStack::default(),
            Flow::ReturnClicked => original,
            Flow::Tail => {
                if stack.count == original.count {
                    return ItemStack::default();
                }
                if matches!(self.kind(i), SlotKind::Result(_)) && !stack.is_empty() {
                    // The leftover of a result stack is dropped by the player.
                    self.unknown = true;
                }
                self.on_take(i);
                original
            }
        }
    }

    fn mv(&mut self, stack: &mut ItemStack, range: Range<usize>, backwards: bool) -> bool {
        self.move_to(stack, range.start, range.end, backwards)
    }

    fn tail(ok: bool) -> Flow {
        if ok { Flow::Tail } else { Flow::ReturnEmpty }
    }

    /// Standard container: result/outputs move out backwards, inventory items move in forwards.
    fn route_container(&mut self, i: usize, stack: &mut ItemStack, container: usize) -> Flow {
        let n = self.slots.len();
        let ok = if i < container {
            self.mv(stack, container..n, true)
        } else {
            self.mv(stack, 0..container, false)
        };
        Self::tail(ok)
    }

    /// Slots after `inv` are the 27 main plus 9 hotbar player slots; moves between them.
    fn route_player_area(&mut self, i: usize, stack: &mut ItemStack, inv: usize) -> Flow {
        let hotbar = inv + 27;
        let end = hotbar + 9;
        if (inv..hotbar).contains(&i) {
            Self::tail(self.mv(stack, hotbar..end, false))
        } else if (hotbar..end).contains(&i) {
            Self::tail(self.mv(stack, inv..hotbar, false))
        } else {
            Flow::Tail
        }
    }

    fn route(&mut self, i: usize, stack: &mut ItemStack, clicked: &ItemStack) -> Flow {
        let n = self.slots.len();
        match self.layout {
            Layout::Player => self.route_player(i, stack, clicked),
            Layout::Crafting => {
                if i == 0 {
                    Self::tail(self.mv(stack, 10..46, true))
                } else if (10..46).contains(&i) {
                    if self.mv(stack, 1..10, false) {
                        Flow::Tail
                    } else if i < 37 {
                        Self::tail(self.mv(stack, 37..46, false))
                    } else {
                        Self::tail(self.mv(stack, 10..37, false))
                    }
                } else {
                    Self::tail(self.mv(stack, 10..46, false))
                }
            }
            Layout::Chest { container, .. } => self.route_container(i, stack, container),
            Layout::Furnace(kind) => {
                if i == 2 {
                    Self::tail(self.mv(stack, 3..39, true))
                } else if i == 0 || i == 1 {
                    Self::tail(self.mv(stack, 3..39, false))
                } else if self.rules.in_recipe_set(kind.input_set(), clicked) {
                    Self::tail(self.mv(stack, 0..1, false))
                } else if self.rules.is_fuel(clicked) {
                    Self::tail(self.mv(stack, 1..2, false))
                } else {
                    self.route_player_area(i, stack, 3)
                }
            }
            Layout::Brewing => self.route_brewing(i, stack, clicked),
            Layout::Enchantment => {
                if i == 0 || i == 1 {
                    Self::tail(self.mv(stack, 2..38, true))
                } else if self.rules.is_item(clicked, "minecraft:lapis_lazuli") {
                    Self::tail(self.mv(stack, 1..2, true))
                } else {
                    let probe = stack.clone();
                    if !self.slots[0].is_empty() || !self.may_place(0, &probe) {
                        return Flow::ReturnEmpty;
                    }
                    let single = split(stack, 1);
                    self.slots[0] = single;
                    Flow::Tail
                }
            }
            Layout::Anvil | Layout::Smithing => {
                let result = if self.layout == Layout::Anvil { 2 } else { 3 };
                let start = result + 1;
                let end = start + PLAYER_SLOTS;
                if i == result {
                    Self::tail(self.mv(stack, start..end, true))
                } else if i < result {
                    Self::tail(self.mv(stack, start..end, false))
                } else if (start..end).contains(&i) && self.can_move_into_inputs(clicked) {
                    Self::tail(self.mv(stack, 0..result, false))
                } else {
                    self.route_player_area(i, stack, start)
                }
            }
            Layout::Grindstone => {
                if i == 2 {
                    Self::tail(self.mv(stack, 3..39, true))
                } else if i != 0 && i != 1 {
                    if !self.slots[0].is_empty() && !self.slots[1].is_empty() {
                        self.route_player_area(i, stack, 3)
                    } else {
                        Self::tail(self.mv(stack, 0..2, false))
                    }
                } else {
                    Self::tail(self.mv(stack, 3..39, false))
                }
            }
            Layout::Cartography => {
                if i == 2 {
                    Self::tail(self.mv(stack, 3..39, true))
                } else if i != 0 && i != 1 {
                    if self.rules.has(clicked, "minecraft:map_id") {
                        Self::tail(self.mv(stack, 0..1, false))
                    } else if self.rules.is_item(clicked, "minecraft:paper")
                        || self.rules.is_item(clicked, "minecraft:map")
                        || self.rules.is_item(clicked, "minecraft:glass_pane")
                    {
                        Self::tail(self.mv(stack, 1..2, false))
                    } else {
                        self.route_player_area(i, stack, 3)
                    }
                } else {
                    Self::tail(self.mv(stack, 3..39, false))
                }
            }
            Layout::Loom => {
                if i == 3 {
                    Self::tail(self.mv(stack, 4..40, true))
                } else if i > 2 {
                    if self.rules.is_banner(clicked) {
                        Self::tail(self.mv(stack, 0..1, false))
                    } else if self.rules.is_loom_dye(clicked) {
                        Self::tail(self.mv(stack, 1..2, false))
                    } else if self.rules.is_loom_pattern(clicked) {
                        Self::tail(self.mv(stack, 2..3, false))
                    } else {
                        self.route_player_area(i, stack, 4)
                    }
                } else {
                    Self::tail(self.mv(stack, 4..40, false))
                }
            }
            Layout::Stonecutter => {
                if i == 1 {
                    Self::tail(self.mv(stack, 2..38, true))
                } else if i == 0 {
                    Self::tail(self.mv(stack, 2..38, false))
                } else if self.rules.registries.stonecutter_accepts(clicked.item) {
                    Self::tail(self.mv(stack, 0..1, false))
                } else if (2..29).contains(&i) {
                    Self::tail(self.mv(stack, 29..38, false))
                } else if (29..38).contains(&i) {
                    Self::tail(self.mv(stack, 2..29, false))
                } else {
                    Flow::Tail
                }
            }
            Layout::Beacon => {
                if i == 0 {
                    Self::tail(self.mv(stack, 1..37, true))
                } else if self.slots[0].is_empty()
                    && self.may_place(0, clicked)
                    && clicked.count == 1
                {
                    Self::tail(self.mv(stack, 0..1, false))
                } else if (1..28).contains(&i) {
                    Self::tail(self.mv(stack, 28..37, false))
                } else if (28..37).contains(&i) {
                    Self::tail(self.mv(stack, 1..28, false))
                } else {
                    Self::tail(self.mv(stack, 1..37, false))
                }
            }
            Layout::Merchant => {
                if i == 2 {
                    Self::tail(self.mv(stack, 3..39, true))
                } else if i != 0 && i != 1 {
                    self.route_player_area(i, stack, 3)
                } else {
                    Self::tail(self.mv(stack, 3..39, false))
                }
            }
            Layout::Crafter => {
                if i < 9 {
                    Self::tail(self.mv(stack, 9..45, true))
                } else {
                    Self::tail(self.mv(stack, 0..9, false))
                }
            }
            Layout::Mount(info) => self.route_mount(i, stack, info),
            Layout::Opaque => {
                self.unknown = true;
                let _ = n;
                Flow::ReturnEmpty
            }
        }
    }

    fn can_move_into_inputs(&self, s: &ItemStack) -> bool {
        if self.layout != Layout::Smithing {
            return true;
        }
        let r = &self.rules;
        (r.in_recipe_set("minecraft:smithing_template", s) && self.slots[0].is_empty())
            || (r.in_recipe_set("minecraft:smithing_base", s) && self.slots[1].is_empty())
            || (r.in_recipe_set("minecraft:smithing_addition", s) && self.slots[2].is_empty())
    }

    fn route_player(&mut self, i: usize, stack: &mut ItemStack, clicked: &ItemStack) -> Flow {
        let eq = self.rules.equip(clicked).map(|e| e.slot);
        if i == 0 {
            return Self::tail(self.mv(stack, 9..45, true));
        }
        if (1..9).contains(&i) {
            return Self::tail(self.mv(stack, 9..45, false));
        }
        let armor_pos = match eq {
            Some(EquipmentSlot::Head) => Some(5),
            Some(EquipmentSlot::Chest) => Some(6),
            Some(EquipmentSlot::Legs) => Some(7),
            Some(EquipmentSlot::Feet) => Some(8),
            _ => None,
        };
        if let Some(pos) = armor_pos.filter(|&p| self.slots[p].is_empty()) {
            Self::tail(self.mv(stack, pos..pos + 1, false))
        } else if eq == Some(EquipmentSlot::OffHand) && self.slots[45].is_empty() {
            Self::tail(self.mv(stack, 45..46, false))
        } else if (9..36).contains(&i) {
            Self::tail(self.mv(stack, 36..45, false))
        } else if (36..45).contains(&i) {
            Self::tail(self.mv(stack, 9..36, false))
        } else {
            Self::tail(self.mv(stack, 9..45, false))
        }
    }

    fn route_brewing(&mut self, i: usize, stack: &mut ItemStack, clicked: &ItemStack) -> Flow {
        if i > 4 {
            if self.rules.has(clicked, "minecraft:brewing_fuel") {
                let reagent = self.may_place(3, stack);
                if self.mv(stack, 4..5, false) || reagent && !self.mv(stack, 3..4, false) {
                    return Flow::ReturnEmpty;
                }
                Flow::Tail
            } else if self.may_place(3, stack) {
                Self::tail(self.mv(stack, 3..4, false))
            } else if self.rules.is_potion_input(clicked) {
                Self::tail(self.mv(stack, 0..3, false))
            } else if (5..32).contains(&i) {
                Self::tail(self.mv(stack, 32..41, false))
            } else if (32..41).contains(&i) {
                Self::tail(self.mv(stack, 5..32, false))
            } else {
                Self::tail(self.mv(stack, 5..41, false))
            }
        } else {
            Self::tail(self.mv(stack, 5..41, true))
        }
    }

    fn route_mount(&mut self, i: usize, stack: &mut ItemStack, info: MountInfo) -> Flow {
        let container = 3 * info.columns;
        let player_start = 2 + container;
        let n = self.slots.len();
        if i < player_start {
            return if self.mv(stack, player_start..n, true) {
                Flow::ReturnClicked
            } else {
                Flow::ReturnEmpty
            };
        }
        if self.may_place(1, stack) && self.slots[1].is_empty() {
            return if self.mv(stack, 1..2, false) {
                Flow::ReturnClicked
            } else {
                Flow::ReturnEmpty
            };
        }
        if self.may_place(0, stack) && self.slots[0].is_empty() {
            return if self.mv(stack, 0..1, false) {
                Flow::ReturnClicked
            } else {
                Flow::ReturnEmpty
            };
        }
        if container != 0 && self.mv(stack, 2..player_start, false) {
            return Flow::ReturnClicked;
        }
        let inv_end = player_start + 27;
        let hotbar_end = inv_end + 9;
        if (inv_end..hotbar_end).contains(&i) {
            self.mv(stack, player_start..inv_end, false);
        } else if (player_start..inv_end).contains(&i) {
            self.mv(stack, inv_end..hotbar_end, false);
        } else {
            self.mv(stack, inv_end..inv_end, false);
        }
        Flow::ReturnEmpty
    }
}
