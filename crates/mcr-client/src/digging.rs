//! Vanilla destroy progress and block-state prediction bookkeeping.

use mcr_buf::BlockPos;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, HolderSet};
use mcr_world::BlockState;
use rustc_hash::FxHashMap;

use crate::state::ClientState;

const MAX_PREDICTIONS: usize = 4096;
const TOOL_COMPONENT: i32 = 28;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct BlockPredictions {
    pending: FxHashMap<BlockPos, (i32, u16)>,
}

impl BlockPredictions {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn retain(&mut self, pos: BlockPos, sequence: i32, server_state: u16) {
        if let Some(entry) = self.pending.get_mut(&pos) {
            entry.0 = sequence;
        } else if self.pending.len() < MAX_PREDICTIONS {
            self.pending.insert(pos, (sequence, server_state));
        }
    }

    /// A server update for a predicted position is held back until its sequence is acknowledged.
    pub(crate) fn hold(&mut self, pos: BlockPos, state: u16) -> bool {
        match self.pending.get_mut(&pos) {
            Some(entry) => {
                entry.1 = state;
                true
            }
            None => false,
        }
    }

    pub(crate) fn end_up_to(&mut self, sequence: i32) -> Vec<(BlockPos, u16)> {
        let done: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, (seq, _))| *seq <= sequence)
            .map(|(p, (_, s))| (*p, *s))
            .collect();
        for (p, _) in &done {
            self.pending.remove(p);
        }
        done
    }
}

struct Rule {
    matches: Matcher,
    speed: Option<f32>,
    correct: Option<bool>,
}

enum Matcher {
    Tag(String),
    Block(u32),
    Ids(Vec<i32>),
}

struct ToolView {
    default_speed: f32,
    rules: Vec<Rule>,
}

impl ClientState {
    fn block_is(&self, block: u32, m: &Matcher) -> bool {
        match m {
            Matcher::Tag(t) => self
                .registries
                .is_in_tag("minecraft:block", t, block as i32),
            Matcher::Block(b) => *b == block,
            Matcher::Ids(ids) => ids.contains(&(block as i32)),
        }
    }

    fn tool_view(&self, stack: &ItemStack) -> Option<ToolView> {
        if stack.is_empty() {
            return None;
        }
        for c in &stack.components.added {
            if let DataComponent::Tool(t) = c {
                return Some(ToolView {
                    default_speed: t.default_mining_speed,
                    rules: t
                        .rules
                        .iter()
                        .take(64)
                        .map(|r| Rule {
                            matches: match &r.blocks {
                                HolderSet::Tag(id) => Matcher::Tag(id.as_str().to_owned()),
                                HolderSet::Ids(ids) => Matcher::Ids(ids.clone()),
                            },
                            speed: r.speed,
                            correct: r.correct_for_drops,
                        })
                        .collect(),
                });
            }
        }
        if stack.components.removed.contains(&TOOL_COMPONENT) {
            return None;
        }
        let tool = self.data.item_tool(u32::try_from(stack.item).ok()?)?;
        Some(ToolView {
            default_speed: tool.default_mining_speed,
            rules: tool
                .rules()
                .map(|r| Rule {
                    matches: match r.blocks.strip_prefix('#') {
                        Some(tag) => Matcher::Tag(tag.to_owned()),
                        None => match self.data.block_by_name(r.blocks) {
                            Some(b) => Matcher::Block(b),
                            None => Matcher::Ids(Vec::new()),
                        },
                    },
                    speed: r.speed,
                    correct: r.correct_for_drops,
                })
                .collect(),
        })
    }

    pub fn held_stack(&self) -> &ItemStack {
        self.inventory.hotbar(self.player.held_slot)
    }

    fn attribute(&self, name: &str, default: f64) -> f64 {
        let id = self
            .data
            .registry("minecraft:attribute")
            .and_then(|r| r.id_of(&format!("minecraft:{name}")));
        id.and_then(|id| self.player.attributes.get(&(id as i32)))
            .map_or(default, |a| a.value(None))
    }

    fn effect_amplifier(&self, name: &str) -> Option<i32> {
        let id = self
            .data
            .registry("minecraft:mob_effect")?
            .id_of(&format!("minecraft:{name}"))?;
        self.player.effects.get(&(id as i32)).map(|e| e.amplifier)
    }

    /// Vanilla `BlockState#getDestroyProgress` for the player and held item; `None` if there is no block.
    pub fn destroy_progress(&self, pos: BlockPos) -> Option<f32> {
        let state = self.chunks.block(pos.x, pos.y, pos.z)?;
        let info = self.data.block_state(u32::from(state.0))?;
        if info.is_air() {
            return None;
        }
        let block = self.data.block_info(info.block)?;
        if block.destroy_time == -1.0 {
            return Some(0.0);
        }
        let tool = self.tool_view(self.held_stack());
        let mut speed = 1.0f32;
        let mut correct = false;
        if let Some(t) = &tool {
            speed = t.default_speed;
            if let Some(s) = t
                .rules
                .iter()
                .find(|r| r.speed.is_some() && self.block_is(info.block, &r.matches))
            {
                speed = s.speed.unwrap_or(speed);
            }
            correct = t
                .rules
                .iter()
                .find(|r| r.correct.is_some() && self.block_is(info.block, &r.matches))
                .and_then(|r| r.correct)
                .unwrap_or(false);
        }
        if speed > 1.0 {
            speed += self.attribute("mining_efficiency", 0.0) as f32;
        }
        let haste = [
            self.effect_amplifier("haste"),
            self.effect_amplifier("conduit_power"),
        ]
        .into_iter()
        .flatten()
        .max();
        if let Some(amp) = haste {
            speed *= 1.0 + (amp + 1) as f32 * 0.2;
        }
        if let Some(amp) = self.effect_amplifier("mining_fatigue") {
            speed *= 0.3f64.powi(amp.clamp(0, 255) + 1) as f32;
        }
        speed *= self.attribute("block_break_speed", 1.0) as f32;
        if self.player.physics.eye_in_water() {
            speed *= self.attribute("submerged_mining_speed", 0.2) as f32;
        }
        if !self.player.physics.on_ground {
            speed /= 5.0;
        }
        let modifier = if !block.requires_correct_tool || correct {
            30.0
        } else {
            100.0
        };
        Some(speed / block.destroy_time / modifier)
    }

    /// What breaking the block leaves behind: its fluid, or air.
    pub(crate) fn state_after_break(&self, state: BlockState) -> BlockState {
        let fluid = self
            .data
            .block_state(u32::from(state.0))
            .and_then(|i| i.fluid)
            .filter(|f| f.source);
        let name = fluid.and_then(|f| self.data.fluid_name(f.fluid));
        name.and_then(|n| self.data.block_by_name(n))
            .and_then(|b| self.data.block_info(b))
            .map_or(BlockState::AIR, |i| BlockState(i.default_state as u16))
    }

    /// Client-side `setBlock` under a prediction sequence; the previous state is restored on ack
    /// unless the server sent its own.
    pub(crate) fn predict_block(&mut self, pos: BlockPos, new: BlockState, sequence: i32) {
        let Some(old) = self.chunks.block(pos.x, pos.y, pos.z) else {
            return;
        };
        self.predictions.retain(pos, sequence, old.0);
        self.chunks.set_block(pos.x, pos.y, pos.z, new);
    }

    pub(crate) fn finish_predictions(&mut self, sequence: i32) -> Vec<(BlockPos, u16)> {
        let done = self.predictions.end_up_to(sequence);
        for (pos, state) in &done {
            self.chunks
                .set_block(pos.x, pos.y, pos.z, BlockState(*state));
        }
        done
    }
}
