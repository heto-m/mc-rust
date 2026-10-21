//! Adventure mode: `Player#blockActionRestricted` and the `can_break` / `can_place_on` predicates.

use mcr_buf::BlockPos;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{
    AdventureModePredicate, BlockPredicate, DataComponent, HolderSet, ValueMatcher,
};
use mcr_protocol::types::game::GameType;

use crate::state::ClientState;

const MAX_PREDICATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdventureCheck {
    CanBreak,
    CanPlaceOn,
}

fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

impl ClientState {
    /// `Abilities#mayBuild` as `GameType#updatePlayerAbilities` sets it.
    pub(crate) fn may_build(&self) -> bool {
        !matches!(
            self.player.game_mode,
            GameType::Adventure | GameType::Spectator
        )
    }

    fn adventure_predicate<'s>(
        &self,
        stack: &'s ItemStack,
        check: AdventureCheck,
    ) -> Option<&'s AdventureModePredicate> {
        if stack.is_empty() {
            return None;
        }
        stack
            .components
            .added
            .iter()
            .find_map(|c| match (c, check) {
                (DataComponent::CanBreak(p), AdventureCheck::CanBreak)
                | (DataComponent::CanPlaceOn(p), AdventureCheck::CanPlaceOn) => Some(p),
                _ => None,
            })
    }

    /// `ItemStack#canBreakBlockInAdventureMode` and `canPlaceOnBlockInAdventureMode`.
    pub(crate) fn adventure_allows(
        &self,
        stack: &ItemStack,
        check: AdventureCheck,
        pos: BlockPos,
    ) -> bool {
        self.adventure_predicate(stack, check).is_some_and(|p| {
            p.0.iter()
                .take(MAX_PREDICATES)
                .any(|b| self.block_predicate_matches(b, pos))
        })
    }

    /// `Player#blockActionRestricted` for the main hand.
    pub(crate) fn block_action_restricted(&self, pos: BlockPos) -> bool {
        match self.player.game_mode {
            GameType::Spectator => true,
            GameType::Adventure => {
                !self.adventure_allows(self.held_stack(), AdventureCheck::CanBreak, pos)
            }
            _ => false,
        }
    }

    /// `BlockPredicate#matches(BlockInWorld)`: state and block entity NBT. Like vanilla's adventure
    /// check it ignores the component matchers.
    fn block_predicate_matches(&self, p: &BlockPredicate, pos: BlockPos) -> bool {
        let Some(state) = self.chunks.block(pos.x, pos.y, pos.z) else {
            return false;
        };
        let Some(info) = self.data.block_state(state.0) else {
            return false;
        };
        let block = info.block as i32;
        let in_set = match &p.blocks {
            None => true,
            Some(HolderSet::Ids(ids)) => ids.contains(&block),
            Some(HolderSet::Tag(tag)) => {
                self.registries
                    .is_in_tag("minecraft:block", tag.as_str(), block)
            }
        };
        if !in_set {
            return false;
        }
        if let Some(props) = &p.properties {
            for m in props.0.iter().take(MAX_PREDICATES) {
                let Some((_, value)) = self
                    .data
                    .state_properties(state.0)
                    .find(|(k, _)| *k == m.name)
                else {
                    return false;
                };
                let ok = match &m.matcher {
                    ValueMatcher::Exact(v) => value == v,
                    ValueMatcher::Ranged(r) => {
                        r.min
                            .as_deref()
                            .is_none_or(|min| compare(value, min).is_ge())
                            && r.max
                                .as_deref()
                                .is_none_or(|max| compare(value, max).is_le())
                    }
                };
                if !ok {
                    return false;
                }
            }
        }
        match &p.nbt {
            None => true,
            Some(expected) => self.block_entity_nbt_matches(&expected.0, pos),
        }
    }

    /// `NbtPredicate#matches` against `BlockEntity#saveWithFullMetadata` of the client's copy: the
    /// data the server sent plus the type id and position.
    fn block_entity_nbt_matches(&self, expected: &Compound, pos: BlockPos) -> bool {
        let Some((kind, data)) = self.block_entities.get(&pos) else {
            return false;
        };
        let mut actual = data.clone();
        if let Some(id) = u32::try_from(*kind).ok().and_then(|k| {
            self.data
                .registry("minecraft:block_entity_type")?
                .name_of(k)
        }) {
            actual.insert("id", Value::String(id.to_owned()));
        }
        actual.insert("x", Value::Int(pos.x));
        actual.insert("y", Value::Int(pos.y));
        actual.insert("z", Value::Int(pos.z));
        let mut budget = NBT_COMPARE_BUDGET;
        compare_compound(expected, &actual, &mut budget)
    }
}

/// Element comparisons one predicate may spend; partial list matching is quadratic.
const NBT_COMPARE_BUDGET: u32 = 1 << 16;

fn spend(budget: &mut u32) -> bool {
    *budget = budget.saturating_sub(1);
    *budget > 0
}

/// `NbtUtils#compareNbt` with partial list matching.
fn compare_value(expected: &Value, actual: &Value, budget: &mut u32) -> bool {
    if !spend(budget) {
        return false;
    }
    match (expected, actual) {
        (Value::Compound(e), Value::Compound(a)) => compare_compound(e, a, budget),
        (Value::List(e), Value::List(a)) => compare_list(e, a, budget),
        _ => expected == actual,
    }
}

fn compare_compound(expected: &Compound, actual: &Compound, budget: &mut u32) -> bool {
    actual.len() >= expected.len()
        && expected
            .iter()
            .all(|(k, v)| actual.get(k).is_some_and(|a| compare_value(v, a, budget)))
}

fn compare_list(expected: &List, actual: &List, budget: &mut u32) -> bool {
    fn each<T>(e: &[T], a: &[T], budget: &mut u32, eq: impl Fn(&T, &T, &mut u32) -> bool) -> bool {
        e.iter()
            .all(|x| a.iter().any(|y| spend(budget) && eq(x, y, budget)))
    }
    fn same<T: PartialEq>(x: &T, y: &T, _: &mut u32) -> bool {
        x == y
    }
    if expected.is_empty() {
        return actual.is_empty();
    }
    if actual.len() < expected.len() {
        return false;
    }
    match (expected, actual) {
        (List::Byte(e), List::Byte(a)) => each(e, a, budget, same),
        (List::Short(e), List::Short(a)) => each(e, a, budget, same),
        (List::Int(e), List::Int(a)) => each(e, a, budget, same),
        (List::Long(e), List::Long(a)) => each(e, a, budget, same),
        (List::Float(e), List::Float(a)) => each(e, a, budget, same),
        (List::Double(e), List::Double(a)) => each(e, a, budget, same),
        (List::String(e), List::String(a)) => each(e, a, budget, same),
        (List::ByteArray(e), List::ByteArray(a)) => each(e, a, budget, same),
        (List::IntArray(e), List::IntArray(a)) => each(e, a, budget, same),
        (List::LongArray(e), List::LongArray(a)) => each(e, a, budget, same),
        (List::Compound(e), List::Compound(a)) => each(e, a, budget, compare_compound),
        (List::List(e), List::List(a)) => each(e, a, budget, compare_list),
        _ => false,
    }
}
