use mcr_protocol::types::entity_data::{EntityDataValue, Pose};

use crate::assets::SceneAssets;
use crate::idx;
use crate::plan::{Part, Plan};
use crate::rig::{AnimIn, Rig};
use crate::texcache::TextureCache;
use crate::view::{EntityView, Registries};

pub struct PlanCx<'a> {
    pub reg: &'a dyn Registries,
    pub assets: &'a SceneAssets,
    pub tex: &'a mut TextureCache,
}

impl std::fmt::Debug for PlanCx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlanCx")
    }
}

impl PlanCx<'_> {
    pub fn type_name(&self, v: &EntityView<'_>) -> Option<&'static str> {
        mcr_data::v26_3().entity_type_name(u32::try_from(v.type_id).ok()?)
    }
}

pub type BuildFn = fn(&mut PlanCx<'_>, &EntityView<'_>, &mut Plan);

/// A mob drawn from one adult model and optionally a baby model, each with a fixed texture.
#[derive(Debug)]
pub struct Simple {
    pub layer: &'static str,
    pub baby_layer: Option<&'static str>,
    pub texture: &'static str,
    pub baby_texture: Option<&'static str>,
    pub rig: Rig,
}

#[derive(Clone, Copy)]
pub(crate) enum Builder {
    Skip,
    Hitbox,
    Simple(&'static Simple),
    Func(BuildFn),
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Builder::Skip => "Skip",
            Builder::Hitbox => "Hitbox",
            Builder::Simple(_) => "Simple",
            Builder::Func(_) => "Func",
        })
    }
}

#[derive(Debug)]
pub struct Catalog {
    builders: Vec<Builder>,
}

pub(crate) const fn simple_entry(
    name: &'static str,
    layer: &'static str,
    baby_layer: Option<&'static str>,
    texture: &'static str,
    baby_texture: Option<&'static str>,
    rig: Rig,
) -> (&'static str, Simple) {
    (
        name,
        Simple {
            layer,
            baby_layer,
            texture,
            baby_texture,
            rig,
        },
    )
}

pub fn wrap_degrees(mut d: f32) -> f32 {
    d %= 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Inputs of `setupAnim` that every living entity shares.
pub fn base_anim(v: &EntityView<'_>) -> AnimIn {
    let pose = v.pose();
    AnimIn {
        walk_pos: v.limb_swing,
        walk_speed: v.limb_amount,
        age: v.age_ticks,
        head_yaw: wrap_degrees(v.head_yaw - v.body_yaw),
        head_pitch: v.pitch,
        baby: false,
        on_ground: v.on_ground,
        attack: v.attack_time,
        swing_left: false,
        crouching: pose == Pose::Crouching,
        sprinting: v.shared_flag(3),
        sitting: false,
        passenger: v.passenger,
        aggressive: v.mob_flag(2),
        swim_amount: if pose == Pose::Swimming { 1.0 } else { 0.0 },
        fall_flying: pose == Pose::FallFlying,
        main_right: true,
        ..AnimIn::default()
    }
}

impl Catalog {
    pub fn new() -> Self {
        let version = mcr_data::v26_3();
        let mut builders = Vec::new();
        let mut id = 0u32;
        while let Some(name) = version.entity_type_name(id) {
            builders.push(crate::mobs::builder_for(name));
            id += 1;
        }
        Catalog { builders }
    }

    pub fn build(&self, cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
        let Ok(index) = usize::try_from(v.type_id) else {
            return;
        };
        match self.builders.get(index).copied().unwrap_or(Builder::Hitbox) {
            Builder::Skip => {}
            Builder::Hitbox => hitbox(v, plan),
            Builder::Simple(s) => simple(s, v, plan),
            Builder::Func(f) => f(cx, v, plan),
        }
        crate::kf::drive(cx, v, plan);
    }

    /// Whether a type has a dedicated renderer rather than the hitbox fallback.
    pub fn is_mapped(&self, type_id: u32) -> bool {
        !matches!(
            self.builders.get(type_id as usize),
            Some(Builder::Hitbox) | None
        )
    }

    pub fn len(&self) -> usize {
        self.builders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.builders.is_empty()
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn hitbox(v: &EntityView<'_>, plan: &mut Plan) {
    let (w, h) = mcr_data::v26_3()
        .entity_type_info(v.type_id as u32)
        .map_or((0.6, 0.6), |i| (i.width, i.height));
    plan.rig = Rig::Static;
    plan.skip_rotation = true;
    plan.extra_scale = glam::Vec3::new(w, h, w);
    plan.push(
        Part::new("builtin:hitbox", "dyn:hitbox")
            .pipeline(crate::gpu::Pipeline::Translucent)
            .no_hurt(),
    );
}

pub(crate) fn simple(s: &Simple, v: &EntityView<'_>, plan: &mut Plan) {
    let baby = is_baby(v);
    plan.rig = s.rig;
    plan.anim.baby = baby;
    let (layer, texture) = match (baby, s.baby_layer) {
        (true, Some(l)) => (l, s.baby_texture.unwrap_or(s.texture)),
        _ => (s.layer, s.texture),
    };
    plan.push(Part::new(layer, texture));
}

pub fn is_baby(v: &EntityView<'_>) -> bool {
    matches!(v.raw(idx::BABY), Some(EntityDataValue::Boolean(true)))
}
