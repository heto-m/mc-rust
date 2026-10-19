//! Client-side container click simulation mirroring vanilla `AbstractContainerMenu#clicked`.

mod layouts;
mod rules;

use mcr_protocol::play::serverbound::ContainerInput;
use mcr_protocol::types::ItemStack;
use rustc_hash::FxHashMap;

pub use layouts::{Layout, MountInfo, ResultKind, SlotKind};
pub use rules::Rules;

use crate::inventory::OUTSIDE_SLOT as OUTSIDE;
const MAX_QUICK_MOVE_ROUNDS: usize = 4608;
const CONTAINER_MAX_STACK: i32 = 99;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuickCraft {
    pub status: u8,
    pub kind: u8,
    pub slots: Vec<usize>,
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

fn shrink(stack: &mut ItemStack, n: i32) {
    stack.count -= n;
    if stack.count <= 0 {
        *stack = ItemStack::default();
    }
}

pub struct Menu<'a> {
    pub rules: Rules<'a>,
    pub layout: Layout,
    pub slots: &'a mut Vec<ItemStack>,
    pub cursor: &'a mut ItemStack,
    pub offhand: &'a mut ItemStack,
    pub qc: &'a mut QuickCraft,
    pub data: &'a FxHashMap<i16, i16>,
    /// Set when a touched slot's behaviour is unknown; the caller keeps its prior state.
    pub unknown: bool,
}

impl Menu<'_> {
    fn kind(&self, i: usize) -> SlotKind {
        self.layout.kind(i, self.slots.len())
    }

    fn may_place(&self, i: usize, s: &ItemStack) -> bool {
        self.layout
            .may_place(&self.rules, self.data, self.kind(i), s)
    }

    fn may_pickup(&self, i: usize) -> bool {
        self.layout
            .may_pickup(&self.rules, self.kind(i), &self.slots[i])
    }

    fn slot_max(&self, i: usize, s: &ItemStack) -> i32 {
        let base = self.rules.max_stack(s).min(CONTAINER_MAX_STACK);
        match self.kind(i) {
            SlotKind::Armor(_)
            | SlotKind::MountArmor(_)
            | SlotKind::Beacon
            | SlotKind::BrewPotion => 1.min(base),
            SlotKind::EnchantItem => 1.min(base),
            SlotKind::Fuel if self.rules.is_bucket(s) => 1.min(base),
            _ => base,
        }
    }

    fn touch(&mut self, i: usize) -> bool {
        if matches!(
            self.kind(i),
            SlotKind::Unknown | SlotKind::Result(ResultKind::Unpredicted)
        ) {
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

    /// `Slot#tryRemove`: result and other non-placeable slots only give up their whole stack at once.
    fn try_remove(&mut self, i: usize, amount: i32, max: i32) -> Option<ItemStack> {
        if !self.may_pickup(i) {
            return None;
        }
        let allow_modification = self.may_place(i, &self.slots[i].clone());
        if !allow_modification && max < self.slots[i].count {
            return None;
        }
        let taken = split(&mut self.slots[i], amount.min(max));
        (!taken.is_empty()).then_some(taken)
    }

    /// `Slot#onTake`: what leaving a result slot does to the rest of the menu.
    fn on_take(&mut self, i: usize) {
        let SlotKind::Result(kind) = self.kind(i) else {
            return;
        };
        match kind {
            ResultKind::Crafting { grid, width } => self.take_crafting(grid, width),
            ResultKind::Furnace => {}
            ResultKind::Smithing => (0..3).for_each(|s| shrink(&mut self.slots[s], 1)),
            ResultKind::Grindstone => (0..2).for_each(|s| self.slots[s] = ItemStack::default()),
            ResultKind::Stonecutter => shrink(&mut self.slots[0], 1),
            ResultKind::Loom => (0..2).for_each(|s| shrink(&mut self.slots[s], 1)),
            ResultKind::Cartography => (0..2).for_each(|s| shrink(&mut self.slots[s], 1)),
            ResultKind::Unpredicted => self.unknown = true,
        }
    }

    fn take_crafting(&mut self, grid: std::ops::Range<usize>, _width: usize) {
        for s in grid {
            let item = self.slots[s].clone();
            if item.is_empty() {
                continue;
            }
            let remainder = self.rules.craft_remainder(&item);
            shrink(&mut self.slots[s], 1);
            let Some(replacement) = remainder else {
                continue;
            };
            if self.slots[s].is_empty() {
                self.slots[s] = replacement;
            } else if same(&self.slots[s], &replacement) {
                let mut merged = replacement;
                merged.count += self.slots[s].count;
                self.slots[s] = merged;
            } else {
                self.unknown = true;
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
            if self.may_pickup(i) {
                self.quick_move_loop(i);
            }
            return;
        }
        let clicked = self.slots[i].clone();
        let mut carried = std::mem::take(self.cursor);
        let bundle_action = |bundle: &ItemStack, other: &ItemStack| {
            self.rules.is_bundle(bundle) && (button == 1 || !other.is_empty())
        };
        if bundle_action(&carried, &clicked) || bundle_action(&clicked, &carried) {
            self.unknown = true;
            *self.cursor = carried;
            return;
        }
        if clicked.is_empty() {
            if !carried.is_empty() {
                let amount = if button == 0 { carried.count } else { 1 };
                self.safe_insert(i, &mut carried, amount);
            }
        } else if self.may_pickup(i) {
            if carried.is_empty() {
                let amount = if button == 0 {
                    clicked.count
                } else {
                    (clicked.count + 1) / 2
                };
                if let Some(taken) = self.try_remove(i, amount, i32::MAX) {
                    carried = taken;
                    self.on_take(i);
                }
            } else if self.may_place(i, &carried) {
                if same(&clicked, &carried) {
                    let amount = if button == 0 { carried.count } else { 1 };
                    self.safe_insert(i, &mut carried, amount);
                } else if carried.count <= self.slot_max(i, &carried) {
                    self.slots[i] = std::mem::replace(&mut carried, clicked);
                }
            } else if same(&clicked, &carried) {
                let room = self.rules.max_stack(&carried) - carried.count;
                if let Some(taken) = self.try_remove(i, clicked.count, room) {
                    carried.count += taken.count;
                    self.on_take(i);
                }
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
            if self.may_pickup(a) {
                set_source(self, target);
                self.slots[a] = ItemStack::default();
                self.on_take(a);
            }
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
        } else if self.may_pickup(a) && self.may_place(a, &source) {
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
                self.on_take(a);
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
            if self.try_remove(i, amount, i32::MAX).is_none() {
                return;
            }
            self.on_take(i);
            if button != 1 || !same(&self.slots[i], &item) {
                return;
            }
        }
    }

    fn pickup_all(&mut self, i: usize, button: i32) {
        if self.cursor.is_empty()
            || (!self.slots[i].is_empty()
                && self.kind(i) != SlotKind::Unknown
                && self.may_pickup(i))
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
                if matches!(
                    self.kind(t),
                    SlotKind::Unknown | SlotKind::Result(ResultKind::Unpredicted)
                ) {
                    self.unknown = true;
                    continue;
                }
                if !self.may_pickup(t) || !self.layout.can_take_for_pickup_all(self.kind(t)) {
                    continue;
                }
                if pass == 0 && self.slots[t].count == self.rules.max_stack(&self.slots[t]) {
                    continue;
                }
                let take = self.slots[t].count.min(max - self.cursor.count);
                if let Some(taken) = self.try_remove(t, take, max - self.cursor.count) {
                    self.cursor.count += taken.count;
                    self.on_take(t);
                }
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
            return !matches!(
                self.kind(i),
                SlotKind::Unknown | SlotKind::Result(ResultKind::Unpredicted)
            );
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

    /// `AbstractContainerMenu#moveItemStackTo` over `start..end`.
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
                if matches!(
                    self.kind(d),
                    SlotKind::Unknown | SlotKind::Result(ResultKind::Unpredicted)
                ) && self.slots[d].is_empty()
                {
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
