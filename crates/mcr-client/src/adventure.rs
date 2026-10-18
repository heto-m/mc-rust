//! Adventure mode: `Player#blockActionRestricted` and the `can_break` / `can_place_on` predicates.

use mcr_buf::BlockPos;
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

    /// `BlockPredicate#matches`. Block entity NBT and component checks are left to the server.
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
        p.nbt.is_none() || self.block_entities.contains_key(&pos)
    }
}
