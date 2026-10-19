//! Moving piston blocks. Vanilla's client runs `PistonBaseBlock#triggerEvent` itself and creates the
//! `PistonMovingBlockEntity` at every destination; the server sends no data for them. Here the block
//! event is remembered and each `moving_piston` the server then places is matched to it, its moved
//! block taken from the world as it was before the move.

use mcr_buf::BlockPos;
use mcr_data::Version;
use rustc_hash::FxHashMap;

use crate::events::ClientEvent;

const MAX_PENDING: usize = 16;
const MAX_LOGGED: usize = 1024;
/// Client ticks a block event waits for the server's block updates of the move.
const PENDING_TICKS: u8 = 4;
pub const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const STEPS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pending {
    pos: BlockPos,
    facing: u8,
    extending: bool,
    sticky: bool,
    age: u8,
}

#[derive(Debug, Clone, Copy)]
struct Ids {
    piston: u32,
    sticky: u32,
    head: u32,
    moving: u32,
}

impl Ids {
    fn resolve(data: &Version) -> Option<Self> {
        let block = |name: &str| data.block_by_name(name);
        Some(Self {
            piston: block("minecraft:piston")?,
            sticky: block("minecraft:sticky_piston")?,
            head: block("minecraft:piston_head")?,
            moving: block("minecraft:moving_piston")?,
        })
    }
}

/// One moving block: what [`ClientEvent::PistonMoving`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PistonMove {
    pub pos: BlockPos,
    pub moved_state: u32,
    pub facing: u8,
    pub extending: bool,
    pub source: bool,
}

impl PistonMove {
    pub fn event(self) -> ClientEvent {
        ClientEvent::PistonMoving {
            pos: self.pos,
            moved_state: self.moved_state,
            facing: self.facing,
            extending: self.extending,
            source: self.source,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PistonTracker {
    pending: Vec<Pending>,
    /// The first state each position had since the oldest pending move began.
    before: FxHashMap<BlockPos, u32>,
    ids: Option<Ids>,
}

fn step(pos: BlockPos, facing: u8, by: i32) -> BlockPos {
    let s = STEPS[usize::from(facing.min(5))];
    BlockPos::new(
        pos.x.wrapping_add(s[0] * by),
        pos.y.wrapping_add(s[1] * by),
        pos.z.wrapping_add(s[2] * by),
    )
}

fn facing_of(data: &Version, state: u32) -> Option<u8> {
    let (_, value) = data.state_properties(state).find(|(k, _)| *k == "facing")?;
    DIRECTIONS.iter().position(|d| *d == value).map(|i| i as u8)
}

impl PistonTracker {
    pub(crate) fn active(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.before.clear();
    }

    pub(crate) fn tick(&mut self) {
        self.pending.retain_mut(|p| {
            p.age += 1;
            p.age < PENDING_TICKS
        });
        if self.pending.is_empty() {
            self.before.clear();
        }
    }

    /// A block event: `b0` 0 extends, 1 and 2 retract; `b1` is the piston's facing.
    pub(crate) fn block_event(
        &mut self,
        data: &Version,
        pos: BlockPos,
        block: i32,
        b0: u8,
        b1: u8,
    ) {
        let ids = match self.ids {
            Some(ids) => ids,
            None => match Ids::resolve(data) {
                Some(ids) => *self.ids.insert(ids),
                None => return,
            },
        };
        let Ok(block) = u32::try_from(block) else {
            return;
        };
        if (block != ids.piston && block != ids.sticky) || b0 > 2 || b1 & 7 > 5 {
            return;
        }
        if self.pending.len() >= MAX_PENDING {
            self.pending.remove(0);
        }
        self.pending.push(Pending {
            pos,
            facing: b1 & 7,
            extending: b0 == 0,
            sticky: block == ids.sticky,
            age: 0,
        });
    }

    /// Before the server changes `pos`: keeps its pre-move state while a move is pending.
    pub(crate) fn note_old(&mut self, pos: BlockPos, old: u32) {
        if self.before.len() < MAX_LOGGED || self.before.contains_key(&pos) {
            self.before.entry(pos).or_insert(old);
        }
    }

    /// After the server set `pos` to `state`: the moving block it starts, if it is one of ours.
    pub(crate) fn resolve(
        &self,
        data: &Version,
        pos: BlockPos,
        state: u32,
        current: impl Fn(BlockPos) -> Option<u32>,
    ) -> Option<PistonMove> {
        let ids = self.ids?;
        if data.block_state(state)?.block != ids.moving {
            return None;
        }
        let facing = facing_of(data, state)?;
        let moved = |at: BlockPos| {
            let usable = |s: &u32| {
                data.block_state(*s)
                    .is_some_and(|i| !i.is_air() && i.block != ids.moving)
            };
            self.before
                .get(&at)
                .copied()
                .filter(usable)
                .or_else(|| current(at).filter(usable))
        };
        for p in self.pending.iter().rev().filter(|p| p.facing == facing) {
            let piston = |block: u32| {
                let base = data.block_info(block)?.default_state;
                data.with_property(base, "facing", DIRECTIONS[usize::from(facing)])
            };
            let (moved_state, source) = if p.extending && pos == step(p.pos, facing, 1) {
                let head = piston(ids.head)?;
                let kind = if p.sticky { "sticky" } else { "normal" };
                (data.with_property(head, "type", kind)?, true)
            } else if !p.extending && pos == p.pos {
                let base = piston(if p.sticky { ids.sticky } else { ids.piston })?;
                (base, true)
            } else {
                let from = step(pos, facing, if p.extending { -1 } else { 1 });
                match moved(from) {
                    Some(s) => (s, false),
                    None => continue,
                }
            };
            return Some(PistonMove {
                pos,
                moved_state,
                facing,
                extending: p.extending,
                source,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, props: &[(&str, &str)]) -> u32 {
        let v = mcr_data::v26_3();
        let mut s = v
            .block_info(v.block_by_name(name).unwrap())
            .unwrap()
            .default_state;
        for (k, val) in props {
            s = v.with_property(s, k, val).unwrap();
        }
        s
    }

    fn block_id(name: &str) -> i32 {
        mcr_data::v26_3().block_by_name(name).unwrap() as i32
    }

    #[test]
    fn extension_moves_the_pushed_block_and_the_head() {
        let v = mcr_data::v26_3();
        let mut t = PistonTracker::default();
        let base = BlockPos::new(0, 64, 0);
        t.block_event(v, base, block_id("minecraft:piston"), 0, 5);
        assert!(t.active());
        let stone = state("minecraft:stone", &[]);
        let world = |p: BlockPos| (p == BlockPos::new(1, 64, 0)).then_some(stone);
        let moving = state("minecraft:moving_piston", &[("facing", "east")]);
        t.note_old(BlockPos::new(1, 64, 0), stone);
        let pushed = t
            .resolve(v, BlockPos::new(2, 64, 0), moving, world)
            .unwrap();
        assert_eq!(pushed.moved_state, stone);
        assert!(pushed.extending && !pushed.source);
        assert_eq!(pushed.facing, 5);
        let head = t
            .resolve(v, BlockPos::new(1, 64, 0), moving, world)
            .unwrap();
        assert!(head.source);
        assert_eq!(
            v.state_block_name(head.moved_state),
            Some("minecraft:piston_head")
        );
        let air = state("minecraft:air", &[]);
        assert_eq!(t.resolve(v, BlockPos::new(1, 64, 0), air, world), None);
        for _ in 0..PENDING_TICKS {
            t.tick();
        }
        assert!(!t.active());
        assert_eq!(t.resolve(v, BlockPos::new(2, 64, 0), moving, world), None);
    }

    #[test]
    fn sticky_retraction_pulls_the_block_back() {
        let v = mcr_data::v26_3();
        let mut t = PistonTracker::default();
        let base = BlockPos::new(0, 64, 0);
        t.block_event(v, base, block_id("minecraft:sticky_piston"), 1, 1);
        let slime = state("minecraft:slime_block", &[]);
        let moving = state("minecraft:moving_piston", &[("facing", "up")]);
        t.note_old(BlockPos::new(0, 66, 0), slime);
        let pulled = t
            .resolve(v, BlockPos::new(0, 65, 0), moving, |_| None)
            .unwrap();
        assert_eq!(pulled.moved_state, slime);
        assert!(!pulled.extending && !pulled.source);
        let source = t.resolve(v, base, moving, |_| None).unwrap();
        assert!(source.source);
        assert_eq!(
            v.state_block_name(source.moved_state),
            Some("minecraft:sticky_piston")
        );
    }

    #[test]
    fn hostile_block_events_stay_bounded() {
        let v = mcr_data::v26_3();
        let mut t = PistonTracker::default();
        for i in 0..10_000 {
            t.block_event(
                v,
                BlockPos::new(i, 0, 0),
                block_id("minecraft:piston"),
                0,
                7,
            );
            t.block_event(
                v,
                BlockPos::new(i, 0, 0),
                block_id("minecraft:piston"),
                9,
                0,
            );
            t.block_event(v, BlockPos::new(i, 0, 0), block_id("minecraft:stone"), 0, 0);
            t.block_event(
                v,
                BlockPos::new(i, 0, 0),
                block_id("minecraft:piston"),
                0,
                0,
            );
            t.note_old(BlockPos::new(i, 1, 0), 1);
        }
        assert!(t.pending.len() <= MAX_PENDING);
        assert!(t.before.len() <= MAX_LOGGED);
    }
}
