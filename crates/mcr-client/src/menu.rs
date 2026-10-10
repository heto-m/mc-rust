//! Client-side container click simulation mirroring vanilla `AbstractContainerMenu#clicked`.

use mcr_data::Version;
use mcr_protocol::play::serverbound::ContainerInput;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, EquipmentSlot};

use crate::inventory::OUTSIDE_SLOT as OUTSIDE;
const MAX_QUICK_MOVE_ROUNDS: usize = 4608;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Normal,
    Armor(EquipmentSlot),
    /// Outputs and slots whose rules depend on recipes or block state the client lacks.
    Unknown,
    NoShulker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Player,
    Crafting,
    /// Container slots followed by the 36 player slots; quick-move shuttles between the two.
    Simple {
        container: usize,
        shulker: bool,
    },
    /// Only the trailing 36 player slots are understood.
    Opaque,
}

impl Layout {
    pub fn from_menu(data: &Version, id: i32, menu_type: Option<i32>, slots: usize) -> Self {
        match Self::classify(data, id, menu_type, slots) {
            Self::Simple { container, .. } if container + 36 != slots => Self::Opaque,
            l => l,
        }
    }

    fn classify(data: &Version, id: i32, menu_type: Option<i32>, slots: usize) -> Self {
        if id == 0 {
            return if slots == 46 {
                Self::Player
            } else {
                Self::Opaque
            };
        }
        let Some(name) = menu_type
            .and_then(|t| u32::try_from(t).ok())
            .and_then(|t| data.registry("minecraft:menu")?.name_of(t))
        else {
            return Self::Opaque;
        };
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        match name {
            "crafting" if slots == 46 => Self::Crafting,
            _ if slots < 36 => Self::Opaque,
            "generic_3x3" => Self::Simple {
                container: 9,
                shulker: false,
            },
            "hopper" => Self::Simple {
                container: 5,
                shulker: false,
            },
            "shulker_box" => Self::Simple {
                container: 27,
                shulker: true,
            },
            n => n
                .strip_prefix("generic_9x")
                .and_then(|r| r.parse::<usize>().ok())
                .filter(|r| (1..=6).contains(r))
                .map_or(Self::Opaque, |r| Self::Simple {
                    container: r * 9,
                    shulker: false,
                }),
        }
    }

    fn kind(self, slot: usize, total: usize) -> SlotKind {
        match self {
            Self::Player => match slot {
                0 => SlotKind::Unknown,
                5 => SlotKind::Armor(EquipmentSlot::Head),
                6 => SlotKind::Armor(EquipmentSlot::Chest),
                7 => SlotKind::Armor(EquipmentSlot::Legs),
                8 => SlotKind::Armor(EquipmentSlot::Feet),
                _ => SlotKind::Normal,
            },
            Self::Crafting => {
                if slot == 0 {
                    SlotKind::Unknown
                } else {
                    SlotKind::Normal
                }
            }
            Self::Simple { container, shulker } => {
                if shulker && slot < container {
                    SlotKind::NoShulker
                } else {
                    SlotKind::Normal
                }
            }
            Self::Opaque => {
                if slot + 36 >= total {
                    SlotKind::Normal
                } else {
                    SlotKind::Unknown
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuickCraft {
    pub status: u8,
    pub kind: u8,
    pub slots: Vec<usize>,
}

pub struct Rules<'a> {
    pub data: &'a Version,
    pub creative: bool,
}

impl Rules<'_> {
    pub fn max_stack(&self, s: &ItemStack) -> i32 {
        for c in &s.components.added {
            if let DataComponent::MaxStackSize(v) = c {
                return v.0.clamp(1, 99);
            }
        }
        u32::try_from(s.item)
            .ok()
            .and_then(|i| self.data.item_info(i))
            .map_or(64, |i| i32::from(i.max_stack_size).max(1))
    }

    fn equip_slot(&self, s: &ItemStack) -> Option<EquipmentSlot> {
        for c in &s.components.added {
            if let DataComponent::Equippable(e) = c {
                return Some(e.slot);
            }
        }
        let name = u32::try_from(s.item)
            .ok()
            .and_then(|i| self.data.item_name(i))?;
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Some(if name.ends_with("_helmet") || name == "carved_pumpkin" {
            EquipmentSlot::Head
        } else if name.ends_with("_chestplate") || name == "elytra" {
            EquipmentSlot::Chest
        } else if name.ends_with("_leggings") {
            EquipmentSlot::Legs
        } else if name.ends_with("_boots") {
            EquipmentSlot::Feet
        } else if name == "shield" {
            EquipmentSlot::OffHand
        } else {
            return None;
        })
    }

    fn is_shulker(&self, s: &ItemStack) -> bool {
        u32::try_from(s.item)
            .ok()
            .and_then(|i| self.data.item_name(i))
            .is_some_and(|n| n.ends_with("shulker_box"))
    }
}

fn same(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.components == b.components
}

fn split(stack: &mut ItemStack, n: i32) -> ItemStack {
    let n = n.min(stack.count).max(0);
    let mut out = stack.clone();
    out.count = n;
    stack.count -= n;
    if stack.count <= 0 {
        *stack = ItemStack::default();
    }
    out
}

fn with_count(stack: &ItemStack, n: i32) -> ItemStack {
    let mut out = stack.clone();
    out.count = n;
    if n <= 0 {
        return ItemStack::default();
    }
    out
}

pub struct Menu<'a> {
    pub rules: Rules<'a>,
    pub layout: Layout,
    pub slots: &'a mut Vec<ItemStack>,
    pub cursor: &'a mut ItemStack,
    pub offhand: &'a mut ItemStack,
    pub qc: &'a mut QuickCraft,
    /// Set when a touched slot's behaviour is unknown; the caller keeps its prior state.
    pub unknown: bool,
}

impl Menu<'_> {
    fn kind(&self, i: usize) -> SlotKind {
        self.layout.kind(i, self.slots.len())
    }

    fn may_place(&self, i: usize, s: &ItemStack) -> bool {
        match self.kind(i) {
            SlotKind::Normal => true,
            SlotKind::Armor(slot) => self.rules.equip_slot(s) == Some(slot),
            SlotKind::NoShulker => !self.rules.is_shulker(s),
            SlotKind::Unknown => false,
        }
    }

    fn slot_max(&self, i: usize, s: &ItemStack) -> i32 {
        let base = self.rules.max_stack(s);
        match self.kind(i) {
            SlotKind::Armor(_) => 1.min(base),
            _ => base.min(64),
        }
    }

    fn touch(&mut self, i: usize) -> bool {
        if self.kind(i) == SlotKind::Unknown {
            self.unknown = true;
            return false;
        }
        true
    }

    fn safe_insert(&mut self, i: usize, input: &mut ItemStack, amount: i32) {
        if input.is_empty() || !self.may_place(i, input) {
            return;
        }
        let max = self.slot_max(i, input);
        let n = amount.min(input.count).min(max - self.slots[i].count);
        if n <= 0 {
            return;
        }
        if self.slots[i].is_empty() {
            self.slots[i] = split(input, n);
        } else if same(&self.slots[i], input) {
            input.count -= n;
            self.slots[i].count += n;
            if input.count <= 0 {
                *input = ItemStack::default();
            }
        }
    }

    pub fn click(
        &mut self,
        slot: i16,
        button: i8,
        input: ContainerInput,
        player_hotbar: &dyn Fn(i8) -> Option<usize>,
    ) {
        let button = i32::from(button);
        let idx = usize::try_from(slot).ok().filter(|&i| i < self.slots.len());
        match input {
            ContainerInput::QuickCraft => self.quick_craft(idx, button),
            _ if self.qc.status != 0 => self.reset_qc(),
            ContainerInput::Pickup | ContainerInput::QuickMove if button == 0 || button == 1 => {
                self.pickup_or_move(slot, idx, button, input == ContainerInput::QuickMove)
            }
            ContainerInput::Swap if (0..9).contains(&button) || button == 40 => {
                if let Some(i) = idx {
                    self.swap(i, button, player_hotbar);
                }
            }
            ContainerInput::Clone if self.rules.creative && self.cursor.is_empty() => {
                if let Some(i) = idx
                    && !self.slots[i].is_empty()
                    && self.touch(i)
                {
                    let max = self.rules.max_stack(&self.slots[i]);
                    *self.cursor = with_count(&self.slots[i], max);
                }
            }
            ContainerInput::Throw if self.cursor.is_empty() => {
                if let Some(i) = idx {
                    self.throw(i, button);
                }
            }
            ContainerInput::PickupAll => {
                if let Some(i) = idx {
                    self.pickup_all(i, button);
                }
            }
            _ => {}
        }
    }

    fn reset_qc(&mut self) {
        self.qc.status = 0;
        self.qc.slots.clear();
    }

    fn pickup_or_move(&mut self, slot: i16, idx: Option<usize>, button: i32, quick: bool) {
        if slot == OUTSIDE {
            if !self.cursor.is_empty() {
                if button == 0 {
                    *self.cursor = ItemStack::default();
                } else {
                    split(self.cursor, 1);
                }
            }
            return;
        }
        let Some(i) = idx else { return };
        if !self.touch(i) {
            return;
        }
        if quick {
            self.quick_move_loop(i);
            return;
        }
        let clicked = self.slots[i].clone();
        let mut carried = std::mem::take(self.cursor);
        if clicked.is_empty() {
            if !carried.is_empty() {
                let amount = if button == 0 { carried.count } else { 1 };
                self.safe_insert(i, &mut carried, amount);
            }
        } else if carried.is_empty() {
            let amount = if button == 0 {
                clicked.count
            } else {
                (clicked.count + 1) / 2
            };
            carried = split(&mut self.slots[i], amount);
        } else if self.may_place(i, &carried) {
            if same(&clicked, &carried) {
                let amount = if button == 0 { carried.count } else { 1 };
                self.safe_insert(i, &mut carried, amount);
            } else if carried.count <= self.slot_max(i, &carried) {
                self.slots[i] = std::mem::replace(&mut carried, clicked);
            }
        } else if same(&clicked, &carried) {
            let room = self.rules.max_stack(&carried) - carried.count;
            let take = clicked.count.min(room);
            if take > 0 {
                let taken = split(&mut self.slots[i], take);
                carried.count += taken.count;
            }
        }
        *self.cursor = carried;
    }

    fn swap(&mut self, a: usize, button: i32, hotbar: &dyn Fn(i8) -> Option<usize>) {
        if !self.touch(a) {
            return;
        }
        let b = if button == 40 {
            (self.layout == Layout::Player).then_some(45)
        } else {
            match hotbar(button as i8).filter(|&b| b < self.slots.len()) {
                Some(b) => Some(b),
                None => return,
            }
        };
        if b == Some(a) {
            return;
        }
        let mut source = match b {
            Some(b) => self.slots[b].clone(),
            None => self.offhand.clone(),
        };
        let target = self.slots[a].clone();
        if source.is_empty() && target.is_empty() {
            return;
        }
        let set_source = |this: &mut Self, v: ItemStack| match b {
            Some(b) => this.slots[b] = v,
            None => *this.offhand = v,
        };
        if source.is_empty() {
            set_source(self, target);
            self.slots[a] = ItemStack::default();
        } else if target.is_empty() {
            if self.may_place(a, &source) {
                let max = self.slot_max(a, &source);
                if source.count > max {
                    let part = split(&mut source, max);
                    self.slots[a] = part;
                    set_source(self, source);
                } else {
                    set_source(self, ItemStack::default());
                    self.slots[a] = source;
                }
            }
        } else if self.may_place(a, &source) {
            let max = self.slot_max(a, &source);
            if source.count > max {
                let part = split(&mut source, max);
                self.slots[a] = part;
                set_source(self, source);
                // Inventory::add of the displaced stack is not predicted.
                self.unknown = true;
            } else {
                set_source(self, target);
                self.slots[a] = source;
            }
        }
    }

    fn throw(&mut self, i: usize, button: i32) {
        if !self.touch(i) {
            return;
        }
        for _ in 0..MAX_QUICK_MOVE_ROUNDS {
            let item = self.slots[i].clone();
            if item.is_empty() {
                return;
            }
            let amount = if button == 0 { 1 } else { item.count };
            split(&mut self.slots[i], amount);
            if button != 1 || !same(&self.slots[i], &item) {
                return;
            }
        }
    }

    fn pickup_all(&mut self, i: usize, button: i32) {
        if self.cursor.is_empty()
            || (!self.slots[i].is_empty() && self.kind(i) != SlotKind::Unknown)
        {
            return;
        }
        let n = self.slots.len();
        let max = self.rules.max_stack(self.cursor);
        for pass in 0..2 {
            let mut k = if button == 0 { 0 } else { n as isize - 1 };
            let step = if button == 0 { 1 } else { -1 };
            while k >= 0 && (k as usize) < n && self.cursor.count < max {
                let t = k as usize;
                k += step;
                if self.slots[t].is_empty() || !same(&self.slots[t], self.cursor) {
                    continue;
                }
                if self.kind(t) == SlotKind::Unknown {
                    self.unknown = true;
                    continue;
                }
                if pass == 0 && self.slots[t].count == self.rules.max_stack(&self.slots[t]) {
                    continue;
                }
                let take = self.slots[t].count.min(max - self.cursor.count);
                let taken = split(&mut self.slots[t], take);
                self.cursor.count += taken.count;
            }
        }
    }

    fn quick_craft(&mut self, idx: Option<usize>, button: i32) {
        let expected = self.qc.status;
        let header = (button & 3) as u8;
        self.qc.status = header;
        if ((expected != 1 || header != 2) && expected != header) || self.cursor.is_empty() {
            self.reset_qc();
        } else if header == 0 {
            let kind = ((button >> 2) & 3) as u8;
            if kind == 0 || kind == 1 || (kind == 2 && self.rules.creative) {
                self.qc.kind = kind;
                self.qc.status = 1;
                self.qc.slots.clear();
            } else {
                self.reset_qc();
            }
        } else if header == 1 {
            let Some(i) = idx else { return };
            if self.can_quick_replace(i, true)
                && self.may_place(i, self.cursor)
                && (self.qc.kind == 2 || self.cursor.count as usize > self.qc.slots.len())
                && !self.qc.slots.contains(&i)
                && self.qc.slots.len() < self.slots.len()
            {
                self.qc.slots.push(i);
            }
        } else if header == 2 {
            self.finish_quick_craft();
        } else {
            self.reset_qc();
        }
    }

    fn can_quick_replace(&self, i: usize, ignore_size: bool) -> bool {
        let s = &self.slots[i];
        if s.is_empty() {
            return self.kind(i) != SlotKind::Unknown;
        }
        same(s, self.cursor)
            && s.count + if ignore_size { 0 } else { self.cursor.count }
                <= self.rules.max_stack(self.cursor)
    }

    fn finish_quick_craft(&mut self) {
        let len = self.slots.len();
        let mut slots = std::mem::take(&mut self.qc.slots);
        slots.retain(|&i| i < len);
        let kind = self.qc.kind;
        self.reset_qc();
        if slots.is_empty() {
            return;
        }
        if slots.len() == 1 {
            self.qc.status = 0;
            let i = slots[0];
            self.pickup_or_move(i as i16, Some(i), i32::from(kind), false);
            return;
        }
        let source = self.cursor.clone();
        if source.is_empty() {
            return;
        }
        let mut remaining = self.cursor.count;
        for &i in &slots {
            if self.can_quick_replace(i, true)
                && self.may_place(i, self.cursor)
                && (kind == 2 || self.cursor.count as usize >= slots.len())
            {
                let carry = self.slots[i].count;
                let max = self.rules.max_stack(&source).min(self.slot_max(i, &source));
                let place = match kind {
                    0 => (source.count as f32 / slots.len() as f32).floor() as i32,
                    1 => 1,
                    2 => self.rules.max_stack(&source),
                    _ => source.count,
                };
                let new = (place + carry).min(max);
                remaining -= new - carry;
                self.slots[i] = with_count(&source, new);
            }
        }
        *self.cursor = with_count(&source, remaining);
    }

    fn quick_move_loop(&mut self, i: usize) {
        for _ in 0..MAX_QUICK_MOVE_ROUNDS {
            let moved = self.quick_move(i);
            if moved.is_empty() || self.slots[i].item != moved.item || self.slots[i].is_empty() {
                break;
            }
        }
    }

    fn quick_move(&mut self, i: usize) -> ItemStack {
        if self.slots[i].is_empty() {
            return ItemStack::default();
        }
        let n = self.slots.len();
        let original = self.slots[i].clone();
        let mut stack = original.clone();
        let ok = match self.layout {
            Layout::Player => self.quick_move_player(i, &mut stack),
            Layout::Crafting => {
                if i == 0 {
                    self.unknown = true;
                    false
                } else if (10..46).contains(&i) {
                    if self.move_to(&mut stack, 1, 10, false) {
                        true
                    } else if i < 37 {
                        self.move_to(&mut stack, 37, 46, false)
                    } else {
                        self.move_to(&mut stack, 10, 37, false)
                    }
                } else {
                    self.move_to(&mut stack, 10, 46, false)
                }
            }
            Layout::Simple { container, .. } => {
                if i < container {
                    self.move_to(&mut stack, container, n, true)
                } else {
                    self.move_to(&mut stack, 0, container, false)
                }
            }
            Layout::Opaque => {
                self.unknown = true;
                false
            }
        };
        if self.unknown && !ok {
            return ItemStack::default();
        }
        if !ok {
            return ItemStack::default();
        }
        self.slots[i] = if stack.is_empty() {
            ItemStack::default()
        } else {
            stack.clone()
        };
        if matches!(self.layout, Layout::Player | Layout::Crafting) && stack.count == original.count
        {
            return ItemStack::default();
        }
        original
    }

    fn quick_move_player(&mut self, i: usize, stack: &mut ItemStack) -> bool {
        let eq = self.rules.equip_slot(stack);
        match i {
            0 => {
                self.unknown = true;
                false
            }
            1..=8 => self.move_to(stack, 9, 45, false),
            _ => {
                let armor_pos = match eq {
                    Some(EquipmentSlot::Head) => Some(5),
                    Some(EquipmentSlot::Chest) => Some(6),
                    Some(EquipmentSlot::Legs) => Some(7),
                    Some(EquipmentSlot::Feet) => Some(8),
                    _ => None,
                };
                if let Some(pos) = armor_pos.filter(|&p| self.slots[p].is_empty()) {
                    self.move_to(stack, pos, pos + 1, false)
                } else if eq == Some(EquipmentSlot::OffHand) && self.slots[45].is_empty() {
                    self.move_to(stack, 45, 46, false)
                } else if (9..36).contains(&i) {
                    self.move_to(stack, 36, 45, false)
                } else if (36..45).contains(&i) {
                    self.move_to(stack, 9, 36, false)
                } else {
                    self.move_to(stack, 9, 45, false)
                }
            }
        }
    }

    fn move_to(
        &mut self,
        stack: &mut ItemStack,
        start: usize,
        end: usize,
        backwards: bool,
    ) -> bool {
        let end = end.min(self.slots.len());
        let mut changed = false;
        let order: Vec<usize> = if backwards {
            (start..end).rev().collect()
        } else {
            (start..end).collect()
        };
        if self.rules.max_stack(stack) > 1 {
            for &d in &order {
                if stack.is_empty() {
                    break;
                }
                let target = &self.slots[d];
                if target.is_empty() || !same(stack, target) {
                    continue;
                }
                let max = self.slot_max(d, target);
                let total = target.count + stack.count;
                if total <= max {
                    stack.count = 0;
                    self.slots[d].count = total;
                    changed = true;
                } else if target.count < max {
                    stack.count -= max - target.count;
                    self.slots[d].count = max;
                    changed = true;
                }
            }
            if stack.count <= 0 {
                *stack = ItemStack::default();
            }
        }
        if !stack.is_empty() {
            for &d in &order {
                if self.kind(d) == SlotKind::Unknown && self.slots[d].is_empty() {
                    self.unknown = true;
                }
                if self.slots[d].is_empty() && self.may_place(d, stack) {
                    let max = self.slot_max(d, stack);
                    let part = split(stack, max);
                    self.slots[d] = part;
                    changed = true;
                    break;
                }
            }
        }
        changed
    }
}
