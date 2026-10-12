use mcr_protocol::types::ItemStack;
use mcr_protocol::types::entity_data::{EntityDataValue, Pose};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use uuid::Uuid;

/// Indices into the client's equipment array (`EquipmentSlot as usize`).
pub mod slot {
    pub const MAINHAND: usize = 0;
    pub const FEET: usize = 1;
    pub const LEGS: usize = 2;
    pub const CHEST: usize = 3;
    pub const HEAD: usize = 4;
    pub const OFFHAND: usize = 5;
    pub const BODY: usize = 6;
    pub const SADDLE: usize = 7;
}

/// Synched entity data by index; implemented for the client's maps.
pub trait EntityData {
    fn get(&self, index: u8) -> Option<&EntityDataValue>;
}

impl EntityData for FxHashMap<u8, EntityDataValue> {
    fn get(&self, index: u8) -> Option<&EntityDataValue> {
        FxHashMap::get(self, &index)
    }
}

impl EntityData for HashMap<u8, EntityDataValue> {
    fn get(&self, index: u8) -> Option<&EntityDataValue> {
        HashMap::get(self, &index)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoData;

impl EntityData for NoData {
    fn get(&self, _: u8) -> Option<&EntityDataValue> {
        None
    }
}

/// Names for registry-backed ids (variants, trim materials); the caller owns the synced registries.
pub trait Registries {
    /// Entry name such as `minecraft:pale` for `registry` such as `minecraft:wolf_variant`.
    fn entry_name(&self, registry: &str, id: i32) -> Option<&str>;
}

impl Registries for () {
    fn entry_name(&self, _: &str, _: i32) -> Option<&str> {
        None
    }
}

/// Skin pixels for a player, 64x64 or 64x32 legacy RGBA.
#[derive(Debug, Clone)]
pub struct Skin {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub slim: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerView<'a> {
    pub skin: Option<&'a Skin>,
    /// Client information model customisation bitmask (cape, jacket, sleeves, pants, hat).
    pub model_parts: u8,
    pub crouching: bool,
    pub fall_flying: bool,
    pub swimming: bool,
    pub using_item: bool,
    pub left_handed: bool,
}

/// Elytra flight state (`extractFlightData`, `ElytraAnimationState`), with the partial tick applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    /// `fallFlyingTimeInTicks`; 0 when not gliding.
    pub ticks: f32,
    /// `flyingYRot` in radians, when both movement and look have a horizontal part.
    pub y_rot: Option<f32>,
    /// `speedValue`: limb swing is divided by it.
    pub speed: f32,
    /// Left wing rotation (x, y, z) in radians.
    pub wings: [f32; 3],
}

impl Glide {
    pub const REST_WINGS: [f32; 3] = [
        std::f32::consts::PI / 12.0,
        0.0,
        -std::f32::consts::PI / 12.0,
    ];

    /// `AvatarRenderState.fallFlyingScale`.
    pub fn scale(&self) -> f32 {
        (self.ticks * self.ticks / 100.0).clamp(0.0, 1.0)
    }
}

impl Default for Glide {
    fn default() -> Self {
        Self {
            ticks: 0.0,
            y_rot: None,
            speed: 1.0,
            wings: Self::REST_WINGS,
        }
    }
}

/// Client-side animation state started by entity events, with the partial tick applied.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EventAnim {
    /// Sheep `eatAnimationTick`, 40 down to 0.
    pub eat: f32,
    /// Goat `lowerHeadTick`, 0 to 20.
    pub ram: f32,
    /// Ravager stun ticks left, 40 down to 0.
    pub stunned: f32,
    /// Ravager roar progress after the stun, 0 to 1.
    pub roar: f32,
    /// Warden tendril shiver, 1 down to 0.
    pub tendril: f32,
    /// Ticks since a keyframe animation was started: warden and creaking attacks.
    pub attack: Option<f32>,
    pub sonic_boom: Option<f32>,
    pub invulnerable: Option<f32>,
    pub peek: Option<f32>,
    pub hop: Option<f32>,
    /// Armor stand ticks since it was hit.
    pub wiggle: Option<f32>,
    /// Evoker fangs bite progress; 0 keeps them hidden.
    pub bite: f32,
    /// Ticks since the squid's tentacle phase was resynced.
    pub tentacle_sync: Option<f32>,
}

#[derive(Clone, Copy)]
pub struct EntityView<'a> {
    pub id: i32,
    pub uuid: Uuid,
    pub type_id: i32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub limb_swing: f32,
    pub limb_amount: f32,
    pub age_ticks: f32,
    pub on_ground: bool,
    pub scale: f32,
    pub hurt: bool,
    pub death_time: f32,
    pub attack_time: f32,
    pub use_ticks: f32,
    pub squish: f32,
    pub shake: f32,
    pub attack_ticks: f32,
    pub offer_ticks: f32,
    pub events: EventAnim,
    pub glide: Glide,
    /// `fuseRemainingInTicks` of primed TNT and lit TNT minecarts, with the partial tick applied.
    pub fuse: Option<f32>,
    /// `Creeper.getSwelling` with the partial tick applied; 0 for everything else.
    pub swelling: f32,
    pub tether: Option<[f64; 3]>,
    /// Absolute start and end of the lead rope when this entity is leashed.
    pub leash: Option<([f64; 3], [f64; 3])>,
    /// Ender dragon flight history: delayed yaw and pitch lean in degrees.
    pub flight: Option<(f32, f32)>,
    /// Elder guardian curse progress: draws the model as a translucent ghost in front of the camera.
    pub ghost: Option<f32>,
    pub data: &'a dyn EntityData,
    pub equipment: Option<&'a [ItemStack; 8]>,
    pub object_data: i32,
    pub light: [u8; 2],
    pub passenger: bool,
    pub invisible: bool,
    pub custom_name: Option<&'a str>,
    pub player: PlayerView<'a>,
    /// Glow colour (`Entity.getTeamColor` of a glowing entity), drawn as an outline.
    pub outline: Option<[u8; 3]>,
}

impl std::fmt::Debug for EntityView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityView")
            .field("id", &self.id)
            .field("type_id", &self.type_id)
            .field("pos", &self.pos)
            .finish_non_exhaustive()
    }
}

impl<'a> EntityView<'a> {
    pub fn new(type_id: i32, data: &'a dyn EntityData) -> Self {
        Self {
            id: 0,
            uuid: Uuid::nil(),
            type_id,
            pos: [0.0; 3],
            yaw: 0.0,
            body_yaw: 0.0,
            head_yaw: 0.0,
            pitch: 0.0,
            limb_swing: 0.0,
            limb_amount: 0.0,
            age_ticks: 0.0,
            on_ground: true,
            scale: 1.0,
            hurt: false,
            death_time: 0.0,
            attack_time: 0.0,
            use_ticks: 0.0,
            squish: 0.0,
            shake: 0.0,
            attack_ticks: 0.0,
            offer_ticks: 0.0,
            events: EventAnim::default(),
            glide: Glide::default(),
            fuse: None,
            swelling: 0.0,
            tether: None,
            leash: None,
            flight: None,
            ghost: None,
            data,
            equipment: None,
            object_data: 0,
            light: [0, 15],
            passenger: false,
            invisible: false,
            outline: None,
            custom_name: None,
            player: PlayerView::default(),
        }
    }

    /// Looks the type up by registry name (`minecraft:pig`); unknown names get id -1.
    pub fn named(type_name: &str, data: &'a dyn EntityData) -> Self {
        let id = mcr_data::v26_3()
            .entity_type_by_name(type_name)
            .map_or(-1, |id| id as i32);
        Self::new(id, data)
    }

    pub fn item(&self, slot: usize) -> Option<&'a ItemStack> {
        self.equipment
            .and_then(|e| e.get(slot))
            .filter(|s| !s.is_empty())
    }

    pub fn raw(&self, index: u8) -> Option<&'a EntityDataValue> {
        self.data.get(index)
    }

    pub fn byte(&self, index: u8) -> Option<i8> {
        match self.raw(index) {
            Some(EntityDataValue::Byte(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn int(&self, index: u8) -> Option<i32> {
        match self.raw(index) {
            Some(EntityDataValue::Int(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn boolean(&self, index: u8) -> bool {
        matches!(self.raw(index), Some(EntityDataValue::Boolean(true)))
    }

    pub fn float(&self, index: u8) -> Option<f32> {
        match self.raw(index) {
            Some(EntityDataValue::Float(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn stack(&self, index: u8) -> Option<&'a ItemStack> {
        match self.raw(index) {
            Some(EntityDataValue::ItemStack(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    /// Registry-backed id of any variant serializer at `index`.
    pub fn variant_id(&self, index: u8) -> Option<i32> {
        use EntityDataValue as V;
        match self.raw(index)? {
            V::CatVariant(v)
            | V::CowVariant(v)
            | V::WolfVariant(v)
            | V::FrogVariant(v)
            | V::PigVariant(v)
            | V::ChickenVariant(v)
            | V::ZombieNautilusVariant(v)
            | V::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn pose(&self) -> Pose {
        match self.raw(6) {
            Some(EntityDataValue::Pose(p)) => *p,
            _ => Pose::Standing,
        }
    }

    /// Entity-base flag byte: on fire, crouching, sprinting, swimming, invisible, glowing, fall flying.
    pub fn shared_flag(&self, bit: u8) -> bool {
        self.byte(0).is_some_and(|f| f as u8 >> bit & 1 != 0)
    }

    pub fn living_flag(&self, bit: u8) -> bool {
        self.byte(8).is_some_and(|f| f as u8 >> bit & 1 != 0)
    }

    pub fn mob_flag(&self, bit: u8) -> bool {
        self.byte(15).is_some_and(|f| f as u8 >> bit & 1 != 0)
    }
}
