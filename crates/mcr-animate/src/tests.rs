use std::cell::Cell;
use std::collections::HashMap;

use mcr_audio::SoundSource;
use mcr_nbt::snbt;
use mcr_protocol::types::particle::Particle;

use super::*;

type P = ([f64; 3], [f64; 3], bool);

#[derive(Default)]
struct Rec {
    particles: Vec<(Particle, P)>,
    cues: Vec<Cue>,
}

impl Sink for Rec {
    fn particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3], always: bool) {
        self.particles.push((particle.clone(), (pos, vel, always)));
    }

    fn sound(&mut self, cue: &Cue) {
        self.cues.push(*cue);
    }
}

impl Rec {
    fn count(&self, name: &str) -> usize {
        let id = simple(name);
        self.particles.iter().filter(|(p, _)| *p == id).count()
    }

    fn only(&self, name: &str) -> Vec<P> {
        let id = simple(name);
        self.particles
            .iter()
            .filter(|(p, _)| *p == id)
            .map(|(_, p)| *p)
            .collect()
    }

    fn sounds(&self, event: &str) -> usize {
        self.cues.iter().filter(|c| c.event == event).count()
    }
}

fn simple(name: &str) -> Particle {
    let id = mcr_particles::PARTICLE_NAMES
        .iter()
        .position(|n| *n == name)
        .expect(name);
    Particle::Simple(id as i32)
}

fn data() -> &'static Version {
    mcr_data::v26_3()
}

fn state(block: &str, props: &[(&str, &str)]) -> u32 {
    let data = data();
    let id = data
        .block_by_name(&format!("minecraft:{block}"))
        .expect(block);
    let (first, last) = data.block_info(id).unwrap().states;
    (first..last)
        .find(|s| {
            let have: Vec<_> = data.state_properties(*s).collect();
            props.iter().all(|p| have.contains(p))
        })
        .unwrap_or_else(|| panic!("{block} {props:?}"))
}

fn block_id(block: &str) -> u16 {
    data().block_by_name(&format!("minecraft:{block}")).unwrap() as u16
}

struct World {
    blocks: HashMap<[i32; 3], u32>,
    fill: u32,
    queries: Cell<u32>,
    raining: bool,
    thundering: bool,
    ambient: Vec<(Particle, f32)>,
    drip: Option<Particle>,
    surface: i32,
    creaking: bool,
    firefly: bool,
    darken: u8,
}

impl World {
    fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            fill: state("air", &[]),
            queries: Cell::new(0),
            raining: false,
            thundering: false,
            ambient: Vec::new(),
            drip: None,
            surface: 0,
            creaking: false,
            firefly: false,
            darken: 0,
        }
    }

    fn set(&mut self, p: [i32; 3], s: u32) -> &mut Self {
        self.blocks.insert(p, s);
        self
    }
}

impl AnimateWorld for World {
    fn state(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        self.queries.set(self.queries.get() + 1);
        Some(self.blocks.get(&[x, y, z]).copied().unwrap_or(self.fill))
    }

    fn light(&self, _: i32, _: i32, _: i32) -> (u8, u8) {
        (0, 15)
    }

    fn raining_at(&self, _: i32, _: i32, _: i32) -> bool {
        self.raining
    }

    fn thundering(&self) -> bool {
        self.thundering
    }

    fn game_time(&self) -> i64 {
        199
    }

    fn height(&self, _: Height, _: i32, _: i32) -> i32 {
        self.surface
    }

    fn sky_darken(&self) -> u8 {
        self.darken
    }

    fn creaking_active(&self) -> bool {
        self.creaking
    }

    fn ambient_particles(&self, _: i32, _: i32, _: i32) -> &[(Particle, f32)] {
        &self.ambient
    }

    fn dripstone_particle(&self, _: i32, _: i32, _: i32) -> Option<&Particle> {
        self.drip.as_ref()
    }

    fn firefly_sounds(&self, _: i32, _: i32, _: i32) -> bool {
        self.firefly
    }
}

fn animator() -> Animator {
    let mut a = Animator::new(data(), 0xC0FFEE);
    a.table_mut().set_tags(&|name, block| {
        let is = |b: &str| u32::from(block_id(b)) == block;
        match name {
            "minecraft:enchantment_power_provider" => is("bookshelf"),
            "minecraft:enchantment_power_transmitter" => is("air"),
            "minecraft:pale_oak_logs" => is("pale_oak_log"),
            "minecraft:required_for_poplar_leaf_ambience" => is("poplar_log"),
            "minecraft:triggers_ambient_dried_ghast_block_sounds" => is("soul_sand"),
            "minecraft:triggers_ambient_desert_dry_vegetation_block_sounds" => is("sand"),
            "minecraft:triggers_ambient_desert_sand_block_sounds" => is("sand"),
            "minecraft:impermeable" => is("glass"),
            "minecraft:terracotta" => is("terracotta"),
            _ => false,
        }
    });
    a
}

fn run(world: &World, at: [i32; 3], n: u32, sounds: bool) -> Rec {
    let mut a = animator();
    let mut rec = Rec::default();
    for _ in 0..n {
        a.sample_at(at, world, &mut rec, sounds);
    }
    rec
}

fn rate(hits: usize, n: u32) -> f64 {
    hits as f64 / f64::from(n)
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

const AT: [i32; 3] = [4, 70, 4];

fn world_with(s: u32) -> World {
    let mut w = World::new();
    w.set(AT, s);
    w
}

#[test]
fn table_covers_every_kind_and_names_resolve() {
    let a = animator();
    let mut seen = std::collections::HashSet::new();
    for s in 0..a.table().states() as u32 {
        seen.insert(a.table().info(s).kind);
    }
    use Kind::*;
    for kind in [
        Torch,
        WallTorch,
        RedstoneTorch,
        RedstoneWallTorch,
        Candle,
        CandleCake,
        Campfire,
        Fire,
        SoulFire,
        Furnace,
        BlastFurnace,
        Smoker,
        BrewingStand,
        Beehive,
        Brushable,
        FallingBlock,
        Sand,
        BubbleColumn,
        CreakingHeart,
        CryingObsidian,
        DriedGhast,
        DeadBush,
        DryGrass,
        EnchantingTable,
        EnderChest,
        EndPortal,
        EndRod,
        Eyeblossom,
        FireflyBush,
        HangingMoss,
        Leaves,
        Lever,
        LightningRod,
        Mycelium,
        NetherPortal,
        PointedDripstone,
        PotentSulfur,
        RedstoneOre,
        RedstoneWire,
        Repeater,
        RespawnAnchor,
        SculkSensor,
        SporeBlossom,
        WetSponge,
        WitherRose,
    ] {
        assert!(seen.contains(&kind), "{kind:?} has no state");
    }
    for (name, _) in table::LEAVES {
        assert!(
            data().block_by_name(&format!("minecraft:{name}")).is_some(),
            "{name}"
        );
    }
    Ids::new();
}

#[test]
fn unlit_and_unpowered_states_have_no_behaviour() {
    let a = animator();
    for (block, props) in [
        ("furnace", vec![("lit", "false")]),
        ("redstone_ore", vec![("lit", "false")]),
        ("campfire", vec![("lit", "false")]),
        ("white_candle", vec![("lit", "false")]),
        ("lever", vec![("powered", "false")]),
        ("repeater", vec![("powered", "false")]),
        ("respawn_anchor", vec![("charges", "0")]),
        ("creaking_heart", vec![("creaking_heart_state", "uprooted")]),
        ("sculk_sensor", vec![("sculk_sensor_phase", "inactive")]),
        ("potent_sulfur", vec![("potent_sulfur_state", "dry")]),
        ("redstone_wire", vec![("power", "0")]),
    ] {
        let s = state(block, &props);
        assert_eq!(a.table().info(s).kind, Kind::None, "{block}");
    }
    assert_eq!(a.table().info(state("stone", &[])).kind, Kind::None);
}

#[test]
fn torch_makes_one_smoke_and_one_flame() {
    let w = world_with(state("torch", &[]));
    let rec = run(&w, AT, 1, true);
    for name in ["smoke", "flame"] {
        let got = rec.only(name);
        assert_eq!(got.len(), 1, "{name}");
        assert_eq!(got[0].0, [4.5, 70.7, 4.5]);
    }
    let soul = run(&world_with(state("soul_torch", &[])), AT, 1, true);
    assert_eq!(soul.count("soul_fire_flame"), 1);
    let copper = run(&world_with(state("copper_torch", &[])), AT, 1, true);
    assert_eq!(copper.count("copper_fire_flame"), 1);
}

#[test]
fn wall_torch_moves_toward_the_wall() {
    let w = world_with(state("wall_torch", &[("facing", "north")]));
    let rec = run(&w, AT, 1, true);
    let flame = rec.only("flame")[0].0;
    assert!(near(flame[0], 4.5, 1e-9) && near(flame[1], 70.92, 1e-9) && near(flame[2], 4.77, 1e-9));
}

#[test]
fn redstone_torch_only_when_lit_and_jitters_in_a_small_box() {
    let w = world_with(state("redstone_torch", &[("lit", "true")]));
    let rec = run(&w, AT, 500, true);
    assert_eq!(rec.particles.len(), 500);
    for (p, (pos, _, _)) in &rec.particles {
        assert_eq!(*p, ids::REDSTONE_DUST);
        assert!((4.4..=4.6).contains(&pos[0]) && (70.6..=70.8).contains(&pos[1]));
    }
}

#[test]
fn candles_flame_every_candle_and_smoke_with_a_sound() {
    let w = world_with(state("white_candle", &[("lit", "true"), ("candles", "4")]));
    let n = 4000;
    let rec = run(&w, AT, n, true);
    assert_eq!(rec.count("small_flame"), 4 * n as usize);
    assert!(near(rate(rec.count("smoke"), 4 * n), 0.3, 0.02));
    assert!(near(
        rate(rec.sounds("block.candle.ambient"), 4 * n),
        0.17,
        0.015
    ));
    let cake = run(
        &world_with(state("candle_cake", &[("lit", "true")])),
        AT,
        1,
        true,
    );
    assert_eq!(cake.only("small_flame")[0].0, [4.5, 71.0, 4.5]);
}

#[test]
fn campfire_crackles_and_only_the_normal_one_pops_lava() {
    let n = 20_000;
    let normal = run(
        &world_with(state("campfire", &[("lit", "true")])),
        AT,
        n,
        true,
    );
    assert!(near(
        rate(normal.sounds("block.campfire.crackle"), n),
        0.1,
        0.01
    ));
    assert!(near(rate(normal.count("lava"), n), 0.2, 0.015));
    let soul = run(
        &world_with(state("soul_campfire", &[("lit", "true")])),
        AT,
        n,
        true,
    );
    assert_eq!(soul.count("lava"), 0);
    assert!(soul.sounds("block.campfire.crackle") > 0);
}

#[test]
fn fire_smokes_from_burnable_neighbours_when_unsupported() {
    let mut w = world_with(state("fire", &[]));
    w.set([3, 70, 4], state("oak_planks", &[]));
    let rec = run(&w, AT, 10, true);
    assert_eq!(rec.count("large_smoke"), 20);
    let supported = run(
        &{
            let mut w = world_with(state("fire", &[]));
            w.set([4, 69, 4], state("stone", &[]));
            w
        },
        AT,
        10,
        true,
    );
    assert_eq!(supported.count("large_smoke"), 30);
    let soul = run(&world_with(state("soul_fire", &[])), AT, 10, true);
    assert_eq!(soul.count("large_smoke"), 30);
    let n = 20_000;
    let crackle = run(&world_with(state("fire", &[])), AT, n, true);
    assert!(near(
        rate(crackle.sounds("block.fire.ambient"), n),
        1.0 / 24.0,
        0.006
    ));
}

#[test]
fn waterlogged_planks_do_not_burn() {
    let mut w = world_with(state("fire", &[]));
    w.set(
        [3, 70, 4],
        state("oak_slab", &[("waterlogged", "true"), ("type", "bottom")]),
    );
    let rec = run(&w, AT, 10, true);
    assert_eq!(rec.count("large_smoke"), 0);
}

#[test]
fn furnaces_smoke_on_their_front_and_crackle() {
    let n = 20_000;
    let furnace = run(
        &world_with(state("furnace", &[("lit", "true"), ("facing", "north")])),
        AT,
        n,
        true,
    );
    assert!(near(
        rate(furnace.sounds("block.furnace.fire_crackle"), n),
        0.1,
        0.01
    ));
    assert_eq!(furnace.count("smoke"), n as usize);
    assert_eq!(furnace.count("flame"), n as usize);
    for (_, (pos, _, _)) in &furnace.particles {
        assert!(near(pos[2], 4.5 - 0.52, 1e-9));
        assert!((4.2..=4.8).contains(&pos[0]));
        assert!((70.0..=70.375).contains(&pos[1]));
    }
    let blast = run(
        &world_with(state(
            "blast_furnace",
            &[("lit", "true"), ("facing", "east")],
        )),
        AT,
        n,
        true,
    );
    assert_eq!(blast.count("flame"), 0);
    assert!(blast.sounds("block.blastfurnace.fire_crackle") > 0);
    assert!(
        blast
            .only("smoke")
            .iter()
            .all(|(p, _, _)| near(p[0], 4.5 + 0.52, 1e-9))
    );
    let smoker = run(
        &world_with(state("smoker", &[("lit", "true"), ("facing", "south")])),
        AT,
        n,
        true,
    );
    assert!(smoker.sounds("block.smoker.smoke") > 0);
    assert!(smoker.only("smoke").iter().all(|(p, _, _)| p[1] == 71.1));
}

#[test]
fn unlit_furnace_is_silent() {
    let w = world_with(state("furnace", &[("lit", "false"), ("facing", "north")]));
    let rec = run(&w, AT, 1000, true);
    assert!(rec.particles.is_empty() && rec.cues.is_empty());
}

#[test]
fn brewing_stand_beehive_and_end_portal() {
    let rec = run(&world_with(state("brewing_stand", &[])), AT, 100, true);
    assert_eq!(rec.count("smoke"), 100);
    let honey = run(
        &world_with(state(
            "beehive",
            &[("honey_level", "5"), ("facing", "north")],
        )),
        AT,
        10_000,
        true,
    );
    assert!(near(rate(honey.count("dripping_honey"), 10_000), 0.7, 0.02));
    assert!(
        honey
            .only("dripping_honey")
            .iter()
            .all(|(p, _, _)| near(p[1], 69.95, 1e-9))
    );
    let empty = run(
        &world_with(state(
            "beehive",
            &[("honey_level", "4"), ("facing", "north")],
        )),
        AT,
        100,
        true,
    );
    assert!(empty.particles.is_empty());
    let portal = run(&world_with(state("end_portal", &[])), AT, 50, true);
    assert!(portal.only("smoke").iter().all(|(p, _, _)| p[1] == 70.8));
}

#[test]
fn nether_portal_swirls_toward_the_open_axis() {
    let n = 10_000;
    let mut w = world_with(state("nether_portal", &[("axis", "x")]));
    w.set([3, 70, 4], state("nether_portal", &[("axis", "x")]));
    let rec = run(&w, AT, n, true);
    assert_eq!(rec.count("portal"), 4 * n as usize);
    assert!(near(
        rate(rec.sounds("block.portal.ambient"), n),
        0.01,
        0.003
    ));
    for (pos, vel, _) in rec.only("portal").iter().take(2000) {
        assert!(near((pos[2] - 4.5).abs(), 0.25, 1e-9));
        assert!(vel[2].abs() <= 2.0);
    }
    assert!(
        rec.cues
            .iter()
            .all(|c| c.volume == 0.5 && (0.8..=1.2).contains(&c.pitch))
    );
}

#[test]
fn ender_chest_end_rod_and_respawn_anchor() {
    let chest = run(
        &world_with(state(
            "ender_chest",
            &[("facing", "north"), ("waterlogged", "false")],
        )),
        AT,
        10,
        true,
    );
    assert_eq!(chest.count("portal"), 30);
    let rod = run(
        &world_with(state("end_rod", &[("facing", "up")])),
        AT,
        20_000,
        true,
    );
    assert!(near(rate(rod.count("end_rod"), 20_000), 0.2, 0.01));
    assert!(
        rod.only("end_rod")
            .iter()
            .all(|(_, v, _)| v.iter().all(|c| c.abs() < 0.05))
    );
    let anchor = run(
        &world_with(state("respawn_anchor", &[("charges", "2")])),
        AT,
        20_000,
        true,
    );
    assert_eq!(anchor.count("reverse_portal"), 20_000);
    assert!(near(
        rate(anchor.sounds("block.respawn_anchor.ambient"), 20_000),
        0.01,
        0.003
    ));
    assert!(
        anchor
            .only("reverse_portal")
            .iter()
            .all(|(p, _, _)| p[1] == 71.0)
    );
}

#[test]
fn end_gateway_swirls_once_per_exposed_face() {
    let mut w = world_with(state("end_gateway", &[]));
    let rec = run(&w, AT, 10, true);
    assert_eq!(rec.count("portal"), 60);
    for d in table::DIRS.iter().take(2) {
        w.set(
            [AT[0] + d[0], AT[1] + d[1], AT[2] + d[2]],
            state("bedrock", &[]),
        );
    }
    assert_eq!(run(&w, AT, 10, true).count("portal"), 40);
}

#[test]
fn enchanting_table_glyphs_need_a_clear_bookshelf_line() {
    let mut w = world_with(state("enchanting_table", &[]));
    w.set([6, 70, 4], state("bookshelf", &[]));
    let rec = run(&w, AT, 8000, true);
    assert!(near(rate(rec.count("enchant"), 8000), 1.0 / 16.0, 0.01));
    let (pos, vel, _) = rec.only("enchant")[0];
    assert_eq!(pos, [4.5, 72.0, 4.5]);
    assert!((1.5..=2.5).contains(&vel[0]) && (-2.0..=-1.0).contains(&vel[1]));
    w.set([5, 70, 4], state("stone", &[]));
    let blocked = run(&w, AT, 4000, true);
    assert_eq!(blocked.count("enchant"), 0);
}

#[test]
fn redstone_ore_wire_repeater_lever_and_sculk() {
    let mut ore = world_with(state("redstone_ore", &[("lit", "true")]));
    for d in table::DIRS.iter().take(2) {
        ore.set(
            [AT[0] + d[0], AT[1] + d[1], AT[2] + d[2]],
            state("stone", &[]),
        );
    }
    let rec = run(&ore, AT, 10, true);
    assert_eq!(rec.particles.len(), 40);
    let lit_wire = world_with(state(
        "redstone_wire",
        &[
            ("power", "15"),
            ("north", "none"),
            ("east", "none"),
            ("south", "none"),
            ("west", "none"),
        ],
    ));
    let wire = run(&lit_wire, AT, 20_000, true);
    assert!(near(rate(wire.particles.len(), 20_000), 4.0 * 0.06, 0.02));
    let Particle::Dust { color, .. } = wire.particles[0].0 else {
        panic!()
    };
    assert_eq!(color, 0xFF_3300);
    let repeater = run(
        &world_with(state(
            "repeater",
            &[("powered", "true"), ("facing", "north"), ("delay", "4")],
        )),
        AT,
        4000,
        true,
    );
    assert_eq!(repeater.particles.len(), 4000);
    let lever = run(
        &world_with(state(
            "lever",
            &[("powered", "true"), ("face", "floor"), ("facing", "north")],
        )),
        AT,
        20_000,
        true,
    );
    assert!(near(rate(lever.particles.len(), 20_000), 0.25, 0.015));
    let sculk = run(
        &world_with(state(
            "sculk_sensor",
            &[
                ("sculk_sensor_phase", "active"),
                ("power", "0"),
                ("waterlogged", "false"),
            ],
        )),
        AT,
        12_000,
        true,
    );
    assert!(near(rate(sculk.particles.len(), 12_000), 4.0 / 6.0, 0.02));
    assert!(
        sculk
            .particles
            .iter()
            .all(|(p, _)| matches!(p, Particle::DustColorTransition { .. }))
    );
}

#[test]
fn lava_pops_only_under_air() {
    let n = 100_000;
    let open = run(&world_with(state("lava", &[("level", "0")])), AT, n, true);
    assert!(near(rate(open.count("lava"), n), 0.01, 0.002));
    assert_eq!(open.sounds("block.lava.pop"), open.count("lava"));
    assert!(near(
        rate(open.sounds("block.lava.ambient"), n),
        0.005,
        0.0015
    ));
    assert!(open.only("lava").iter().all(|(p, _, _)| p[1] == 71.0));
    assert!(open.cues.iter().all(|c| c.source == SoundSource::Ambient));
    let mut covered = world_with(state("lava", &[("level", "0")]));
    covered.set([4, 71, 4], state("stone", &[]));
    assert!(run(&covered, AT, n, true).particles.is_empty());
}

#[test]
fn water_sounds_flow_and_bubbles_for_still_water() {
    let n = 20_000;
    let flowing = run(&world_with(state("water", &[("level", "3")])), AT, n, true);
    assert!(near(
        rate(flowing.sounds("block.water.ambient"), n),
        1.0 / 64.0,
        0.003
    ));
    assert_eq!(flowing.count("underwater"), 0);
    let still = run(&world_with(state("water", &[("level", "0")])), AT, n, true);
    assert!(near(rate(still.count("underwater"), n), 0.1, 0.01));
    assert_eq!(still.sounds("block.water.ambient"), 0);
}

#[test]
fn lava_drips_off_the_underside_of_a_top_slab() {
    let n = 20_000;
    let mut w = world_with(state("lava", &[("level", "0")]));
    w.set(
        [4, 69, 4],
        state("stone_slab", &[("type", "top"), ("waterlogged", "false")]),
    );
    let rec = run(&w, AT, n, true);
    let drips = rec.only("dripping_lava");
    assert!(near(rate(drips.len(), n), 0.1, 0.01));
    assert!(
        drips
            .iter()
            .all(|(p, _, _)| near(p[1], 69.0 + 0.5 - 0.05, 1e-9))
    );
    let mut glass = world_with(state("lava", &[("level", "0")]));
    glass.set([4, 69, 4], state("glass", &[]));
    assert_eq!(run(&glass, AT, n, true).count("dripping_lava"), 0);
}

#[test]
fn water_drips_through_bottomless_air_only_with_a_sturdy_floor() {
    let n = 20_000;
    let sturdy = state("water", &[("level", "0")]);
    let rec = run(&world_with(sturdy), AT, n, true);
    assert_eq!(rec.count("dripping_water"), 0);
    let mut leaky = world_with(state(
        "stone_slab",
        &[("type", "bottom"), ("waterlogged", "true")],
    ));
    leaky.set([4, 69, 4], state("air", &[]));
    let rec = run(&leaky, AT, n, true);
    assert!(rec.count("dripping_water") > 0);
}

#[test]
fn crying_obsidian_wet_sponge_and_wither_rose() {
    let n = 20_000;
    let obsidian = run(&world_with(state("crying_obsidian", &[])), AT, n, true);
    assert!(near(
        rate(obsidian.count("dripping_obsidian_tear"), n),
        0.2 * 5.0 / 6.0,
        0.015
    ));
    let mut buried = world_with(state("crying_obsidian", &[]));
    for d in table::DIRS {
        buried.set(
            [AT[0] + d[0], AT[1] + d[1], AT[2] + d[2]],
            state("stone", &[]),
        );
    }
    assert_eq!(run(&buried, AT, n, true).count("dripping_obsidian_tear"), 0);
    let sponge = run(&world_with(state("wet_sponge", &[])), AT, n, true);
    assert!(near(
        rate(sponge.count("dripping_water"), n),
        5.0 / 6.0,
        0.02
    ));
    let rose = run(&world_with(state("wither_rose", &[])), AT, n, true);
    assert!(near(rate(rose.count("smoke"), n), 1.5, 0.05));
}

#[test]
fn leaves_fall_only_over_open_ground_and_drip_in_rain() {
    let n = 100_000;
    let oak = state(
        "oak_leaves",
        &[
            ("distance", "1"),
            ("persistent", "true"),
            ("waterlogged", "false"),
        ],
    );
    let open = run(&world_with(oak), AT, n, true);
    let tinted = open
        .particles
        .iter()
        .filter(|(p, _)| matches!(p, Particle::Color { .. }))
        .count();
    assert!(near(rate(tinted, n), 0.01, 0.002));
    let mut floor = world_with(oak);
    floor.set([4, 69, 4], state("stone", &[]));
    assert!(run(&floor, AT, n, true).particles.is_empty());
    let cherry = state(
        "cherry_leaves",
        &[
            ("distance", "1"),
            ("persistent", "true"),
            ("waterlogged", "false"),
        ],
    );
    let c = run(&world_with(cherry), AT, n, true);
    assert!(near(rate(c.count("cherry_leaves"), n), 0.1, 0.005));
    let spruce = state(
        "spruce_leaves",
        &[
            ("distance", "1"),
            ("persistent", "true"),
            ("waterlogged", "false"),
        ],
    );
    assert!(run(&world_with(spruce), AT, n, true).particles.is_empty());
    let mut rain = world_with(spruce);
    rain.raining = true;
    let wet = run(&rain, AT, n, true);
    assert!(near(
        rate(wet.count("dripping_water"), n),
        1.0 / 15.0,
        0.005
    ));
    let birch = state(
        "birch_leaves",
        &[
            ("distance", "1"),
            ("persistent", "true"),
            ("waterlogged", "false"),
        ],
    );
    let b = run(&world_with(birch), AT, n, true);
    assert!(
        b.particles.iter().any(
            |(p, _)| matches!(p, Particle::Color { argb, .. } if *argb == 0xFF80_A755u32 as i32)
        )
    );
}

#[test]
fn poplar_leaves_hum_between_a_log_and_three_leaves() {
    let leaf = state(
        "red_poplar_leaves",
        &[
            ("distance", "1"),
            ("persistent", "true"),
            ("waterlogged", "false"),
        ],
    );
    let mut w = world_with(leaf);
    w.set([3, 70, 4], leaf)
        .set([5, 70, 4], leaf)
        .set([4, 70, 3], leaf);
    w.set([4, 70, 5], state("poplar_log", &[("axis", "y")]));
    let n = 100_000;
    let rec = run(&w, AT, n, true);
    assert!(near(
        rate(rec.sounds("block.poplar_leaves.ambient"), n),
        1.0 / 300.0,
        0.0015
    ));
    let lonely = run(&world_with(leaf), AT, n, true);
    assert_eq!(lonely.sounds("block.poplar_leaves.ambient"), 0);
    assert!(lonely.count("red_poplar_leaves") > 0);
}

#[test]
fn falling_blocks_shed_dust_over_air_and_sand_hums_in_deserts() {
    let n = 40_000;
    let gravel = run(&world_with(state("gravel", &[])), AT, n, true);
    assert!(near(rate(gravel.particles.len(), n), 1.0 / 16.0, 0.006));
    assert!(
        gravel
            .particles
            .iter()
            .all(|(p, _)| matches!(p, Particle::BlockState { .. }))
    );
    let mut floor = world_with(state("gravel", &[]));
    floor.set([4, 69, 4], state("stone", &[]));
    assert!(run(&floor, AT, n, true).particles.is_empty());
    let mut desert = world_with(state("sand", &[]));
    desert.fill = state("air", &[]);
    desert.surface = 71;
    let sand = state("sand", &[]);
    for d in [[8, 0], [-8, 0], [0, 8], [0, -8]] {
        desert.set([AT[0] + d[0], 70, AT[2] + d[1]], sand);
    }
    let hum = run(&desert, AT, 200_000, true);
    assert!(near(
        rate(hum.sounds("block.sand.idle"), 200_000),
        1.0 / 2100.0,
        0.0004
    ));
    desert.blocks.retain(|k, _| k[0] == AT[0] && k[2] == AT[2]);
    assert_eq!(run(&desert, AT, 200_000, true).sounds("block.sand.idle"), 0);
}

#[test]
fn dry_vegetation_sounds_need_sand_below() {
    let n = 100_000;
    let sand = state("sand", &[]);
    let mut bush = world_with(state("dead_bush", &[]));
    bush.set([4, 69, 4], sand).set([4, 68, 4], sand);
    let rec = run(&bush, AT, n, true);
    assert!(near(
        rate(rec.sounds("block.deadbush.idle"), n),
        1.0 / 130.0,
        0.002
    ));
    assert_eq!(
        run(&world_with(state("dead_bush", &[])), AT, n, true)
            .cues
            .len(),
        0
    );
    let mut grass = world_with(state("short_dry_grass", &[]));
    grass.set([4, 69, 4], sand).set([4, 68, 4], sand);
    let rec = run(&grass, AT, n, true);
    let cue = rec
        .cues
        .iter()
        .find(|c| c.event == "block.dry_grass.ambient")
        .unwrap();
    assert_eq!(cue.pos, None);
}

#[test]
fn spore_blossom_and_mycelium() {
    let rec = run(&world_with(state("spore_blossom", &[])), AT, 100, true);
    assert_eq!(rec.count("falling_spore_blossom"), 100);
    assert!(rec.count("spore_blossom_air") > 1000);
    let n = 20_000;
    let myc = run(
        &world_with(state("mycelium", &[("snowy", "false")])),
        AT,
        n,
        true,
    );
    assert!(near(rate(myc.count("mycelium"), n), 0.1, 0.01));
}

#[test]
fn bubble_columns_are_always_visible() {
    let up = run(
        &world_with(state("bubble_column", &[("drag", "false")])),
        AT,
        10,
        true,
    );
    assert_eq!(up.count("bubble_column_up"), 20);
    assert!(
        up.only("bubble_column_up")
            .iter()
            .all(|(_, v, always)| *always && v[1] == 0.04)
    );
    let down = run(
        &world_with(state("bubble_column", &[("drag", "true")])),
        AT,
        10,
        true,
    );
    assert_eq!(down.count("current_down"), 10);
    let n = 40_000;
    let rec = run(
        &world_with(state("bubble_column", &[("drag", "true")])),
        AT,
        n,
        true,
    );
    assert!(near(
        rate(rec.sounds("block.bubble_column.whirlpool_ambient"), n),
        0.005,
        0.0015
    ));
}

#[test]
fn dried_ghast_branches_on_water() {
    let n = 40_000;
    let mut dry = world_with(state(
        "dried_ghast",
        &[
            ("waterlogged", "false"),
            ("facing", "north"),
            ("hydration", "0"),
        ],
    ));
    dry.set([4, 69, 4], state("soul_sand", &[]));
    let rec = run(&dry, AT, n, true);
    assert!(near(
        rate(rec.sounds("block.dried_ghast.ambient"), n),
        1.0 / 40.0,
        0.004
    ));
    assert!(near(rate(rec.count("white_smoke"), n), 1.0 / 6.0, 0.01));
    let wet = run(
        &world_with(state(
            "dried_ghast",
            &[
                ("waterlogged", "true"),
                ("facing", "north"),
                ("hydration", "0"),
            ],
        )),
        AT,
        n,
        true,
    );
    assert!(wet.sounds("block.dried_ghast.ambient_water") > 0);
    assert!(wet.count("happy_villager") > 0 && wet.count("white_smoke") == 0);
}

#[test]
fn sound_only_blocks_respect_their_conditions() {
    let n = 200_000;
    let mut eye = world_with(state("open_eyeblossom", &[]));
    assert_eq!(run(&eye, AT, n, true).cues.len(), 0);
    eye.set([4, 69, 4], state("pale_moss_block", &[]));
    let rec = run(&eye, AT, n, true);
    assert!(near(
        rate(rec.sounds("block.eyeblossom.idle"), n),
        1.0 / 700.0,
        0.0008
    ));
    let mut moss = world_with(state("pale_hanging_moss", &[]));
    moss.set([4, 71, 4], state("pale_oak_log", &[("axis", "y")]));
    assert!(near(
        rate(
            run(&moss, AT, n, true).sounds("block.pale_hanging_moss.idle"),
            n
        ),
        1.0 / 500.0,
        0.001
    ));
    let mut heart = world_with(state(
        "creaking_heart",
        &[
            ("creaking_heart_state", "awake"),
            ("axis", "y"),
            ("natural", "true"),
        ],
    ));
    for d in table::DIRS {
        heart.set(
            [AT[0] + d[0], AT[1] + d[1], AT[2] + d[2]],
            state("pale_oak_log", &[("axis", "y")]),
        );
    }
    assert_eq!(run(&heart, AT, n, true).cues.len(), 0);
    heart.creaking = true;
    assert!(near(
        rate(
            run(&heart, AT, n, true).sounds("block.creaking_heart.idle"),
            n
        ),
        1.0 / 16.0,
        0.004
    ));
    let mut sulfur = world_with(state("potent_sulfur", &[("potent_sulfur_state", "wet")]));
    sulfur.set([4, 71, 4], state("water", &[("level", "0")]));
    let rec = run(&sulfur, AT, 10_000, true);
    assert_eq!(rec.count("sulfur_bubbles"), 20_000);
    assert!(near(
        rate(rec.sounds("block.potent_sulfur.noxious_gas"), 10_000),
        0.1,
        0.01
    ));
}

#[test]
fn firefly_bushes_glow_in_the_dark_and_hum_under_the_open_sky() {
    let mut w = world_with(state("firefly_bush", &[]));
    w.firefly = true;
    w.surface = 0;
    let n = 40_000;
    let day = run(&w, AT, n, true);
    assert_eq!(day.count("firefly"), 0);
    assert!(near(
        rate(day.sounds("block.firefly_bush.idle"), n),
        1.0 / 30.0,
        0.004
    ));
    w.darken = 15;
    w.surface = 100;
    let night = run(&w, AT, n, true);
    assert!(near(rate(night.count("firefly"), n), 0.7, 0.02));
    assert_eq!(night.sounds("block.firefly_bush.idle"), 0);
}

#[test]
fn lightning_rods_spark_in_storms_at_the_surface() {
    let rod = state(
        "lightning_rod",
        &[
            ("facing", "up"),
            ("powered", "false"),
            ("waterlogged", "false"),
        ],
    );
    let mut w = world_with(rod);
    w.surface = AT[1] + 1;
    assert_eq!(run(&w, AT, 1000, true).particles.len(), 0);
    w.thundering = true;
    let rec = run(&w, AT, 1000, true);
    assert!(rec.particles.len() >= 1000);
    assert!(
        rec.particles
            .iter()
            .all(|(p, _)| *p == simple("electric_spark"))
    );
    w.surface = AT[1] + 5;
    assert_eq!(run(&w, AT, 1000, true).particles.len(), 0);
}

#[test]
fn dripstone_drips_follow_the_fluid_above() {
    let tip = state(
        "pointed_dripstone",
        &[
            ("thickness", "tip"),
            ("vertical_direction", "down"),
            ("waterlogged", "false"),
        ],
    );
    let mut w = world_with(tip);
    w.set([4, 71, 4], state("stone", &[]));
    w.set([4, 72, 4], state("water", &[("level", "0")]));
    let n = 40_000;
    let rec = run(&w, AT, n, true);
    assert!(near(
        rate(rec.count("dripping_dripstone_water"), n),
        0.12,
        0.01
    ));
    w.set([4, 72, 4], state("lava", &[("level", "0")]));
    assert!(run(&w, AT, n, true).count("dripping_dripstone_lava") > 0);
    w.set([4, 72, 4], state("air", &[]));
    assert_eq!(run(&w, AT, n, true).particles.len(), 0);
    w.drip = Some(simple("dripping_dripstone_water"));
    let dry = run(&w, AT, n, true);
    assert!(near(rate(dry.particles.len(), n), 0.02, 0.005));
}

#[test]
fn biome_ambient_particles_skip_full_blocks_and_respect_probability() {
    let mut w = World::new();
    w.ambient = vec![(simple("ash"), 0.25)];
    let n = 40_000;
    let air = run(&w, AT, n, true);
    assert!(near(rate(air.count("ash"), n), 0.25, 0.01));
    let stone = run(
        &world_with(state("stone", &[])).with_ambient(vec![(simple("ash"), 1.0)]),
        AT,
        100,
        true,
    );
    assert!(stone.particles.is_empty());
}

impl World {
    fn with_ambient(mut self, ambient: Vec<(Particle, f32)>) -> Self {
        self.ambient = ambient;
        self
    }
}

#[test]
fn sounds_can_be_switched_off_for_java_owned_audio() {
    let w = world_with(state("furnace", &[("lit", "true"), ("facing", "north")]));
    let rec = run(&w, AT, 5000, false);
    assert!(rec.cues.is_empty());
    assert_eq!(rec.particles.len(), 10_000);
}

#[test]
fn campfire_block_entity_smoke_rises_and_overrides_the_limiter() {
    let mut a = animator();
    let w = World::new();
    let mut rec = Rec::default();
    for _ in 0..4000 {
        a.campfire_tick(AT, true, &w, &mut rec);
    }
    let n = rec.count("campfire_signal_smoke");
    assert!(n > 4000 / 20 * 2 && n < 4000 / 5 * 4, "{n}");
    assert!(
        rec.particles
            .iter()
            .all(|(p, (_, v, always))| *always && v[1] == 0.07 && overrides_limiter(p))
    );
    let mut cosy = Rec::default();
    for _ in 0..200 {
        a.campfire_tick(AT, false, &w, &mut cosy);
    }
    assert!(cosy.count("campfire_cosy_smoke") > 0);
}

#[test]
fn tick_visits_a_fixed_number_of_positions_whatever_the_world_holds() {
    let mut a = animator();
    let empty = World::new();
    let mut rec = Rec::default();
    let stats = a.tick([0, 64, 0], &empty, &mut rec, true);
    assert_eq!(stats.samples, 1334);

    let mut dense = World::new();
    dense.fill = state("enchanting_table", &[]);
    dense.ambient = vec![(simple("ash"), 1.0); 16];
    let mut rec = Rec::default();
    let before = dense.queries.get();
    let stats = a.tick([0, 64, 0], &dense, &mut rec, true);
    assert_eq!(stats.samples, 1334);
    assert!(stats.particles <= MAX_PARTICLES_PER_TICK);
    assert!(stats.sounds <= MAX_SOUNDS_PER_TICK);
    assert!(
        dense.queries.get() - before <= 1334 * 70,
        "{}",
        dense.queries.get()
    );
}

#[test]
fn hostile_registry_lists_cannot_flood_the_pools() {
    let mut a = animator();
    let mut w = World::new();
    w.ambient = vec![(simple("ash"), 1.0); 16];
    let mut rec = Rec::default();
    let stats = a.tick([0, 64, 0], &w, &mut rec, true);
    assert_eq!(stats.particles, MAX_PARTICLES_PER_TICK);
    assert_eq!(rec.particles.len(), MAX_PARTICLES_PER_TICK as usize);

    let mut furnaces = World::new();
    furnaces.fill = state("furnace", &[("lit", "true"), ("facing", "north")]);
    let mut rec = Rec::default();
    let stats = a.tick([0, 64, 0], &furnaces, &mut rec, true);
    assert_eq!(stats.sounds, MAX_SOUNDS_PER_TICK);
}

#[test]
fn tick_samples_within_the_two_radii() {
    let mut a = animator();
    let mut w = World::new();
    w.fill = state("torch", &[]);
    let mut rec = Rec::default();
    a.tick([100, 64, -50], &w, &mut rec, true);
    for (_, (pos, _, _)) in &rec.particles {
        assert!((100.0 - 33.0..=100.0 + 34.0).contains(&pos[0]), "{pos:?}");
        assert!((64.0 - 33.0..=64.0 + 34.0).contains(&pos[1]));
        assert!((-50.0 - 33.0..=-50.0 + 34.0).contains(&pos[2]));
    }
}

fn attrs(text: &str) -> Visual {
    let value = snbt::parse(text).expect("snbt");
    Visual::from_attributes(value.as_compound().expect("compound"))
}

#[test]
fn nether_biomes_append_their_particles_to_the_dimensions() {
    let dimension = attrs(
        r#"{"minecraft:visual/default_dripstone_particle":{type:"minecraft:dripping_dripstone_lava"},"minecraft:gameplay/water_evaporates":1b}"#,
    );
    let biome = attrs(
        r#"{"minecraft:visual/ambient_particles":{modifier:"append",argument:[{particle:{type:"minecraft:ash"},probability:0.00625f}]}}"#,
    );
    let merged = biome.over(&dimension);
    assert_eq!(merged.ambient, vec![(simple("ash"), 0.00625)]);
    assert_eq!(merged.dripstone, Some(simple("dripping_dripstone_lava")));
    assert_eq!(merged.water_evaporates, Some(true));
}

#[test]
fn ambient_lists_are_capped_clamped_and_filtered() {
    let entries: Vec<String> = (0..40)
        .map(|_| r#"{particle:{type:"minecraft:ash"},probability:7.0f}"#.to_owned())
        .collect();
    let v = attrs(&format!(
        r#"{{"minecraft:visual/ambient_particles":[{}]}}"#,
        entries.join(",")
    ));
    assert_eq!(v.ambient.len(), MAX_AMBIENT_PARTICLES);
    assert!(v.ambient.iter().all(|(_, p)| *p == 1.0));
    let odd = attrs(
        r#"{"minecraft:visual/ambient_particles":[{particle:{type:"minecraft:nonsense"},probability:0.5f},{particle:{type:"minecraft:block",block_state:{Name:"minecraft:stone"}},probability:0.5f},{particle:{type:"minecraft:ash"},probability:"x"},{particle:{type:"minecraft:dust",color:16711680,scale:99.0f},probability:0.5f}]}"#,
    );
    assert_eq!(odd.ambient.len(), 1);
    assert!(matches!(odd.ambient[0].0, Particle::Dust { scale, .. } if scale == 4.0));
}

#[test]
fn a_biome_without_the_attribute_inherits_the_dimension_list() {
    let dimension = attrs(
        r#"{"minecraft:visual/ambient_particles":[{particle:{type:"minecraft:ash"},probability:0.5f}]}"#,
    );
    let biome = Visual::default();
    assert_eq!(biome.over(&dimension).ambient.len(), 1);
    let replacing = attrs(
        r#"{"minecraft:visual/ambient_particles":[{particle:{type:"minecraft:white_ash"},probability:0.1f}]}"#,
    );
    assert_eq!(
        replacing.over(&dimension).ambient,
        vec![(simple("white_ash"), 0.1)]
    );
}

#[test]
fn limiter_overrides_match_vanilla_types() {
    assert!(overrides_limiter(&simple("electric_spark")));
    assert!(overrides_limiter(&simple("campfire_cosy_smoke")));
    assert!(!overrides_limiter(&simple("smoke")));
    assert!(!overrides_limiter(&ids::REDSTONE_DUST));
}

#[test]
fn marker_target_spawns_block_markers_only_for_that_block() {
    let barrier = state("barrier", &[]);
    let mut world = World::new();
    world.set([2, 3, 4], barrier);
    world.set([5, 3, 4], state("stone", &[]));
    let mut animator = Animator::new(data(), 7);
    let mut rec = Rec::default();
    animator.sample_at([2, 3, 4], &world, &mut rec, false);
    let markers = |rec: &Rec| {
        rec.particles
            .iter()
            .filter(|(p, _)| matches!(p, Particle::BlockState { .. }))
            .count()
    };
    assert_eq!(markers(&rec), 0);
    animator.set_marker(Some(block_id("barrier")));
    animator.sample_at([2, 3, 4], &world, &mut rec, false);
    animator.sample_at([5, 3, 4], &world, &mut rec, false);
    assert_eq!(markers(&rec), 1);
    let (particle, (pos, vel, _)) = rec
        .particles
        .iter()
        .find(|(p, _)| matches!(p, Particle::BlockState { .. }))
        .unwrap();
    assert_eq!(
        *particle,
        Particle::BlockState {
            kind: mcr_protocol::types::particle::BlockParticleKind::BlockMarker,
            state: barrier as i32,
        }
    );
    assert_eq!((*pos, *vel), ([2.5, 3.5, 4.5], [0.0; 3]));
}

#[test]
fn campfire_food_smokes_over_filled_slots_only() {
    let mut a = animator();
    let w = World::new();
    let mut rec = Rec::default();
    for _ in 0..2000 {
        a.campfire_food_tick(AT, 2, [true, false, false, false], &w, &mut rec);
    }
    let smoke = rec.only("smoke");
    assert!(
        smoke.len() > 2000 / 5 * 4 / 2 && smoke.len() % 4 == 0,
        "{}",
        smoke.len()
    );
    let (x, z) = (f64::from(AT[0]) + 0.8125, f64::from(AT[2]) + 0.8125);
    assert!(smoke.iter().all(|(pos, vel, always)| {
        (pos[0] - x).abs() < 1e-9
            && (pos[1] - (f64::from(AT[1]) + 0.5)).abs() < 1e-9
            && (pos[2] - z).abs() < 1e-9
            && *vel == [0.0, 5.0e-4, 0.0]
            && !always
    }));
    let mut none = Rec::default();
    for _ in 0..200 {
        a.campfire_food_tick(AT, 0, [false; 4], &w, &mut none);
    }
    assert!(none.particles.is_empty());
}

#[test]
fn campfire_states_know_their_facing() {
    let table = Table::build(data());
    let facing = |f: &str| {
        table
            .campfire_rotation(state("campfire", &[("lit", "true"), ("facing", f)]))
            .unwrap()
    };
    assert_eq!(
        [
            facing("south"),
            facing("west"),
            facing("north"),
            facing("east")
        ],
        [0, 1, 2, 3]
    );
    assert_eq!(
        table.campfire_rotation(state("campfire", &[("lit", "false")])),
        None
    );
}

struct Timed(i64);

impl AnimateWorld for Timed {
    fn state(&self, _: i32, _: i32, _: i32) -> Option<u32> {
        None
    }

    fn game_time(&self) -> i64 {
        self.0
    }
}

#[test]
fn spawner_cage_smokes_and_flames_at_one_point_inside_the_block() {
    let mut a = animator();
    let mut rec = Rec::default();
    for _ in 0..50 {
        a.spawner_tick(AT, &Timed(0), &mut rec);
    }
    let (smoke, flame) = (rec.only("smoke"), rec.only("flame"));
    assert_eq!((smoke.len(), flame.len()), (50, 50));
    for (s, f) in smoke.iter().zip(&flame) {
        assert_eq!(s.0, f.0);
        for (c, base) in s.0.iter().zip(AT) {
            assert!((0.0..1.0).contains(&(c - f64::from(base))));
        }
    }
}

#[test]
fn trial_spawner_particles_follow_the_state() {
    let mut a = animator();
    let run = |a: &mut Animator, state, ominous, time| {
        let mut rec = Rec::default();
        for _ in 0..400 {
            a.trial_spawner_tick(AT, state, ominous, &Timed(time), &mut rec, true);
        }
        rec
    };
    let inactive = run(&mut a, TrialState::Inactive, false, 1);
    assert!(inactive.particles.is_empty() && inactive.cues.is_empty());

    let waiting = run(&mut a, TrialState::WaitingForPlayers, false, 1);
    let n = waiting.count("small_flame");
    assert!(n > 120 && n < 280 && waiting.count("smoke") == 0, "{n}");
    assert!(waiting.sounds("block.trial_spawner.ambient") > 0);

    let active = run(&mut a, TrialState::Active, true, 1);
    assert_eq!(active.count("smoke"), 400);
    assert_eq!(active.count("soul_fire_flame"), 400);
    assert!(active.sounds("block.trial_spawner.ambient_ominous") > 0);

    let cooldown = run(&mut a, TrialState::Cooldown, false, 1);
    assert!(cooldown.count("smoke") < 200 && cooldown.cues.is_empty());
    let mut burst = Rec::default();
    a.trial_spawner_tick(
        AT,
        TrialState::Cooldown,
        false,
        &Timed(40),
        &mut burst,
        true,
    );
    assert!(burst.count("smoke") >= 20);
}

#[test]
fn vault_links_players_at_the_keyhole_once_a_second() {
    let mut a = animator();
    let players = [
        VaultPlayer {
            block: [AT[0] + 2, AT[1], AT[2]],
            center: [
                f64::from(AT[0]) + 2.5,
                f64::from(AT[1]) + 1.0,
                f64::from(AT[2]) + 0.5,
            ],
        },
        VaultPlayer {
            block: [AT[0] + 20, AT[1], AT[2]],
            center: [f64::from(AT[0]) + 20.5, f64::from(AT[1]), f64::from(AT[2])],
        },
    ];
    let mut rec = Rec::default();
    a.vault_tick(
        AT,
        2,
        false,
        true,
        &players,
        4.5,
        &Timed(200),
        &mut rec,
        true,
    );
    let links = rec.only("vault_connection");
    assert!((2..=5).contains(&links.len()), "{}", links.len());
    for (pos, vel, _) in &links {
        assert_eq!(
            *pos,
            [
                f64::from(AT[0]) + 0.5,
                f64::from(AT[1]) + 1.75,
                f64::from(AT[2])
            ]
        );
        assert!(vel[0] > 1.0 && vel[0] < 3.0);
    }
    let mut quiet = Rec::default();
    a.vault_tick(
        AT,
        2,
        false,
        true,
        &players,
        4.5,
        &Timed(201),
        &mut quiet,
        true,
    );
    assert_eq!(quiet.count("vault_connection"), 0);
}

#[test]
fn vault_idles_with_smoke_and_flames_only_when_it_shows_an_item() {
    let mut a = animator();
    let idle = |a: &mut Animator, item: bool, ominous: bool| {
        let mut rec = Rec::default();
        for _ in 0..400 {
            a.vault_tick(AT, 2, ominous, item, &[], 4.5, &Timed(1), &mut rec, true);
        }
        rec
    };
    let empty = idle(&mut a, false, false);
    assert!(empty.count("smoke") > 120 && empty.count("small_flame") == 0);
    assert!(empty.cues.is_empty());
    let shown = idle(&mut a, true, true);
    assert_eq!(shown.count("soul_fire_flame"), shown.count("smoke"));
    assert!(shown.sounds("block.vault.ambient") > 0);
}

#[test]
fn conduit_sends_nautilus_particles_from_above_the_block_and_at_the_target() {
    let mut a = animator();
    let frame: Vec<[i32; 3]> = (0..42)
        .map(|i| [AT[0] + 2, AT[1] + i % 3 - 1, AT[2] + i])
        .collect();
    let target = ConduitTarget {
        eye: [1.0, 2.0, 3.0],
        width: 0.6,
        height: 1.95,
    };
    let mut rec = Rec::default();
    for t in 0..500 {
        a.conduit_tick(AT, t, &frame, Some(target), &Timed(0), &mut rec);
    }
    let n = rec.only("nautilus");
    let at_target = n.iter().filter(|p| p.0 == target.eye).count();
    assert_eq!(at_target, 500);
    let from_frame = n.len() - at_target;
    assert!(from_frame > 200 && from_frame < 600, "{from_frame}");
    for (pos, _, _) in n.iter().filter(|p| p.0 != target.eye) {
        assert!((pos[0] - (f64::from(AT[0]) + 0.5)).abs() < 1e-9);
        assert!(pos[1] >= f64::from(AT[1]) + 1.5 && pos[1] <= f64::from(AT[1]) + 2.1 + 1e-9);
    }
    let mut none = Rec::default();
    a.conduit_tick(AT, 0, &[], None, &Timed(0), &mut none);
    assert!(none.particles.is_empty());
}

#[test]
fn trial_spawner_and_vault_states_expose_their_block_properties() {
    let table = Table::build(data());
    let trial = state(
        "trial_spawner",
        &[("trial_spawner_state", "cooldown"), ("ominous", "true")],
    );
    assert_eq!(
        table.trial_spawner(trial),
        Some((TrialState::Cooldown, true))
    );
    let vault = state("vault", &[("facing", "east"), ("ominous", "false")]);
    assert_eq!(table.vault(vault), Some((5, false)));
    assert_eq!(table.vault(trial), None);
}
