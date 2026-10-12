use mcr_audio::{FastRandom, SoundSource};
use mcr_data::Version;
use mcr_mesh::{OffsetSpec, block_offset};
use mcr_protocol::types::particle::Particle;

use crate::ids::{REDSTONE_DUST, SCULK_TO_REDSTONE, falling_dust, tinted_leaves};
use crate::table::{
    DIRS, HORIZONTAL, Info, Kind, LEAVES, LeafParticle, Table, flag, fluid, opposite, tag,
};
use crate::{
    AnimateWorld, Animator, Cue, Height, Ids, MAX_PARTICLES_PER_TICK, MAX_SOUNDS_PER_TICK, Sink,
    Stats,
};

const ZERO: [f64; 3] = [0.0; 3];
const UP: usize = 1;
const AZALEA_TINT: i32 = -9_399_763;
const BIRCH_TINT: i32 = 0xFF80_A755u32 as i32;
const ENCHANT_OFFSETS: [[i32; 3]; 32] = enchant_offsets();

const fn enchant_offsets() -> [[i32; 3]; 32] {
    let mut out = [[0; 3]; 32];
    let mut n = 0;
    let mut x = -2;
    while x <= 2 {
        let mut y = 0;
        while y <= 1 {
            let mut z = -2;
            while z <= 2 {
                if x == 2 || x == -2 || z == 2 || z == -2 {
                    out[n] = [x, y, z];
                    n += 1;
                }
                z += 1;
            }
            y += 1;
        }
        x += 1;
    }
    out
}

const CANDLE_OFFSETS: [&[[f64; 3]]; 4] = [
    &[[8.0, 8.0, 8.0]],
    &[[6.0, 7.0, 8.0], [10.0, 8.0, 7.0]],
    &[[8.0, 5.0, 10.0], [6.0, 7.0, 8.0], [9.0, 8.0, 7.0]],
    &[
        [7.0, 5.0, 9.0],
        [10.0, 7.0, 9.0],
        [6.0, 7.0, 6.0],
        [9.0, 8.0, 6.0],
    ],
];
const CANDLE_CAKE_OFFSET: [[f64; 3]; 1] = [[8.0, 16.0, 8.0]];

fn flame(ids: &Ids, which: u8) -> &Particle {
    match which {
        1 => &ids.soul_fire_flame,
        2 => &ids.copper_fire_flame,
        _ => &ids.flame,
    }
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

pub(crate) struct Scan<'a> {
    data: &'static Version,
    table: &'a Table,
    ids: &'a Ids,
    rng: &'a mut FastRandom,
    world: &'a dyn AnimateWorld,
    sink: &'a mut dyn Sink,
    wither_rose: OffsetSpec,
    dripstone: OffsetSpec,
    sounds: bool,
    stats: Stats,
}

impl<'a> Scan<'a> {
    pub(crate) fn new(
        animator: &'a mut Animator,
        world: &'a dyn AnimateWorld,
        sink: &'a mut dyn Sink,
        sounds: bool,
    ) -> Self {
        Self {
            data: animator.data,
            table: &animator.table,
            ids: &animator.ids,
            rng: &mut animator.rng,
            world,
            sink,
            wither_rose: animator.wither_rose,
            dripstone: animator.dripstone,
            sounds,
            stats: Stats::default(),
        }
    }

    pub(crate) fn finish(self) -> Stats {
        self.stats
    }

    pub(crate) fn random_offset(&mut self, center: [i32; 3], radius: u32) -> [i32; 3] {
        let mut p = center;
        for c in &mut p {
            *c += self.rng.below(radius) as i32 - self.rng.below(radius) as i32;
        }
        p
    }

    fn d(&mut self) -> f64 {
        self.rng.f64()
    }

    fn f(&mut self) -> f64 {
        f64::from(self.rng.f32())
    }

    fn chance(&mut self, bound: u32) -> bool {
        self.rng.below(bound) == 0
    }

    fn gaussian(&mut self) -> f64 {
        let u1 = 1.0 - self.rng.f64();
        let u2 = self.rng.f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    fn particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3]) {
        self.emit(particle, pos, vel, false);
    }

    fn always(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3]) {
        self.emit(particle, pos, vel, true);
    }

    fn emit(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3], always: bool) {
        if self.stats.particles >= MAX_PARTICLES_PER_TICK {
            return;
        }
        self.stats.particles += 1;
        self.sink.particle(particle, pos, vel, always);
    }

    fn sound(
        &mut self,
        event: &'static str,
        pos: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        source: SoundSource,
    ) {
        if !self.sounds || self.stats.sounds >= MAX_SOUNDS_PER_TICK {
            return;
        }
        self.stats.sounds += 1;
        self.sink.sound(&Cue {
            event,
            pos,
            volume,
            pitch,
            source,
        });
    }

    fn info(&self, p: [i32; 3]) -> &'a Info {
        match self.world.state(p[0], p[1], p[2]) {
            Some(s) => self.table.info(s),
            None => &Info::AIR,
        }
    }

    fn at(&self, p: [i32; 3], dir: usize) -> &'a Info {
        let d = DIRS[dir];
        self.info([p[0] + d[0], p[1] + d[1], p[2] + d[2]])
    }

    pub(crate) fn sample(&mut self, p: [i32; 3]) {
        self.stats.samples += 1;
        let Some(state) = self.world.state(p[0], p[1], p[2]) else {
            return;
        };
        let info = self.table.info(state);
        if info.kind != Kind::None {
            self.block(p, state, info);
        }
        if info.fluid != fluid::NONE {
            self.fluid(p, info);
        }
        if !info.has(flag::FULL_COLLISION) && self.world.has_ambient_particles() {
            self.ambient(p);
        }
    }

    fn ambient(&mut self, p: [i32; 3]) {
        let world = self.world;
        for (particle, probability) in world.ambient_particles(p[0], p[1], p[2]) {
            if self.rng.f32() <= *probability {
                let at = [
                    f64::from(p[0]) + self.d(),
                    f64::from(p[1]) + self.d(),
                    f64::from(p[2]) + self.d(),
                ];
                self.particle(particle, at, ZERO);
            }
        }
    }

    fn block(&mut self, p: [i32; 3], state: u16, info: &'a Info) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        match info.kind {
            Kind::None => {}
            Kind::Torch => {
                let at = [x + 0.5, y + 0.7, z + 0.5];
                self.particle(&ids.smoke, at, ZERO);
                self.particle(flame(ids, info.arg), at, ZERO);
            }
            Kind::WallTorch => {
                let o = DIRS[opposite(usize::from(info.aux))];
                let at = [
                    x + 0.5 + 0.27 * f64::from(o[0]),
                    y + 0.7 + 0.22,
                    z + 0.5 + 0.27 * f64::from(o[2]),
                ];
                self.particle(&ids.smoke, at, ZERO);
                self.particle(flame(ids, info.arg), at, ZERO);
            }
            Kind::RedstoneTorch => {
                let at = [
                    x + 0.5 + (self.d() - 0.5) * 0.2,
                    y + 0.7 + (self.d() - 0.5) * 0.2,
                    z + 0.5 + (self.d() - 0.5) * 0.2,
                ];
                self.particle(&REDSTONE_DUST, at, ZERO);
            }
            Kind::RedstoneWallTorch => {
                let o = DIRS[opposite(usize::from(info.aux))];
                let at = [
                    x + 0.5 + (self.d() - 0.5) * 0.2 + 0.27 * f64::from(o[0]),
                    y + 0.7 + (self.d() - 0.5) * 0.2 + 0.22,
                    z + 0.5 + (self.d() - 0.5) * 0.2 + 0.27 * f64::from(o[2]),
                ];
                self.particle(&REDSTONE_DUST, at, ZERO);
            }
            Kind::Candle => {
                let offsets = CANDLE_OFFSETS[usize::from(info.arg.clamp(1, 4)) - 1];
                self.candle(p, offsets);
            }
            Kind::CandleCake => self.candle(p, &CANDLE_CAKE_OFFSET),
            Kind::Campfire => {
                if self.chance(10) {
                    let v = 0.5 + self.f() as f32;
                    let pitch = self.f() as f32 * 0.7 + 0.6;
                    self.sound(
                        "block.campfire.crackle",
                        Some([x + 0.5, y + 0.5, z + 0.5]),
                        v,
                        pitch,
                        SoundSource::Blocks,
                    );
                }
                if info.arg != 0 && self.chance(5) {
                    let vel = [self.f() / 2.0, 5.0e-5, self.f() / 2.0];
                    self.particle(&ids.lava, [x + 0.5, y + 0.5, z + 0.5], vel);
                }
            }
            Kind::Fire | Kind::SoulFire => self.fire(p, info.kind == Kind::SoulFire),
            Kind::Furnace | Kind::BlastFurnace => self.furnace(p, info),
            Kind::Smoker => {
                if self.d() < 0.1 {
                    self.sound(
                        "block.smoker.smoke",
                        Some([x + 0.5, y, z + 0.5]),
                        1.0,
                        1.0,
                        SoundSource::Blocks,
                    );
                }
                self.particle(&ids.smoke, [x + 0.5, y + 1.1, z + 0.5], ZERO);
            }
            Kind::BrewingStand => {
                let at = [
                    x + 0.4 + self.f() * 0.2,
                    y + 0.7 + self.f() * 0.3,
                    z + 0.4 + self.f() * 0.2,
                ];
                self.particle(&ids.smoke, at, ZERO);
            }
            Kind::Beehive => {
                if self.rng.f32() >= 0.3 {
                    let below = self.at(p, 0);
                    if !below.has(flag::FULL_COLLISION) && below.fluid == fluid::NONE {
                        let at = [x + self.d(), y - 0.05, z + self.d()];
                        self.particle(&ids.dripping_honey, at, ZERO);
                    }
                }
            }
            Kind::Brushable => self.falling_dust(p, state),
            Kind::FallingBlock => self.falling_dust(p, state),
            Kind::Sand => {
                self.falling_dust(p, state);
                self.sand_sound(p);
            }
            Kind::BubbleColumn => self.bubble_column(p, info.arg != 0),
            Kind::CreakingHeart => {
                if self.world.creaking_active() && self.chance(16) && self.logs_all_around(p) {
                    self.sound(
                        "block.creaking_heart.idle",
                        Some([x, y, z]),
                        1.0,
                        1.0,
                        SoundSource::Blocks,
                    );
                }
            }
            Kind::CryingObsidian => {
                if self.chance(5) {
                    let dir = self.rng.below(6) as usize;
                    if dir != UP {
                        let neighbour = self.at(p, dir);
                        if !info.has(flag::CAN_OCCLUDE) || !neighbour.sturdy(opposite(dir)) {
                            let step = DIRS[dir];
                            let axis = |s: i32, rng: &mut FastRandom| {
                                if s == 0 {
                                    rng.f64()
                                } else {
                                    0.5 + f64::from(s) * 0.6
                                }
                            };
                            let at = [
                                x + axis(step[0], self.rng),
                                y + axis(step[1], self.rng),
                                z + axis(step[2], self.rng),
                            ];
                            self.particle(&ids.dripping_obsidian_tear, at, ZERO);
                        }
                    }
                }
            }
            Kind::DriedGhast => self.dried_ghast(p, info.arg != 0),
            Kind::DeadBush => self.dead_bush(p),
            Kind::DryGrass => {
                if self.chance(200) && self.dry_vegetation_below(self.below(p)) {
                    self.sound(
                        "block.dry_grass.ambient",
                        None,
                        1.0,
                        1.0,
                        SoundSource::Ambient,
                    );
                }
            }
            Kind::EnchantingTable => self.enchanting_table(p),
            Kind::EnderChest => {
                for _ in 0..3 {
                    let flip_x = f64::from(self.rng.below(2) as i32 * 2 - 1);
                    let flip_z = f64::from(self.rng.below(2) as i32 * 2 - 1);
                    let at = [
                        x + 0.5 + 0.25 * flip_x,
                        y + self.f(),
                        z + 0.5 + 0.25 * flip_z,
                    ];
                    let vel = [
                        self.f() * flip_x,
                        (self.f() - 0.5) * 0.125,
                        self.f() * flip_z,
                    ];
                    self.particle(&ids.portal, at, vel);
                }
            }
            Kind::EndGateway => {
                let exposed = (0..6)
                    .filter(|&dir| !self.at(p, dir).sturdy(opposite(dir)))
                    .count();
                for _ in 0..exposed {
                    let mut at = [x + self.d(), y + self.d(), z + self.d()];
                    let mut vel = [
                        (self.d() - 0.5) * 0.5,
                        (self.d() - 0.5) * 0.5,
                        (self.d() - 0.5) * 0.5,
                    ];
                    let flip = f64::from(self.rng.below(2) as i32 * 2 - 1);
                    if self.rng.below(2) == 1 {
                        at[2] = z + 0.5 + 0.25 * flip;
                        vel[2] = self.f() * 2.0 * flip;
                    } else {
                        at[0] = x + 0.5 + 0.25 * flip;
                        vel[0] = self.f() * 2.0 * flip;
                    }
                    self.particle(&ids.portal, at, vel);
                }
            }
            Kind::EndPortal => {
                let at = [x + self.d(), y + 0.8, z + self.d()];
                self.particle(&ids.smoke, at, ZERO);
            }
            Kind::EndRod => {
                let dir = DIRS[usize::from(info.aux)];
                let at = [
                    x + 0.55 - self.f() * 0.1,
                    y + 0.55 - self.f() * 0.1,
                    z + 0.55 - self.f() * 0.1,
                ];
                let r = 0.4 - (self.f() + self.f()) * 0.4;
                if self.chance(5) {
                    let pos = [
                        at[0] + f64::from(dir[0]) * r,
                        at[1] + f64::from(dir[1]) * r,
                        at[2] + f64::from(dir[2]) * r,
                    ];
                    let vel = [
                        self.gaussian() * 0.005,
                        self.gaussian() * 0.005,
                        self.gaussian() * 0.005,
                    ];
                    self.particle(&ids.end_rod, pos, vel);
                }
            }
            Kind::Eyeblossom => {
                if self.chance(700) && self.at(p, 0).block == self.table.known.pale_moss_block {
                    self.sound(
                        "block.eyeblossom.idle",
                        Some([x, y, z]),
                        1.0,
                        1.0,
                        SoundSource::Ambient,
                    );
                }
            }
            Kind::FireflyBush => self.firefly_bush(p),
            Kind::HangingMoss => {
                if self.chance(500) {
                    let above = self.at(p, UP);
                    if self.table.tagged(above, tag::PALE_OAK_LOGS)
                        || above.block == self.table.known.pale_oak_leaves
                    {
                        self.sound(
                            "block.pale_hanging_moss.idle",
                            Some([x, y, z]),
                            1.0,
                            1.0,
                            SoundSource::Ambient,
                        );
                    }
                }
            }
            Kind::Leaves => self.leaves(p, info),
            Kind::Lever => {
                if self.rng.f32() < 0.25 {
                    let o = DIRS[usize::from(info.arg)];
                    let c = DIRS[usize::from(info.aux)];
                    let at = [0, 1, 2].map(|i| {
                        [x, y, z][i] + 0.5 + 0.1 * f64::from(o[i]) + 0.2 * f64::from(c[i])
                    });
                    self.particle(
                        &Particle::Dust {
                            color: 0xFF_0000,
                            scale: 0.5,
                        },
                        at,
                        ZERO,
                    );
                }
            }
            Kind::LightningRod => self.lightning_rod(p, info.arg),
            Kind::Mycelium => {
                if self.chance(10) {
                    let at = [x + self.d(), y + 1.1, z + self.d()];
                    self.particle(&ids.mycelium, at, ZERO);
                }
            }
            Kind::NetherPortal => self.nether_portal(p, info),
            Kind::PointedDripstone => self.dripstone(p),
            Kind::PotentSulfur => {
                if self.above_is_water_source(p) {
                    for _ in 0..2 {
                        let at = [x + self.f(), y + 1.0 + self.f(), z + self.f()];
                        self.always(&ids.sulfur_bubbles, at, ZERO);
                    }
                    if self.chance(10) {
                        self.sound(
                            "block.potent_sulfur.noxious_gas",
                            Some([x, y, z]),
                            1.0,
                            1.0,
                            SoundSource::Ambient,
                        );
                    }
                }
            }
            Kind::RedstoneOre => {
                for (dir, step) in DIRS.iter().enumerate() {
                    if !self.at(p, dir).has(flag::SOLID_RENDER) {
                        let axis = |i: usize, rng: &mut FastRandom| {
                            if step[i] != 0 {
                                0.5 + 0.5625 * f64::from(step[i])
                            } else {
                                f64::from(rng.f32())
                            }
                        };
                        let at = [
                            x + axis(0, self.rng),
                            y + axis(1, self.rng),
                            z + axis(2, self.rng),
                        ];
                        self.particle(&REDSTONE_DUST, at, ZERO);
                    }
                }
            }
            Kind::RedstoneWire => self.redstone_wire(p, info),
            Kind::Repeater => {
                let dir = DIRS[usize::from(info.arg)];
                let at = [
                    x + 0.5 + (self.d() - 0.5) * 0.2,
                    y + 0.4 + (self.d() - 0.5) * 0.2,
                    z + 0.5 + (self.d() - 0.5) * 0.2,
                ];
                let mut offset = -5.0f32;
                if self.rng.below(2) == 1 {
                    offset = (i32::from(info.aux) * 2 - 1) as f32;
                }
                let offset = f64::from(offset / 16.0);
                self.particle(
                    &REDSTONE_DUST,
                    [
                        at[0] + offset * f64::from(dir[0]),
                        at[1],
                        at[2] + offset * f64::from(dir[2]),
                    ],
                    ZERO,
                );
            }
            Kind::RespawnAnchor => {
                if self.chance(100) {
                    self.sound(
                        "block.respawn_anchor.ambient",
                        Some([x + 0.5, y + 0.5, z + 0.5]),
                        1.0,
                        1.0,
                        SoundSource::Blocks,
                    );
                }
                let at = [
                    x + 0.5 + (0.5 - self.d()),
                    y + 1.0,
                    z + 0.5 + (0.5 - self.d()),
                ];
                let ya = self.f() * 0.04;
                self.particle(&ids.reverse_portal, at, [0.0, ya, 0.0]);
            }
            Kind::SculkSensor => {
                let dir = self.rng.below(6) as usize;
                if dir != 0 && dir != UP {
                    let s = DIRS[dir];
                    let at = [
                        x + 0.5
                            + if s[0] == 0 {
                                0.5 - self.d()
                            } else {
                                f64::from(s[0]) * 0.6
                            },
                        y + 0.25,
                        z + 0.5
                            + if s[2] == 0 {
                                0.5 - self.d()
                            } else {
                                f64::from(s[2]) * 0.6
                            },
                    ];
                    let ya = self.f() * 0.04;
                    self.particle(&SCULK_TO_REDSTONE, at, [0.0, ya, 0.0]);
                }
            }
            Kind::SporeBlossom => self.spore_blossom(p),
            Kind::WetSponge => self.wet_sponge(p, info),
            Kind::WitherRose => {
                let [ox, _, oz] = block_offset(self.wither_rose, p[0], p[2]);
                let cx = x + 0.5 + f64::from(ox);
                let cz = z + 0.5 + f64::from(oz);
                for _ in 0..3 {
                    if self.rng.below(2) == 1 {
                        let at = [
                            cx + self.d() / 5.0,
                            y + (0.5 - self.d()),
                            cz + self.d() / 5.0,
                        ];
                        self.particle(&ids.smoke, at, ZERO);
                    }
                }
            }
        }
    }

    fn below(&self, p: [i32; 3]) -> [i32; 3] {
        [p[0], p[1] - 1, p[2]]
    }

    fn candle(&mut self, p: [i32; 3], offsets: &[[f64; 3]]) {
        let ids = self.ids;
        for o in offsets {
            let at = [
                f64::from(p[0]) + o[0] / 16.0,
                f64::from(p[1]) + o[1] / 16.0,
                f64::from(p[2]) + o[2] / 16.0,
            ];
            let chance = self.rng.f32();
            if chance < 0.3 {
                self.particle(&ids.smoke, at, ZERO);
                if chance < 0.17 {
                    let volume = 1.0 + self.rng.f32();
                    let pitch = self.rng.f32() * 0.7 + 0.3;
                    self.sound(
                        "block.candle.ambient",
                        Some([at[0] + 0.5, at[1] + 0.5, at[2] + 0.5]),
                        volume,
                        pitch,
                        SoundSource::Blocks,
                    );
                }
            }
            self.particle(&ids.small_flame, at, ZERO);
        }
    }

    fn fire(&mut self, p: [i32; 3], soul: bool) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        if self.chance(24) {
            let volume = 1.0 + self.rng.f32();
            let pitch = self.rng.f32() * 0.7 + 0.3;
            self.sound(
                "block.fire.ambient",
                Some([x + 0.5, y + 0.5, z + 0.5]),
                volume,
                pitch,
                SoundSource::Blocks,
            );
        }
        let can_burn = |info: &Info| soul || info.has(flag::FLAMMABLE);
        let below = self.at(p, 0);
        if can_burn(below) || below.sturdy(UP) {
            for _ in 0..3 {
                let at = [x + self.d(), y + self.d() * 0.5 + 0.5, z + self.d()];
                self.particle(&ids.large_smoke, at, ZERO);
            }
            return;
        }
        // west, east, north, south, above
        for dir in [4usize, 5, 2, 3, 1] {
            if !can_burn(self.at(p, dir)) {
                continue;
            }
            for _ in 0..2 {
                let tenth = |rng: &mut FastRandom| f64::from(0.1f32) * rng.f64();
                let at = match dir {
                    4 => [x + tenth(self.rng), y + self.d(), z + self.d()],
                    5 => [x + 1.0 - tenth(self.rng), y + self.d(), z + self.d()],
                    2 => [x + self.d(), y + self.d(), z + tenth(self.rng)],
                    3 => [x + self.d(), y + self.d(), z + 1.0 - tenth(self.rng)],
                    _ => [x + self.d(), y + 1.0 - tenth(self.rng), z + self.d()],
                };
                self.particle(&ids.large_smoke, at, ZERO);
            }
        }
    }

    fn furnace(&mut self, p: [i32; 3], info: &Info) {
        let ids = self.ids;
        let blast = info.kind == Kind::BlastFurnace;
        let [x, y, z] = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ];
        if self.d() < 0.1 {
            let event = if blast {
                "block.blastfurnace.fire_crackle"
            } else {
                "block.furnace.fire_crackle"
            };
            self.sound(event, Some([x, y, z]), 1.0, 1.0, SoundSource::Blocks);
        }
        let dir = DIRS[usize::from(info.aux)];
        let ss = self.d() * 0.6 - 0.3;
        let dx = if dir[0] != 0 {
            f64::from(dir[0]) * 0.52
        } else {
            ss
        };
        let height = if blast { 9.0 } else { 6.0 };
        let dy = self.d() * height / 16.0;
        let dz = if dir[2] != 0 {
            f64::from(dir[2]) * 0.52
        } else {
            ss
        };
        let at = [x + dx, y + dy, z + dz];
        self.particle(&ids.smoke, at, ZERO);
        if !blast {
            self.particle(&ids.flame, at, ZERO);
        }
    }

    fn falling_dust(&mut self, p: [i32; 3], state: u16) {
        if self.chance(16) && self.at(p, 0).has(flag::FREE_TO_FALL) {
            let at = [
                f64::from(p[0]) + self.d(),
                f64::from(p[1]) - 0.05,
                f64::from(p[2]) + self.d(),
            ];
            self.particle(&falling_dust(state), at, ZERO);
        }
    }

    fn bubble_column(&mut self, p: [i32; 3], drag_down: bool) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        let event = if drag_down {
            self.always(&ids.current_down, [x + 0.5, y + 0.8, z], ZERO);
            "block.bubble_column.whirlpool_ambient"
        } else {
            let up = [0.0, 0.04, 0.0];
            self.always(&ids.bubble_column_up, [x + 0.5, y, z + 0.5], up);
            let at = [x + self.f(), y + self.f(), z + self.f()];
            self.always(&ids.bubble_column_up, at, up);
            "block.bubble_column.upwards_ambient"
        };
        if self.chance(200) {
            let volume = 0.2 + self.rng.f32() * 0.2;
            let pitch = 0.9 + self.rng.f32() * 0.15;
            self.sound(event, Some([x, y, z]), volume, pitch, SoundSource::Blocks);
        }
    }

    fn logs_all_around(&self, p: [i32; 3]) -> bool {
        (0..6).all(|dir| self.table.tagged(self.at(p, dir), tag::PALE_OAK_LOGS))
    }

    fn dried_ghast(&mut self, p: [i32; 3], waterlogged: bool) {
        let ids = self.ids;
        let c = p.map(|v| f64::from(v) + 0.5);
        if !waterlogged {
            if self.chance(40) && self.table.tagged(self.at(p, 0), tag::GHAST_TRIGGER) {
                self.sound(
                    "block.dried_ghast.ambient",
                    Some(c),
                    1.0,
                    1.0,
                    SoundSource::Blocks,
                );
            }
            if self.chance(6) {
                self.particle(&ids.white_smoke, c, [0.0, 0.02, 0.0]);
            }
        } else {
            if self.chance(40) {
                self.sound(
                    "block.dried_ghast.ambient_water",
                    Some(c),
                    1.0,
                    1.0,
                    SoundSource::Blocks,
                );
            }
            if self.chance(6) {
                let at = [
                    c[0] + (self.f() * 2.0 - 1.0) / 3.0,
                    c[1] + 0.4,
                    c[2] + (self.f() * 2.0 - 1.0) / 3.0,
                ];
                let vel = [0.0, self.f(), 0.0];
                self.particle(&ids.happy_villager, at, vel);
            }
        }
    }

    fn dry_vegetation_below(&self, below: [i32; 3]) -> bool {
        self.table.tagged(self.info(below), tag::DESERT_VEGETATION)
            && self
                .table
                .tagged(self.info(self.below(below)), tag::DESERT_VEGETATION)
    }

    fn dead_bush(&mut self, p: [i32; 3]) {
        if !self.chance(130) {
            return;
        }
        let below = self.at(p, 0);
        let sandy =
            below.block == self.table.known.red_sand || self.table.tagged(below, tag::TERRACOTTA);
        if sandy && self.rng.below(3) != 0 {
            return;
        }
        if self.dry_vegetation_below(self.below(p)) {
            self.sound(
                "block.deadbush.idle",
                Some(p.map(f64::from)),
                1.0,
                1.0,
                SoundSource::Ambient,
            );
        }
    }

    fn sand_sound(&mut self, p: [i32; 3]) {
        if self.at(p, UP).has(flag::PLAIN_AIR) && self.rng.below(2100) == 0 && self.sand_around(p) {
            self.sound(
                "block.sand.idle",
                Some(p.map(f64::from)),
                1.0,
                1.0,
                SoundSource::Ambient,
            );
        }
    }

    fn sand_around(&self, p: [i32; 3]) -> bool {
        let mut found = 0;
        let mut checked = 0;
        for dir in HORIZONTAL {
            let d = DIRS[dir];
            let column = [p[0] + d[0] * 8, p[1], p[2] + d[2] * 8];
            if self.sand_column(column) {
                found += 1;
                if found > 3 {
                    return true;
                }
            }
            checked += 1;
            if 4 - checked + found < 3 {
                return false;
            }
        }
        false
    }

    fn sand_column(&self, p: [i32; 3]) -> bool {
        let surface = self
            .world
            .height(Height::WorldSurface, p[0], p[2])
            .saturating_sub(1);
        let triggers = |info: &Info| self.table.tagged(info, tag::DESERT_SAND);
        if (surface - p[1]).abs() > 5 {
            let mut y = p[1] + 6;
            let mut above = self.info([p[0], y, p[2]]);
            y -= 1;
            for _ in 0..10 {
                let current = self.info([p[0], y, p[2]]);
                if above.has(flag::AIR) && triggers(current) {
                    return true;
                }
                above = current;
                y -= 1;
            }
            false
        } else {
            self.info([p[0], surface + 1, p[2]]).has(flag::AIR)
                && triggers(self.info([p[0], surface, p[2]]))
        }
    }

    fn enchanting_table(&mut self, p: [i32; 3]) {
        let ids = self.ids;
        for o in ENCHANT_OFFSETS {
            if !self.chance(16) {
                continue;
            }
            let shelf = self.info([p[0] + o[0], p[1] + o[1], p[2] + o[2]]);
            let between = self.info([p[0] + o[0] / 2, p[1] + o[1], p[2] + o[2] / 2]);
            if self.table.tagged(shelf, tag::POWER_PROVIDER)
                && self.table.tagged(between, tag::POWER_TRANSMITTER)
            {
                let at = [
                    f64::from(p[0]) + 0.5,
                    f64::from(p[1]) + 2.0,
                    f64::from(p[2]) + 0.5,
                ];
                let vel = [
                    f64::from(o[0]) + self.f() - 0.5,
                    f64::from(o[1]) - self.f() - 1.0,
                    f64::from(o[2]) + self.f() - 0.5,
                ];
                self.particle(&ids.enchant, at, vel);
            }
        }
    }

    fn firefly_bush(&mut self, p: [i32; 3]) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        if self.chance(30)
            && self.world.firefly_sounds(p[0], p[1], p[2])
            && self
                .world
                .height(Height::MotionBlockingNoLeaves, p[0], p[2])
                <= p[1]
        {
            self.sound(
                "block.firefly_bush.idle",
                Some([x + 0.5, y + 0.5, z + 0.5]),
                1.0,
                1.0,
                SoundSource::Ambient,
            );
        }
        let (block, sky) = self.world.light(p[0], p[1], p[2]);
        let brightness = block.max(sky.saturating_sub(self.world.sky_darken()));
        if brightness <= 13 && self.d() <= 0.7 {
            let at = [
                x + self.d() * 10.0 - 5.0,
                y + self.d() * 5.0,
                z + self.d() * 10.0 - 5.0,
            ];
            self.particle(&ids.firefly, at, ZERO);
        }
    }

    fn leaves(&mut self, p: [i32; 3], info: &Info) {
        let ids = self.ids;
        let leaf = LEAVES[usize::from(info.arg)].1;
        let below = self.at(p, 0);
        let [x, y, z] = p.map(f64::from);
        if self.world.raining_at(p[0], p[1] + 1, p[2])
            && self.rng.below(15) == 1
            && (!below.has(flag::CAN_OCCLUDE) || !below.sturdy(UP))
        {
            let at = [x + self.d(), y - 0.05, z + self.d()];
            self.particle(&ids.dripping_water, at, ZERO);
        }
        if leaf.poplar_sound && self.chance(300) {
            let mut logs = 0;
            let mut same = 0;
            for dir in HORIZONTAL {
                let n = self.at(p, dir);
                if self.table.tagged(n, tag::POPLAR_LEAF_SOUND) {
                    logs += 1;
                }
                if n.block == info.block {
                    same += 1;
                }
                if logs == 1 && same == 3 {
                    self.sound(
                        "block.poplar_leaves.ambient",
                        Some([x, y, z]),
                        1.0,
                        1.0,
                        SoundSource::Ambient,
                    );
                    break;
                }
            }
        }
        if leaf.particle != LeafParticle::None
            && self.rng.f32() < leaf.chance
            && !below.has(flag::TOP_FULL)
        {
            let at = [x + self.d(), y - 0.05, z + self.d()];
            let particle = match leaf.particle {
                LeafParticle::Tinted => tinted_leaves(
                    0xFF00_0000u32 as i32 | self.world.foliage_color(p[0], p[1], p[2]) as i32,
                ),
                LeafParticle::Birch => tinted_leaves(BIRCH_TINT),
                LeafParticle::Azalea => tinted_leaves(AZALEA_TINT),
                LeafParticle::Cherry => ids.cherry_leaves.clone(),
                LeafParticle::PaleOak => ids.pale_oak_leaves.clone(),
                LeafParticle::RedPoplar => ids.red_poplar_leaves.clone(),
                LeafParticle::OrangePoplar => ids.orange_poplar_leaves.clone(),
                LeafParticle::YellowPoplar => ids.yellow_poplar_leaves.clone(),
                LeafParticle::None => return,
            };
            self.particle(&particle, at, ZERO);
        }
    }

    fn lightning_rod(&mut self, p: [i32; 3], axis: u8) {
        let ids = self.ids;
        if !self.world.thundering() {
            return;
        }
        let roll = i64::from(self.rng.below(200));
        if roll > self.world.game_time().rem_euclid(200)
            || p[1]
                != self
                    .world
                    .height(Height::WorldSurface, p[0], p[2])
                    .saturating_sub(1)
        {
            return;
        }
        let c = p.map(|v| f64::from(v) + 0.5);
        let count = 1 + self.rng.below(2);
        for _ in 0..count {
            let mut at = c;
            let mut vel = ZERO;
            for i in 0..3 {
                let along = usize::from(axis) == i;
                at[i] += (self.d() * 2.0 - 1.0) * if along { 0.5 } else { 0.125 };
                if along {
                    vel[i] = self.d() * 2.0 - 1.0;
                }
            }
            self.particle(&ids.electric_spark, at, vel);
        }
    }

    fn nether_portal(&mut self, p: [i32; 3], info: &Info) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        if self.chance(100) {
            let pitch = self.rng.f32() * 0.4 + 0.8;
            self.sound(
                "block.portal.ambient",
                Some([x + 0.5, y + 0.5, z + 0.5]),
                0.5,
                pitch,
                SoundSource::Blocks,
            );
        }
        let along_x = self.at(p, 4).block == info.block || self.at(p, 5).block == info.block;
        for _ in 0..4 {
            let mut at = [x + self.d(), y + self.d(), z + self.d()];
            let mut vel = [
                (self.f() - 0.5) * 0.5,
                (self.f() - 0.5) * 0.5,
                (self.f() - 0.5) * 0.5,
            ];
            let flip = f64::from(self.rng.below(2) as i32 * 2 - 1);
            if along_x {
                at[2] = z + 0.5 + 0.25 * flip;
                vel[2] = self.f() * 2.0 * flip;
            } else {
                at[0] = x + 0.5 + 0.25 * flip;
                vel[0] = self.f() * 2.0 * flip;
            }
            self.particle(&ids.portal, at, vel);
        }
    }

    fn dripstone(&mut self, p: [i32; 3]) {
        let ids = self.ids;
        let roll = self.rng.f32();
        if roll > 0.12 {
            return;
        }
        let mut cursor = p;
        let mut root = None;
        for _ in 1..11 {
            cursor[1] += 1;
            let info = self.info(cursor);
            if info.block != self.table.known.pointed_dripstone {
                root = Some(cursor);
                break;
            }
            if !info.has(flag::DRIP_DOWN) {
                break;
            }
        }
        let Some(root) = root else { return };
        let above = [root[0], root[1] + 1, root[2]];
        let above_info = self.info(above);
        let fluid_kind = if above_info.block == self.table.known.mud
            && !self.world.water_evaporates(above[0], above[1], above[2])
        {
            fluid::WATER_SOURCE
        } else {
            above_info.fluid
        };
        let is_lava = fluid_kind == fluid::LAVA;
        let is_water = matches!(
            fluid_kind,
            fluid::WATER_SOURCE | fluid::WATER_FALLING | fluid::WATER_FLOWING
        );
        if !(roll < 0.02 || is_lava || is_water) {
            return;
        }
        let drip = if fluid_kind == fluid::NONE {
            self.world.dripstone_particle(above[0], above[1], above[2])
        } else if is_lava {
            Some(&ids.dripping_dripstone_lava)
        } else {
            Some(&ids.dripping_dripstone_water)
        };
        let Some(drip) = drip else { return };
        let [ox, _, oz] = block_offset(self.dripstone, p[0], p[2]);
        let at = [
            f64::from(p[0]) + 0.5 + f64::from(ox),
            f64::from(p[1]) + 0.25,
            f64::from(p[2]) + 0.5 + f64::from(oz),
        ];
        self.particle(drip, at, ZERO);
    }

    fn above_is_water_source(&self, p: [i32; 3]) -> bool {
        self.at(p, UP).fluid == fluid::WATER_SOURCE
    }

    fn redstone_wire(&mut self, p: [i32; 3], info: &Info) {
        let power = f32::from(info.arg) / 15.0;
        let red = power * 0.6 + if power > 0.0 { 0.4 } else { 0.3 };
        let green = (power * power * 0.7 - 0.5).clamp(0.0, 1.0);
        let blue = (power * power * 0.6 - 0.7).clamp(0.0, 1.0);
        let byte = |v: f32| (v * 255.0).round() as i32;
        let color = byte(red) << 16 | byte(green) << 8 | byte(blue);
        let dust = Particle::Dust { color, scale: 1.0 };
        for (i, dir) in [2usize, 5, 3, 4].into_iter().enumerate() {
            match info.aux >> (2 * i) & 3 {
                2 => {
                    self.along_line(p, &dust, dir, UP, -0.5, 0.5);
                    self.along_line(p, &dust, 0, dir, 0.0, 0.5);
                }
                1 => self.along_line(p, &dust, 0, dir, 0.0, 0.5),
                _ => self.along_line(p, &dust, 0, dir, 0.0, 0.3),
            }
        }
    }

    fn along_line(
        &mut self,
        p: [i32; 3],
        dust: &Particle,
        side: usize,
        along: usize,
        from: f32,
        to: f32,
    ) {
        let span = to - from;
        if self.rng.f32() >= 0.2 * span {
            return;
        }
        let on_line = f64::from(from + span * self.rng.f32());
        let (s, a) = (DIRS[side], DIRS[along]);
        let at = [0, 1, 2].map(|i| {
            f64::from(p[i])
                + 0.5
                + f64::from(0.4375f32) * f64::from(s[i])
                + on_line * f64::from(a[i])
        });
        self.particle(dust, at, ZERO);
    }

    fn spore_blossom(&mut self, p: [i32; 3]) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        let at = [x + self.d(), y + 0.7, z + self.d()];
        self.particle(&ids.falling_spore_blossom, at, ZERO);
        for _ in 0..14 {
            let q = [
                p[0] - 10 + self.rng.below(21) as i32,
                p[1] - self.rng.below(10) as i32,
                p[2] - 10 + self.rng.below(21) as i32,
            ];
            if !self.info(q).has(flag::FULL_COLLISION) {
                let at = [
                    f64::from(q[0]) + self.d(),
                    f64::from(q[1]) + self.d(),
                    f64::from(q[2]) + self.d(),
                ];
                self.particle(&ids.spore_blossom_air, at, ZERO);
            }
        }
    }

    fn wet_sponge(&mut self, p: [i32; 3], info: &Info) {
        let ids = self.ids;
        let dir = self.rng.below(6) as usize;
        if dir == UP {
            return;
        }
        let neighbour = self.at(p, dir);
        if info.has(flag::CAN_OCCLUDE) && neighbour.sturdy(opposite(dir)) {
            return;
        }
        let [mut x, mut y, mut z] = p.map(f64::from);
        if dir == 0 {
            y -= 0.05;
            x += self.d();
            z += self.d();
        } else {
            y += self.d() * 0.8;
            if dir >= 4 {
                z += self.d();
                x += if dir == 5 { 1.1 } else { 0.05 };
            } else {
                x += self.d();
                z += if dir == 3 { 1.1 } else { 0.05 };
            }
        }
        self.particle(&ids.dripping_water, [x, y, z], ZERO);
    }

    fn fluid(&mut self, p: [i32; 3], info: &Info) {
        let ids = self.ids;
        let [x, y, z] = p.map(f64::from);
        match info.fluid {
            fluid::LAVA => {
                let above = self.at(p, UP);
                if above.has(flag::AIR) && !above.has(flag::SOLID_RENDER) {
                    if self.chance(100) {
                        let at = [x + self.d(), y + 1.0, z + self.d()];
                        self.particle(&ids.lava, at, ZERO);
                        let volume = 0.2 + self.rng.f32() * 0.2;
                        let pitch = 0.9 + self.rng.f32() * 0.15;
                        self.sound(
                            "block.lava.pop",
                            Some(at),
                            volume,
                            pitch,
                            SoundSource::Ambient,
                        );
                    }
                    if self.chance(200) {
                        let volume = 0.2 + self.rng.f32() * 0.2;
                        let pitch = 0.9 + self.rng.f32() * 0.15;
                        self.sound(
                            "block.lava.ambient",
                            Some([x, y, z]),
                            volume,
                            pitch,
                            SoundSource::Ambient,
                        );
                    }
                }
            }
            fluid::WATER_FLOWING => {
                if self.chance(64) {
                    let volume = self.rng.f32() * 0.25 + 0.75;
                    let pitch = self.rng.f32() + 0.5;
                    self.sound(
                        "block.water.ambient",
                        Some([x + 0.5, y + 0.5, z + 0.5]),
                        volume,
                        pitch,
                        SoundSource::Ambient,
                    );
                }
            }
            _ => {
                if self.chance(10) {
                    let at = [x + self.d(), y + self.d(), z + self.d()];
                    self.particle(&ids.underwater, at, ZERO);
                }
            }
        }
        if self.chance(10) {
            let drip = if info.fluid == fluid::LAVA {
                &ids.dripping_lava
            } else {
                &ids.dripping_water
            };
            self.drip_below(p, info, drip);
        }
    }

    fn drip_below(&mut self, p: [i32; 3], info: &Info, drip: &Particle) {
        let below_pos = self.below(p);
        let Some(state) = self.world.state(below_pos[0], below_pos[1], below_pos[2]) else {
            return;
        };
        let below = self.table.info(state);
        if below.fluid != fluid::NONE {
            return;
        }
        let shape = self.data.collision_shape(u32::from(state));
        let top = shape
            .iter()
            .map(|b| b.max[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let [bx, by, bz] = below_pos.map(f64::from);
        if top < 1.0 {
            if info.sturdy(0) {
                self.spawn_fluid(bx, bx + 1.0, bz, bz + 1.0, by + 1.0 - 0.05, drip);
            }
            return;
        }
        if self.table.tagged(below, tag::IMPERMEABLE) {
            return;
        }
        let bottom = shape.iter().map(|b| b.min[1]).fold(f32::INFINITY, f32::min);
        let x1 = shape.iter().map(|b| b.min[0]).fold(f32::INFINITY, f32::min);
        let x2 = shape
            .iter()
            .map(|b| b.max[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let z1 = shape.iter().map(|b| b.min[2]).fold(f32::INFINITY, f32::min);
        let z2 = shape
            .iter()
            .map(|b| b.max[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let (x1, x2, z1, z2) = (
            bx + f64::from(x1),
            bx + f64::from(x2),
            bz + f64::from(z1),
            bz + f64::from(z2),
        );
        if bottom > 0.0 {
            self.spawn_fluid(x1, x2, z1, z2, by + f64::from(bottom) - 0.05, drip);
        } else {
            let under = self.at(below_pos, 0);
            let under_top = self
                .world
                .state(below_pos[0], below_pos[1] - 1, below_pos[2])
                .map_or(f32::NEG_INFINITY, |s| {
                    self.data
                        .collision_shape(u32::from(s))
                        .iter()
                        .map(|b| b.max[1])
                        .fold(f32::NEG_INFINITY, f32::max)
                });
            if under_top < 1.0 && under.fluid == fluid::NONE {
                self.spawn_fluid(x1, x2, z1, z2, by - 0.05, drip);
            }
        }
    }

    fn spawn_fluid(&mut self, x1: f64, x2: f64, z1: f64, z2: f64, y: f64, drip: &Particle) {
        let x = lerp(self.d(), x1, x2);
        let z = lerp(self.d(), z1, z2);
        self.particle(drip, [x, y, z], ZERO);
    }

    pub(crate) fn campfire_smoke(&mut self, p: [i32; 3], signal: bool) {
        let ids = self.ids;
        if self.rng.f32() >= 0.11 {
            return;
        }
        let particle = if signal {
            &ids.campfire_signal_smoke
        } else {
            &ids.campfire_cosy_smoke
        };
        let mut i = 0;
        while i < 8 && i < 2 + self.rng.below(2) {
            let sign = |rng: &mut FastRandom| if rng.below(2) == 1 { 1.0 } else { -1.0 };
            let x = f64::from(p[0]) + 0.5 + self.d() / 3.0 * sign(self.rng);
            let y = f64::from(p[1]) + self.d() + self.d();
            let z = f64::from(p[2]) + 0.5 + self.d() / 3.0 * sign(self.rng);
            self.always(particle, [x, y, z], [0.0, 0.07, 0.0]);
            i += 1;
        }
    }
}
