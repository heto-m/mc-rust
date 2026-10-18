use mcr_particles::PARTICLE_NAMES;
use mcr_protocol::types::particle::{BlockParticleKind, ColorParticleKind, Particle};

fn simple(name: &str) -> Particle {
    let id = PARTICLE_NAMES
        .iter()
        .position(|n| *n == name)
        .unwrap_or_else(|| panic!("unknown particle {name}"));
    Particle::Simple(id as i32)
}

macro_rules! particles {
    ($($field:ident = $name:literal),+ $(,)?) => {
        #[derive(Debug, Clone)]
        pub struct Ids {
            $(pub $field: Particle,)+
        }

        impl Ids {
            pub fn new() -> Self {
                Self { $($field: simple($name),)+ }
            }

            pub const NAMES: &'static [&'static str] = &[$($name),+];
        }
    };
}

particles! {
    smoke = "smoke",
    large_smoke = "large_smoke",
    white_smoke = "white_smoke",
    flame = "flame",
    soul_fire_flame = "soul_fire_flame",
    copper_fire_flame = "copper_fire_flame",
    small_flame = "small_flame",
    nautilus = "nautilus",
    vault_connection = "vault_connection",
    lava = "lava",
    dripping_water = "dripping_water",
    dripping_lava = "dripping_lava",
    dripping_honey = "dripping_honey",
    dripping_obsidian_tear = "dripping_obsidian_tear",
    dripping_dripstone_water = "dripping_dripstone_water",
    dripping_dripstone_lava = "dripping_dripstone_lava",
    falling_spore_blossom = "falling_spore_blossom",
    spore_blossom_air = "spore_blossom_air",
    portal = "portal",
    reverse_portal = "reverse_portal",
    end_rod = "end_rod",
    mycelium = "mycelium",
    enchant = "enchant",
    electric_spark = "electric_spark",
    underwater = "underwater",
    happy_villager = "happy_villager",
    firefly = "firefly",
    sulfur_bubbles = "sulfur_bubbles",
    current_down = "current_down",
    bubble_column_up = "bubble_column_up",
    cherry_leaves = "cherry_leaves",
    pale_oak_leaves = "pale_oak_leaves",
    red_poplar_leaves = "red_poplar_leaves",
    orange_poplar_leaves = "orange_poplar_leaves",
    yellow_poplar_leaves = "yellow_poplar_leaves",
    campfire_cosy_smoke = "campfire_cosy_smoke",
    campfire_signal_smoke = "campfire_signal_smoke",
}

impl Default for Ids {
    fn default() -> Self {
        Self::new()
    }
}

pub const REDSTONE_DUST: Particle = Particle::Dust {
    color: 0xFF_0000,
    scale: 1.0,
};

pub const SCULK_TO_REDSTONE: Particle = Particle::DustColorTransition {
    from: 3_790_560,
    to: 0xFF_0000,
    scale: 1.0,
};

pub fn falling_dust(state: u32) -> Particle {
    Particle::BlockState {
        kind: BlockParticleKind::FallingDust,
        state: state as i32,
    }
}

pub fn tinted_leaves(argb: i32) -> Particle {
    Particle::Color {
        kind: ColorParticleKind::TintedLeaves,
        argb,
    }
}

/// Vanilla particle types registered with `overrideLimiter`.
pub fn overrides_limiter(particle: &Particle) -> bool {
    let Particle::Simple(id) = particle else {
        return false;
    };
    usize::try_from(*id)
        .ok()
        .and_then(|i| PARTICLE_NAMES.get(i))
        .is_some_and(|n| LIMITER_OVERRIDES.contains(n))
}

const LIMITER_OVERRIDES: [&str; 25] = [
    "trial_spawner_detection",
    "trial_spawner_detection_ominous",
    "vault_connection",
    "damage_indicator",
    "elder_guardian",
    "explosion_emitter",
    "explosion",
    "gust",
    "gust_emitter_large",
    "gust_emitter_small",
    "sonic_boom",
    "sculk_charge_pop",
    "poof",
    "spit",
    "squid_ink",
    "sweep_attack",
    "campfire_cosy_smoke",
    "campfire_signal_smoke",
    "glow_squid_ink",
    "glow",
    "wax_on",
    "wax_off",
    "electric_spark",
    "scrape",
    "ominous_spawning",
];
