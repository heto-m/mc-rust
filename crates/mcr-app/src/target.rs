use mcr_buf::BlockPos;
use mcr_client::Entities;
use mcr_data::Version;
use mcr_protocol::play::serverbound::Facing;
use mcr_world::ChunkStore;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockHit {
    pub pos: BlockPos,
    pub face: Facing,
    pub cursor: [f32; 3],
    pub inside: bool,
    pub state: u32,
    pub distance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Block(BlockHit),
    Entity { id: i32, distance: f64 },
}

/// Interaction range of `minecraft:block_interaction_range` and `entity_interaction_range`.
pub fn reach(creative: bool) -> f64 {
    if creative { 5.0 } else { 4.5 }
}

struct Slab {
    t_min: f64,
    axis: usize,
    positive: bool,
}

fn slab(origin: [f64; 3], dir: [f64; 3], min: [f64; 3], max: [f64; 3]) -> Option<Slab> {
    let (mut t_min, mut t_max) = (f64::NEG_INFINITY, f64::INFINITY);
    let (mut axis, mut positive) = (0, true);
    for a in 0..3 {
        if dir[a].abs() < 1.0e-12 {
            if origin[a] < min[a] || origin[a] > max[a] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[a];
        let (mut t0, mut t1) = ((min[a] - origin[a]) * inv, (max[a] - origin[a]) * inv);
        let entering_positive = dir[a] > 0.0;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        if t0 > t_min {
            t_min = t0;
            axis = a;
            positive = entering_positive;
        }
        t_max = t_max.min(t1);
        if t_min > t_max {
            return None;
        }
    }
    (t_max >= 0.0).then_some(Slab {
        t_min,
        axis,
        positive,
    })
}

fn facing(axis: usize, positive_travel: bool) -> Facing {
    match (axis, positive_travel) {
        (0, true) => Facing::West,
        (0, false) => Facing::East,
        (1, true) => Facing::Down,
        (1, false) => Facing::Up,
        (_, true) => Facing::North,
        (_, false) => Facing::South,
    }
}

pub fn raycast_block(
    store: &ChunkStore,
    data: &Version,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> Option<BlockHit> {
    let mut cell = [
        origin[0].floor() as i32,
        origin[1].floor() as i32,
        origin[2].floor() as i32,
    ];
    let mut step = [0i32; 3];
    let mut t_next = [f64::INFINITY; 3];
    let mut t_delta = [f64::INFINITY; 3];
    for a in 0..3 {
        if dir[a] > 0.0 {
            step[a] = 1;
            t_delta[a] = 1.0 / dir[a];
            t_next[a] = (f64::from(cell[a]) + 1.0 - origin[a]) / dir[a];
        } else if dir[a] < 0.0 {
            step[a] = -1;
            t_delta[a] = -1.0 / dir[a];
            t_next[a] = (origin[a] - f64::from(cell[a])) / -dir[a];
        }
    }
    let limit = (max * 3.0).ceil() as usize + 4;
    for _ in 0..limit {
        if let Some(state) = store.block(cell[0], cell[1], cell[2]) {
            let state = u32::from(state.0);
            let local = [
                origin[0] - f64::from(cell[0]),
                origin[1] - f64::from(cell[1]),
                origin[2] - f64::from(cell[2]),
            ];
            let mut best: Option<(f64, Facing, bool)> = None;
            for b in data.outline_shape(state) {
                let lo = b.min.map(f64::from);
                let hi = b.max.map(f64::from);
                if let Some(s) = slab(local, dir, lo, hi)
                    && s.t_min <= max
                {
                    let inside = s.t_min < 0.0;
                    let t = s.t_min.max(0.0);
                    if best.is_none_or(|(bt, ..)| t < bt) {
                        best = Some((t, facing(s.axis, s.positive), inside));
                    }
                }
            }
            if let Some((t, face, inside)) = best {
                let hit = [
                    local[0] + dir[0] * t,
                    local[1] + dir[1] * t,
                    local[2] + dir[2] * t,
                ];
                return Some(BlockHit {
                    pos: BlockPos::new(cell[0], cell[1], cell[2]),
                    face,
                    cursor: hit.map(|v| v.clamp(0.0, 1.0) as f32),
                    inside,
                    state,
                    distance: t,
                });
            }
        }
        let axis = (0..3)
            .min_by(|&a, &b| t_next[a].total_cmp(&t_next[b]))
            .unwrap_or(0);
        if t_next[axis] > max {
            return None;
        }
        cell[axis] += step[axis];
        t_next[axis] += t_delta[axis];
    }
    None
}

pub fn raycast_entity(
    entities: &Entities,
    data: &Version,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> Option<(i32, f64)> {
    let mut best: Option<(i32, f64)> = None;
    for e in entities.iter() {
        let Some(info) = u32::try_from(e.kind)
            .ok()
            .and_then(|k| data.entity_type_info(k))
        else {
            continue;
        };
        let half = f64::from(info.width) / 2.0;
        let min = [e.pos[0] - half, e.pos[1], e.pos[2] - half];
        let top = [
            e.pos[0] + half,
            e.pos[1] + f64::from(info.height),
            e.pos[2] + half,
        ];
        if let Some(s) = slab(origin, dir, min, top) {
            let t = s.t_min.max(0.0);
            if t <= max && best.is_none_or(|(_, bt)| t < bt) {
                best = Some((e.id, t));
            }
        }
    }
    best
}

pub fn pick(
    store: &ChunkStore,
    entities: &Entities,
    data: &Version,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> Option<Target> {
    let block = raycast_block(store, data, origin, dir, max);
    let entity = raycast_entity(
        entities,
        data,
        origin,
        dir,
        block.map_or(max, |b| b.distance),
    );
    match (block, entity) {
        (_, Some((id, distance))) => Some(Target::Entity { id, distance }),
        (Some(b), None) => Some(Target::Block(b)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use mcr_world::{BlockState, ChunkColumn, ChunkPos, WorldConfig};

    use super::*;

    fn world_with(block: (i32, i32, i32), name: &str) -> (ChunkStore, u32) {
        let data = mcr_data::v26_3();
        let cfg = WorldConfig::new(data.state_count() as u32, 80, -64, 384).unwrap();
        let mut store = ChunkStore::new(cfg);
        store.insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
        let id = data.block_by_name(name).unwrap();
        let state = data.block_info(id).unwrap().default_state;
        store.set_block(block.0, block.1, block.2, BlockState(state as u16));
        (store, state)
    }

    #[test]
    fn hits_the_near_face_of_a_block() {
        let data = mcr_data::v26_3();
        let (store, _) = world_with((5, 64, 5), "minecraft:stone");
        let hit = raycast_block(&store, data, [5.5, 64.5, 2.5], [0.0, 0.0, 1.0], 4.5).unwrap();
        assert_eq!(hit.pos, BlockPos::new(5, 64, 5));
        assert_eq!(hit.face, Facing::North);
        assert!((hit.distance - 2.5).abs() < 1e-9);
    }

    #[test]
    fn respects_reach_and_half_height_shapes() {
        let data = mcr_data::v26_3();
        let (store, _) = world_with((5, 64, 5), "minecraft:stone");
        assert!(raycast_block(&store, data, [5.5, 64.5, -1.0], [0.0, 0.0, 1.0], 4.5).is_none());
        let slab_state = {
            let id = data.block_by_name("minecraft:stone_slab").unwrap();
            let (first, end) = data.block_info(id).unwrap().states;
            (first..end)
                .find(|&s| {
                    data.state_properties(s)
                        .any(|(k, v)| k == "type" && v == "bottom")
                })
                .unwrap()
        };
        let (mut store, _) = world_with((0, 0, 0), "minecraft:air");
        store.set_block(5, 64, 5, BlockState(slab_state as u16));
        assert!(raycast_block(&store, data, [5.5, 64.75, 2.5], [0.0, 0.0, 1.0], 4.5).is_none());
        let top = raycast_block(&store, data, [5.5, 66.0, 5.5], [0.0, -1.0, 0.0], 4.5).unwrap();
        assert_eq!(top.face, Facing::Up);
        assert!((top.cursor[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn looking_down_hits_the_top_face() {
        let data = mcr_data::v26_3();
        let (store, _) = world_with((5, 64, 5), "minecraft:stone");
        let hit = raycast_block(&store, data, [5.5, 66.0, 5.5], [0.0, -1.0, 0.0], 4.5).unwrap();
        assert_eq!(hit.face, Facing::Up);
    }
}
