//! Bundle contents weight and display rules (`BundleContents`, `BundleItem`).

use mcr_protocol::types::component::{DataComponent, DataComponentPatch, component_type_id};
use mcr_protocol::types::item::{ItemStack, ItemStackTemplate};

const MAX_NESTING: u8 = 8;
const FULL_BAR_COLOR: u32 = 0xFFFF_5454;
const BAR_COLOR: u32 = 0xFF70_87FF;

/// An exact fraction like Apache Commons' `Fraction`, always reduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundleWeight {
    num: u64,
    den: u64,
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl BundleWeight {
    pub const ZERO: Self = Self { num: 0, den: 1 };
    pub const ONE: Self = Self { num: 1, den: 1 };

    pub fn new(num: u64, den: u64) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let g = gcd(num, den).max(1);
        Some(Self {
            num: num / g,
            den: den / g,
        })
    }

    /// Vanilla throws on `int` overflow and treats the bundle as an error, so overflow is `None` here too.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let g = gcd(self.den, other.den).max(1);
        let den = (self.den / g).checked_mul(other.den)?;
        let num = self
            .num
            .checked_mul(other.den / g)?
            .checked_add(other.num.checked_mul(self.den / g)?)?;
        let w = Self::new(num, den)?;
        (w.num <= i32::MAX as u64 && w.den <= i32::MAX as u64).then_some(w)
    }

    pub fn times(self, n: u64) -> Option<Self> {
        let g = gcd(n, self.den).max(1);
        let w = Self::new(self.num.checked_mul(n / g)?, self.den / g)?;
        (w.num <= i32::MAX as u64).then_some(w)
    }

    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    pub fn is_full(self) -> bool {
        self.num >= self.den
    }

    /// `Mth.mulAndTruncate`.
    pub fn mul_truncate(self, n: u64) -> u64 {
        self.num.saturating_mul(n) / self.den
    }

    /// How many more of an item weighing `item` fit.
    pub fn room_for(self, item: Self) -> u64 {
        if self.is_full() || item.is_zero() {
            return 0;
        }
        let (rem_num, rem_den) = (self.den - self.num, self.den);
        (rem_num.saturating_mul(item.den)) / rem_den.saturating_mul(item.num).max(1)
    }
}

fn get<'a>(patch: &'a DataComponentPatch, name: &str) -> Option<&'a DataComponent> {
    patch.get(component_type_id(name)?)
}

pub fn bundle_contents(patch: &DataComponentPatch) -> Option<&[ItemStackTemplate]> {
    match get(patch, "bundle_contents") {
        Some(DataComponent::BundleContents(items)) => Some(items),
        _ => None,
    }
}

/// The contents of a bundle stack; `None` for anything without `bundle_contents`.
pub fn bundle_items(stack: &ItemStack) -> Option<&[ItemStackTemplate]> {
    bundle_contents(&stack.components).filter(|_| is_bundle(stack))
}

/// `#minecraft:bundles` membership by name: the plain bundle and its sixteen dyed variants.
pub fn is_bundle(stack: &ItemStack) -> bool {
    !stack.is_empty()
        && u32::try_from(stack.item)
            .ok()
            .and_then(|id| mcr_data::v26_3().item_name(id))
            .is_some_and(|n| n == "minecraft:bundle" || n.ends_with("_bundle"))
}

fn max_stack(item: i32, patch: &DataComponentPatch) -> u64 {
    match get(patch, "max_stack_size") {
        Some(DataComponent::MaxStackSize(v)) => v.0.clamp(1, 99) as u64,
        _ => u32::try_from(item)
            .ok()
            .and_then(|id| mcr_data::v26_3().item_info(id))
            .map_or(64, |i| u64::from(i.max_stack_size.max(1))),
    }
}

/// `BundleContents#getWeight` of one item.
pub fn item_weight(item: i32, patch: &DataComponentPatch) -> Option<BundleWeight> {
    item_weight_at(item, patch, 0)
}

fn item_weight_at(item: i32, patch: &DataComponentPatch, depth: u8) -> Option<BundleWeight> {
    if let Some(nested) = bundle_contents(patch) {
        if depth >= MAX_NESTING {
            return None;
        }
        return weight_at(nested, depth + 1)?.checked_add(BundleWeight::new(1, 16)?);
    }
    if let Some(DataComponent::Bees(bees)) = get(patch, "bees")
        && !bees.is_empty()
    {
        return Some(BundleWeight::ONE);
    }
    BundleWeight::new(1, max_stack(item, patch))
}

fn weight_at(items: &[ItemStackTemplate], depth: u8) -> Option<BundleWeight> {
    let mut total = BundleWeight::ZERO;
    for t in items {
        let w = item_weight_at(t.item, &t.components, depth)?;
        total = total.checked_add(w.times(u64::try_from(t.count.max(0)).ok()?)?)?;
    }
    Some(total)
}

/// Total weight of a bundle's contents; `None` when vanilla's computation would fail.
pub fn bundle_weight(items: &[ItemStackTemplate]) -> Option<BundleWeight> {
    weight_at(items, 0)
}

/// `BundleItem#getWeightSafe`: a failed weight counts as full.
pub fn weight_safe(items: &[ItemStackTemplate]) -> BundleWeight {
    bundle_weight(items).unwrap_or(BundleWeight::ONE)
}

/// `BundleContents#getNumberOfItemsToShow`.
pub fn items_to_show(stacks: usize) -> usize {
    let available = if stacks > 12 { 11 } else { 12 };
    let partial = stacks % 4;
    let empty = if partial == 0 { 0 } else { 4 - partial };
    stacks.min(available - empty)
}

/// `BundleItem#isBarVisible`, `getBarWidth` and `getBarColor`.
pub fn bundle_bar(stack: &ItemStack) -> Option<(i32, u32)> {
    let weight = weight_safe(bundle_items(stack)?);
    if weight.is_zero() {
        return None;
    }
    let width = (1 + weight.mul_truncate(12)).min(13) as i32;
    let color = if weight.is_full() {
        FULL_BAR_COLOR
    } else {
        BAR_COLOR
    };
    Some((width, color))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> i32 {
        mcr_data::v26_3().item_by_name(name).unwrap() as i32
    }

    fn template(name: &str, count: i32) -> ItemStackTemplate {
        ItemStackTemplate {
            item: item(name),
            count,
            components: DataComponentPatch::default(),
        }
    }

    fn bundle(items: Vec<ItemStackTemplate>) -> ItemStack {
        let mut s = ItemStack::new(item("minecraft:bundle"), 1);
        s.components
            .added
            .push(DataComponent::BundleContents(items));
        s
    }

    #[test]
    fn weights_follow_max_stack_size_and_nesting() {
        let w = bundle_weight(&[template("minecraft:stone", 32)]).unwrap();
        assert_eq!(w, BundleWeight::new(1, 2).unwrap());
        let w = bundle_weight(&[template("minecraft:diamond_sword", 1)]).unwrap();
        assert!(w.is_full());
        let w = bundle_weight(&[template("minecraft:ender_pearl", 4)]).unwrap();
        assert_eq!(w, BundleWeight::new(1, 4).unwrap());
        let mut nested = template("minecraft:bundle", 1);
        nested
            .components
            .added
            .push(DataComponent::BundleContents(vec![template(
                "minecraft:stone",
                16,
            )]));
        let w = bundle_weight(&[nested]).unwrap();
        assert_eq!(
            w,
            BundleWeight::new(1, 4)
                .unwrap()
                .checked_add(BundleWeight::new(1, 16).unwrap())
                .unwrap()
        );
        assert_eq!(
            BundleWeight::new(1, 2)
                .unwrap()
                .room_for(BundleWeight::new(1, 64).unwrap()),
            32
        );
    }

    #[test]
    fn bar_width_and_colour_like_vanilla() {
        assert_eq!(bundle_bar(&bundle(Vec::new())), None);
        let (w, c) = bundle_bar(&bundle(vec![template("minecraft:stone", 32)])).unwrap();
        assert_eq!((w, c), (7, BAR_COLOR));
        let (w, c) = bundle_bar(&bundle(vec![template("minecraft:stone", 64)])).unwrap();
        assert_eq!((w, c), (13, FULL_BAR_COLOR));
        assert_eq!(
            bundle_bar(&ItemStack::new(item("minecraft:stone"), 1)),
            None
        );
    }

    #[test]
    fn items_to_show_leaves_room_for_the_overflow_count() {
        assert_eq!(items_to_show(0), 0);
        assert_eq!(items_to_show(3), 3);
        assert_eq!(items_to_show(12), 12);
        assert_eq!(items_to_show(13), 8);
        assert_eq!(items_to_show(16), 11);
        assert_eq!(items_to_show(64), 11);
    }

    #[test]
    fn hostile_counts_do_not_overflow() {
        assert_eq!(
            bundle_weight(&[template("minecraft:stone", i32::MAX)]).map(BundleWeight::is_full),
            Some(true)
        );
        let many: Vec<_> = (0..1000)
            .map(|_| template("minecraft:diamond_sword", i32::MAX))
            .collect();
        assert_eq!(bundle_weight(&many), None);
        assert!(weight_safe(&many).is_full());
    }
}
