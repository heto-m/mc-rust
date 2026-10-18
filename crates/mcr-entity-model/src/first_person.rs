use glam::{Mat4, Vec3};
use mcr_protocol::types::ItemStack;

use crate::catalog::{PlanCx, base_anim};
use crate::gpu::{Bone34, PartInstance, Pipeline};
use crate::plan::{ItemContext, Part, Plan};
use crate::pose::evaluate;
use crate::rig::Slot;
use crate::scene::{EntityScene, MODEL_SCALE};
use crate::view::{EntityView, Registries};

const BODY_SLOTS: [Slot; 8] = [
    Slot::Head,
    Slot::Hat,
    Slot::Body,
    Slot::Jacket,
    Slot::RightLeg,
    Slot::LeftLeg,
    Slot::RightPants,
    Slot::LeftPants,
];

impl EntityScene {
    /// Draws one arm of the player in `view` (skin, slim model, sleeve flags) the way vanilla's
    /// first-person `renderRightHand` does: `matrix` is the pose stack, arm coordinates stay y-down.
    pub fn emit_arm(
        &mut self,
        registries: &dyn Registries,
        view: &EntityView<'_>,
        right: bool,
        matrix: Mat4,
    ) {
        let mut plan = Plan {
            anim: base_anim(view),
            ..Plan::default()
        };
        {
            let mut cx = PlanCx {
                reg: registries,
                assets: &self.assets,
                tex: &mut self.textures,
                camera_angles: (0.0, 0.0),
            };
            self.catalog.build(&mut cx, view, &mut plan);
        }
        self.textures.flush(&mut self.gpu);
        let part: Option<Part> = plan.parts.first().cloned();
        if let Some(part) = part
            && let Some(mi) = self.model_entry(&part.layer)
        {
            let entry = &self.models[mi];
            let n = entry.rest.len();
            let mut pose = entry.rest.clone();
            let mut world = Vec::new();
            let (other, sleeve, other_sleeve) = if right {
                (Slot::LeftArm, Slot::RightSleeve, Slot::LeftSleeve)
            } else {
                (Slot::RightArm, Slot::LeftSleeve, Slot::RightSleeve)
            };
            let hidden_sleeve = plan.hide.contains(&sleeve).then_some(sleeve);
            for slot in BODY_SLOTS
                .iter()
                .copied()
                .chain([other, other_sleeve])
                .chain(hidden_sleeve)
            {
                if let Some(i) = entry.binding.get(slot) {
                    pose[i].scale = Vec3::ZERO;
                }
            }
            world.resize(n, Mat4::IDENTITY);
            evaluate(&entry.model, &pose, &mut world);
            let tex = self
                .textures
                .resolve(&part.texture, &mut self.gpu, &self.assets);
            let model_id = entry.id;
            let xf = matrix * Mat4::from_scale(Vec3::splat(MODEL_SCALE));
            if let Some((base, out)) = self.gpu.alloc_bones(n) {
                for (o, w) in out.iter_mut().zip(&world) {
                    *o = Bone34::from_mat(&(xf * *w));
                }
                self.gpu.push(&PartInstance {
                    model: model_id,
                    texture: tex,
                    pipeline: part.pipeline,
                    bone_base: base,
                    tint: part.tint,
                    hurt: false,
                    flash: false,
                    fullbright: false,
                    block_light: view.light[0],
                    sky_light: view.light[1],
                    uv_offset: [0.0; 2],
                    origin: [0.0; 3],
                    outline: None,
                    hidden: false,
                });
                self.stats.parts += 1;
            }
        }
    }

    /// Draws `stack` with its display transform for `context`; `matrix` maps block units (y up).
    /// `using` is `(ticks used, ticks remaining)` while the holder is using the item.
    pub fn emit_item(
        &mut self,
        stack: &ItemStack,
        context: ItemContext,
        matrix: Mat4,
        light: [u8; 2],
        using: Option<(f32, f32)>,
    ) -> bool {
        let Some(geo) = self.items.item_using(stack, context, using) else {
            return false;
        };
        self.push_geo(
            &geo,
            matrix,
            light,
            [0.0; 3],
            0.0,
            crate::equipment::has_glint(stack),
        );
        true
    }

    /// Draws a textured quad of `size` blocks facing -Z at `matrix`; `texture` is a path under
    /// `textures/` or a registered dynamic key.
    pub fn emit_quad(&mut self, texture: &str, size: [f32; 2], matrix: Mat4, light: [u8; 2]) {
        self.emit_quad_at(texture, size, matrix, light, [0.0; 3]);
    }

    pub fn emit_quad_at(
        &mut self,
        texture: &str,
        size: [f32; 2],
        matrix: Mat4,
        light: [u8; 2],
        origin: [f64; 3],
    ) {
        let tex = self.textures.resolve(texture, &mut self.gpu, &self.assets);
        let model_id = self.quad_model();
        let Some((base, out)) = self.gpu.alloc_bones(1) else {
            return;
        };
        out[0] = Bone34::from_mat(&(matrix * Mat4::from_scale(Vec3::new(size[0], size[1], 1.0))));
        self.gpu.push(&PartInstance {
            model: model_id,
            texture: tex,
            pipeline: Pipeline::Cutout,
            bone_base: base,
            tint: [255; 4],
            hurt: false,
            flash: false,
            fullbright: false,
            block_light: light[0],
            sky_light: light[1],
            uv_offset: [0.0; 2],
            origin,
            outline: None,
            hidden: false,
        });
        self.stats.parts += 1;
    }

    /// Uploads runtime textures registered since the last update (skins, maps).
    pub fn flush_textures(&mut self) {
        self.textures.flush(&mut self.gpu);
    }
}
