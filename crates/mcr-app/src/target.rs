use mcr_buf::BlockPos;
use mcr_client::{Entities, Entity, PlayerList};
use mcr_data::{Aabb, StateId, Version};
use mcr_entity_model::idx::ARMOR_STAND_FLAGS;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
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
    raycast_shapes(store, data, origin, dir, max, Version::outline_shape)
}

/// Like [`raycast_block`] but against collision shapes, which is vanilla's `ClipContext.Block.VISUAL` for the camera.
pub fn raycast_collision(
    store: &ChunkStore,
    data: &Version,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> Option<BlockHit> {
    raycast_shapes(store, data, origin, dir, max, Version::collision_shape)
}

/// Vanilla `Camera.getMaxZoom`: eight rays from the eye's ±0.1 corners against collision shapes.
pub fn third_person_distance(
    store: &ChunkStore,
    data: &Version,
    eye: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> f64 {
    let mut distance = max;
    for i in 0..8 {
        let offset = [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|b| if b == 1 { 0.1 } else { -0.1 });
        let from = std::array::from_fn(|a| eye[a] + offset[a]);
        if let Some(hit) = raycast_collision(store, data, from, dir, distance) {
            let point: [f64; 3] = std::array::from_fn(|a| from[a] + dir[a] * hit.distance);
            let d = (0..3)
                .map(|a| (point[a] - eye[a]).powi(2))
                .sum::<f64>()
                .sqrt();
            distance = distance.min(d);
        }
    }
    distance
}

fn raycast_shapes(
    store: &ChunkStore,
    data: &Version,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
    shape: for<'d> fn(&'d Version, StateId) -> &'d [Aabb],
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
            for b in shape(data, state) {
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

const REDIRECTABLE_PROJECTILES: [&str; 3] = [
    "minecraft:fireball",
    "minecraft:wind_charge",
    "minecraft:breeze_wind_charge",
];

const ARMOR_STAND_MARKER: i8 = 0x10;

/// `Entity.isPickable` for the entity types the server can send; `misc` mixes living and non-living types.
fn pickable(name: &str, category: &str, e: &Entity, players: &PlayerList) -> bool {
    match name {
        "minecraft:player" => players
            .get(e.uuid)
            .is_none_or(|p| p.game_mode != GameType::Spectator),
        "minecraft:armor_stand" => !matches!(
            e.data.get(&ARMOR_STAND_FLAGS),
            Some(EntityDataValue::Byte(f)) if f & ARMOR_STAND_MARKER != 0
        ),
        "minecraft:falling_block"
        | "minecraft:tnt"
        | "minecraft:end_crystal"
        | "minecraft:interaction"
        | "minecraft:shulker_bullet"
        | "minecraft:item_frame"
        | "minecraft:glow_item_frame"
        | "minecraft:painting"
        | "minecraft:leash_knot"
        | "minecraft:cushion"
        | "minecraft:mannequin"
        | "minecraft:copper_golem"
        | "minecraft:iron_golem"
        | "minecraft:snow_golem"
        | "minecraft:villager"
        | "minecraft:minecart" => true,
        _ if REDIRECTABLE_PROJECTILES.contains(&name) => true,
        _ if category != "misc" => true,
        _ => name.ends_with("_boat") || name.ends_with("_raft") || name.ends_with("_minecart"),
    }
}

fn pick_radius(name: &str) -> f64 {
    if REDIRECTABLE_PROJECTILES.contains(&name) {
        1.0
    } else {
        0.0
    }
}

pub struct EntityPicker<'a> {
    pub entities: &'a Entities,
    pub players: &'a PlayerList,
    pub data: &'a Version,
    pub own_id: i32,
    pub own_vehicle: Option<i32>,
}

impl EntityPicker<'_> {
    /// Nearest pickable entity strictly closer than `max`.
    pub fn raycast(&self, origin: [f64; 3], dir: [f64; 3], max: f64) -> Option<(i32, f64)> {
        let mut best: Option<(i32, f64)> = None;
        for e in self.entities.iter() {
            if e.id == self.own_id || Some(e.id) == self.own_vehicle {
                continue;
            }
            let Some(kind) = u32::try_from(e.kind).ok() else {
                continue;
            };
            let (Some(info), Some(name)) = (
                self.data.entity_type_info(kind),
                self.data.entity_type_name(kind),
            ) else {
                continue;
            };
            if !pickable(name, info.category, e, self.players) {
                continue;
            }
            let grow = pick_radius(name);
            let half = f64::from(info.width) / 2.0 + grow;
            let min = [e.pos[0] - half, e.pos[1] - grow, e.pos[2] - half];
            let top = [
                e.pos[0] + half,
                e.pos[1] + f64::from(info.height) + grow,
                e.pos[2] + half,
            ];
            let Some(s) = slab(origin, dir, min, top) else {
                continue;
            };
            let t = s.t_min.max(0.0);
            if t < max && best.is_none_or(|(_, bt)| t < bt) {
                best = Some((e.id, t));
            }
        }
        best
    }
}

/// `GameRenderer.pick`: an entity wins only when strictly nearer than the block hit.
pub fn pick(
    store: &ChunkStore,
    picker: &EntityPicker<'_>,
    origin: [f64; 3],
    dir: [f64; 3],
    max: f64,
) -> Option<Target> {
    let block = raycast_block(store, picker.data, origin, dir, max);
    let entity = picker.raycast(origin, dir, block.map_or(max, |b| b.distance));
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
    fn third_person_camera_stops_at_walls_but_not_plants() {
        let data = mcr_data::v26_3();
        let back = [0.0, 0.0, -1.0];
        let (wall, _) = world_with((5, 64, 3), "minecraft:stone");
        let d = third_person_distance(&wall, data, [5.5, 64.5, 5.5], back, 4.0);
        assert!((d - 1.5).abs() < 0.02, "{d}");
        let (grass, _) = world_with((5, 64, 4), "minecraft:short_grass");
        assert_eq!(
            third_person_distance(&grass, data, [5.5, 64.5, 5.5], back, 4.0),
            4.0
        );
        let (open, _) = world_with((5, 64, 4), "minecraft:air");
        assert_eq!(
            third_person_distance(&open, data, [5.5, 64.5, 5.5], back, 4.0),
            4.0
        );
    }

    #[test]
    fn looking_down_hits_the_top_face() {
        let data = mcr_data::v26_3();
        let (store, _) = world_with((5, 64, 5), "minecraft:stone");
        let hit = raycast_block(&store, data, [5.5, 66.0, 5.5], [0.0, -1.0, 0.0], 4.5).unwrap();
        assert_eq!(hit.face, Facing::Up);
    }

    fn entity(id: i32, name: &str, pos: [f64; 3]) -> Entity {
        let kind = mcr_data::v26_3().entity_type_by_name(name).unwrap();
        Entity::new(id, uuid::Uuid::from_u128(id as u128), kind as i32, pos)
    }

    fn pick_with(store: &ChunkStore, list: Vec<Entity>) -> Option<Target> {
        let mut entities = Entities::default();
        for e in list {
            entities.insert(e);
        }
        let players = PlayerList::default();
        let picker = EntityPicker {
            entities: &entities,
            players: &players,
            data: mcr_data::v26_3(),
            own_id: 0,
            own_vehicle: None,
        };
        pick(store, &picker, [5.5, 65.0, 0.5], [0.0, 0.0, 1.0], 4.5)
    }

    #[test]
    fn non_pickable_entities_are_ignored() {
        let (store, _) = world_with((5, 64, 5), "minecraft:air");
        for name in [
            "minecraft:item",
            "minecraft:experience_orb",
            "minecraft:arrow",
            "minecraft:area_effect_cloud",
            "minecraft:marker",
            "minecraft:snowball",
        ] {
            assert_eq!(
                pick_with(&store, vec![entity(7, name, [5.5, 64.0, 3.0])]),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn pickable_entities_are_targeted() {
        let (store, _) = world_with((5, 64, 5), "minecraft:air");
        for name in [
            "minecraft:cow",
            "minecraft:oak_boat",
            "minecraft:minecart",
            "minecraft:item_frame",
        ] {
            let hit = pick_with(&store, vec![entity(7, name, [5.5, 64.5, 3.0])]);
            assert!(matches!(hit, Some(Target::Entity { id: 7, .. })), "{name}");
        }
    }

    #[test]
    fn nearer_block_beats_entity_and_item_never_blocks_block() {
        let (store, _) = world_with((5, 65, 2), "minecraft:stone");
        let cow = entity(7, "minecraft:cow", [5.5, 64.0, 4.0]);
        assert!(matches!(
            pick_with(&store, vec![cow]),
            Some(Target::Block(_))
        ));
        let item = entity(8, "minecraft:item", [5.5, 65.0, 1.5]);
        assert!(matches!(
            pick_with(&store, vec![item]),
            Some(Target::Block(_))
        ));
    }

    #[test]
    fn marker_armor_stand_and_fireball_radius() {
        let (store, _) = world_with((5, 64, 5), "minecraft:air");
        let mut stand = entity(7, "minecraft:armor_stand", [5.5, 64.0, 3.0]);
        assert!(pick_with(&store, vec![stand.clone()]).is_some());
        stand
            .data
            .insert(ARMOR_STAND_FLAGS, EntityDataValue::Byte(0x10));
        assert_eq!(pick_with(&store, vec![stand]), None);
        let fireball = entity(8, "minecraft:fireball", [6.9, 64.0, 3.0]);
        assert!(pick_with(&store, vec![fireball]).is_some());
    }
}
