use std::borrow::Cow;

use glam::{Mat4, Vec3};

use crate::gpu::Pipeline;
use crate::rig::{AnimIn, Rig};

pub const WHITE: [u8; 4] = [255; 4];

/// Where a part's pose comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pose {
    /// Run the entity rig on this part's own model.
    Rig,
    /// Rest pose.
    Rest,
    /// Run a named keyframe animation set: `(name, time seconds, weight)`.
    Keyframes,
}

/// A model plus texture drawn for an entity; layers are extra parts sharing the entity's rig.
#[derive(Debug, Clone)]
pub struct Part {
    pub layer: Cow<'static, str>,
    pub texture: Cow<'static, str>,
    pub pipeline: Pipeline,
    pub tint: [u8; 4],
    pub fullbright: bool,
    pub uv_offset: [f32; 2],
    pub pose: Pose,
    pub hurt: bool,
    pub offset: Vec3,
}

impl Part {
    pub fn new(layer: impl Into<Cow<'static, str>>, texture: impl Into<Cow<'static, str>>) -> Self {
        Part {
            layer: layer.into(),
            texture: texture.into(),
            pipeline: Pipeline::Cutout,
            tint: WHITE,
            fullbright: false,
            uv_offset: [0.0; 2],
            pose: Pose::Rig,
            hurt: true,
            offset: Vec3::ZERO,
        }
    }

    pub fn tint(mut self, rgb: u32) -> Self {
        self.tint = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        self
    }

    pub fn overlay(self) -> Self {
        self.pipeline(Pipeline::Decal).no_hurt_if_false()
    }

    fn no_hurt_if_false(self) -> Self {
        self
    }

    pub fn pipeline(mut self, p: Pipeline) -> Self {
        self.pipeline = p;
        self
    }

    pub fn fullbright(mut self) -> Self {
        self.fullbright = true;
        self
    }

    pub fn no_hurt(mut self) -> Self {
        self.hurt = false;
        self
    }

    pub fn eyes(self) -> Self {
        self.pipeline(Pipeline::Additive).fullbright().no_hurt()
    }
}

/// A static item or block model attached to a bone of the first part.
#[derive(Debug, Clone)]
pub struct Attach {
    pub bone: &'static str,
    pub kind: AttachKind,
    pub local: Mat4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSource {
    Equipment(usize),
    Data(u8),
}

#[derive(Debug, Clone)]
pub enum AttachKind {
    Item {
        source: ItemSource,
        context: ItemContext,
    },
    Block {
        state: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemContext {
    RightHand,
    LeftHand,
    Head,
    Ground,
    Fixed,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub parts: Vec<Part>,
    pub rig: Rig,
    pub anim: AnimIn,
    pub scale: f32,
    pub extra_scale: Vec3,
    pub extra_offset: Vec3,
    pub attachments: Vec<Attach>,
    pub keyframes: Vec<(&'static str, f32, f32)>,
    pub spin: f32,
    pub body_yaw_offset: f32,
    pub skip_rotation: bool,
    pub flip: f32,
    pub tilt: f32,
    pub hide: Vec<crate::rig::Slot>,
    pub rot_overrides: Vec<(crate::rig::Slot, [f32; 3])>,
    pub free_items: Vec<FreeItem>,
    pub free_geo: Vec<FreeGeo>,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            parts: Vec::new(),
            rig: Rig::Static,
            anim: AnimIn::default(),
            scale: 1.0,
            extra_scale: Vec3::ONE,
            extra_offset: Vec3::ZERO,
            attachments: Vec::new(),
            keyframes: Vec::new(),
            spin: 0.0,
            body_yaw_offset: 0.0,
            skip_rotation: false,
            flip: 90.0,
            tilt: 0.0,
            hide: Vec::new(),
            rot_overrides: Vec::new(),
            free_items: Vec::new(),
            free_geo: Vec::new(),
        }
    }
}

impl Plan {
    pub fn clear(&mut self) {
        self.parts.clear();
        self.attachments.clear();
        self.keyframes.clear();
        self.rig = Rig::Static;
        self.scale = 1.0;
        self.extra_scale = Vec3::ONE;
        self.extra_offset = Vec3::ZERO;
        self.spin = 0.0;
        self.body_yaw_offset = 0.0;
        self.skip_rotation = false;
        self.flip = 90.0;
        self.tilt = 0.0;
        self.hide.clear();
        self.rot_overrides.clear();
        self.free_items.clear();
        self.free_geo.clear();
    }

    pub fn push(&mut self, part: Part) -> &mut Part {
        self.parts.push(part);
        self.parts.last_mut().expect("just pushed")
    }
}

/// An item stack drawn without a bone: `local` maps block-unit model space to entity space (y up).
#[derive(Debug, Clone)]
pub struct FreeItem {
    pub source: ItemSource,
    pub context: ItemContext,
    pub local: Mat4,
    pub ground_lift: bool,
}

/// Static textured geometry (paintings, block models) in block units around the entity origin.
#[derive(Debug, Clone)]
pub enum FreeGeo {
    Block {
        state: i32,
        local: Mat4,
    },
    Model {
        id: String,
        local: Mat4,
    },
    Quad {
        texture: String,
        size: [f32; 2],
        local: Mat4,
        tint: [u8; 4],
    },
}
