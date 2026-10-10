use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};
use rustc_hash::FxHashMap;

use crate::Error;
use crate::anim::BoundAnimation;
use crate::assets::SceneAssets;
use crate::catalog::{Catalog, PlanCx};
use crate::gpu::{EntityRenderer, LightInfo, ModelId, PartInstance, TargetFormats};
use crate::items::ItemGeometry;
use crate::model::Model;
use crate::plan::{AttachKind, ItemSource, Plan};
use crate::pose::{Pose, evaluate};
use crate::rig::{Binding, animate};
use crate::texcache::TextureCache;
use crate::view::{EntityView, Registries};

#[derive(Debug)]
struct ModelEntry {
    id: ModelId,
    model: Model,
    binding: Binding,
    rest: Vec<Pose>,
}

/// A name tag the UI should draw: screen position in pixels (origin top-left) and text.
#[derive(Debug, Clone, PartialEq)]
pub struct NameTag {
    pub entity: i32,
    pub text: String,
    pub screen: [f32; 2],
    pub distance: f32,
}

#[derive(Debug, Clone)]
struct TagSource {
    entity: i32,
    text: String,
    anchor: [f64; 3],
}

/// Camera and viewport of one render call; `view_proj` is camera-relative (rotation only view).
#[derive(Debug, Clone, Copy)]
pub struct FrameParams {
    pub view_proj: Mat4,
    pub camera: [f64; 3],
    pub light: LightInfo,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SceneStats {
    pub entities: usize,
    pub parts: usize,
    pub draws: usize,
    pub skipped: usize,
    pub models: usize,
}

#[derive(Debug)]
pub struct EntityScene {
    pub(crate) gpu: EntityRenderer,
    assets: Arc<SceneAssets>,
    models: Vec<ModelEntry>,
    model_index: FxHashMap<Box<str>, Option<usize>>,
    textures: TextureCache,
    bound: FxHashMap<(usize, &'static str), Option<BoundAnimation>>,
    quad: Option<ModelId>,
    catalog: Catalog,
    items: ItemGeometry,
    plan: Plan,
    scratch_pose: Vec<Pose>,
    scratch_world: Vec<Mat4>,
    base_world: Vec<Mat4>,
    tags: Vec<TagSource>,
    stats: SceneStats,
}

pub const MODEL_SCALE: f32 = 1.0 / 16.0;

impl EntityScene {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        formats: TargetFormats,
        assets: Arc<SceneAssets>,
    ) -> Result<Self, Error> {
        let mut textures = TextureCache::default();
        textures.register("dyn:hitbox", || {
            crate::assets::ImageRgba::solid(2, 2, [255, 40, 200, 110])
        });
        Ok(EntityScene {
            gpu: EntityRenderer::new(device, queue, formats),
            bound: FxHashMap::default(),
            quad: None,
            catalog: Catalog::new(),
            items: ItemGeometry::new(assets.clone()),
            assets,
            models: Vec::new(),
            model_index: FxHashMap::default(),
            textures,
            plan: Plan::default(),
            scratch_pose: Vec::new(),
            scratch_world: Vec::new(),
            base_world: Vec::new(),
            tags: Vec::new(),
            stats: SceneStats::default(),
        })
    }

    pub fn assets(&self) -> &Arc<SceneAssets> {
        &self.assets
    }

    pub fn stats(&self) -> SceneStats {
        self.stats
    }

    /// Registers or replaces a runtime-composed texture (player skins) under `key`.
    pub fn set_dynamic_texture(&mut self, key: &str, image: crate::assets::ImageRgba) {
        self.textures.replace(key, image);
    }

    fn model_entry(&mut self, layer: &str) -> Option<usize> {
        if let Some(hit) = self.model_index.get(layer) {
            return *hit;
        }
        let model = if layer == "builtin:hitbox" {
            Some(Model::unit_cube())
        } else {
            self.assets.layers.get(layer).map(Model::from_layer)
        };
        let entry = model.map(|model| ModelEntry {
            id: self.gpu.add_model(&model),
            binding: Binding::new(&model),
            rest: model.rest_pose(),
            model,
        });
        let index = entry.map(|e| {
            self.models.push(e);
            self.models.len() - 1
        });
        self.model_index.insert(layer.into(), index);
        index
    }

    /// Resolves every view into draw instances; call once per frame before `render`.
    pub fn update<'a>(
        &mut self,
        registries: &dyn Registries,
        entities: impl Iterator<Item = EntityView<'a>>,
    ) {
        self.gpu.begin_frame();
        self.items.begin_frame();
        for id in self.items.take_evicted() {
            self.gpu.free_model(id);
        }
        self.tags.clear();
        self.stats = SceneStats::default();
        for view in entities {
            self.stats.entities += 1;
            self.emit(registries, &view);
        }
    }

    fn emit(&mut self, registries: &dyn Registries, v: &EntityView<'_>) {
        let mut plan = std::mem::take(&mut self.plan);
        plan.clear();
        plan.anim = crate::catalog::base_anim(v);
        {
            let mut cx = PlanCx {
                reg: registries,
                assets: &self.assets,
                tex: &mut self.textures,
            };
            self.catalog.build(&mut cx, v, &mut plan);
        }
        self.textures.flush(&mut self.gpu);
        let xf = entity_transform(v, &plan);
        let light = v.light;
        let hurt = v.hurt || v.death_time > 0.0;
        let mut base_ready = false;
        for (index, part) in plan.parts.iter().enumerate() {
            let Some(mi) = self.model_entry(&part.layer) else {
                self.stats.skipped += 1;
                continue;
            };
            let entry = &self.models[mi];
            let n = entry.rest.len();
            self.scratch_pose.clear();
            self.scratch_pose.extend_from_slice(&entry.rest);
            match part.pose {
                crate::plan::Pose::Rig => {
                    animate(plan.rig, &plan.anim, &entry.binding, &mut self.scratch_pose)
                }
                crate::plan::Pose::Rest => {}
                crate::plan::Pose::Keyframes => {
                    for &(name, seconds, weight) in &plan.keyframes {
                        let assets = &self.assets;
                        let bound = self.bound.entry((mi, name)).or_insert_with(|| {
                            assets
                                .animations
                                .get(name)
                                .map(|def| BoundAnimation::bind(def, &entry.model))
                        });
                        if let Some(anim) = bound {
                            anim.apply(seconds, weight, &mut self.scratch_pose);
                        }
                    }
                    animate(plan.rig, &plan.anim, &entry.binding, &mut self.scratch_pose);
                }
            }
            if matches!(
                part.pose,
                crate::plan::Pose::Rig | crate::plan::Pose::Keyframes
            ) {
                for slot in &plan.hide {
                    if let Some(i) = entry.binding.get(*slot) {
                        self.scratch_pose[i].scale = Vec3::ZERO;
                    }
                }
                for (slot, rot) in &plan.rot_overrides {
                    if let Some(i) = entry.binding.get(*slot) {
                        self.scratch_pose[i].rot =
                            Vec3::from(*rot) * (std::f32::consts::PI / 180.0);
                    }
                }
            }
            self.scratch_world.resize(n, Mat4::IDENTITY);
            evaluate(&entry.model, &self.scratch_pose, &mut self.scratch_world);
            if index == 0 && !plan.attachments.is_empty() {
                self.base_world.clear();
                self.base_world
                    .extend(self.scratch_world.iter().map(|m| xf * *m));
                base_ready = true;
            }
            let tex = self
                .textures
                .resolve(&part.texture, &mut self.gpu, &self.assets);
            let model_id = entry.id;
            let part_xf = xf * Mat4::from_translation(part.offset);
            let Some((base, out)) = self.gpu.alloc_bones(n) else {
                self.stats.skipped += 1;
                continue;
            };
            for (o, w) in out.iter_mut().zip(&self.scratch_world) {
                *o = crate::gpu::Bone34::from_mat(&(part_xf * *w));
            }
            self.gpu.push(&PartInstance {
                model: model_id,
                texture: tex,
                pipeline: part.pipeline,
                bone_base: base,
                tint: part.tint,
                hurt: hurt && part.hurt,
                fullbright: part.fullbright,
                block_light: light[0],
                sky_light: light[1],
                uv_offset: part.uv_offset,
                origin: v.pos,
            });
            self.stats.parts += 1;
        }
        if base_ready {
            self.emit_attachments(v, &plan);
        }
        self.emit_free(v, &plan);
        if let Some(name) = v.custom_name {
            let height = mcr_data::v26_3()
                .entity_type_info(v.type_id as u32)
                .map_or(1.8, |i| f64::from(i.height));
            self.tags.push(TagSource {
                entity: v.id,
                text: name.to_owned(),
                anchor: [v.pos[0], v.pos[1] + height + 0.5, v.pos[2]],
            });
        }
        self.plan = plan;
    }

    fn emit_attachments(&mut self, v: &EntityView<'_>, plan: &Plan) {
        let Some(base_model) = self
            .model_index
            .get(&*plan.parts[0].layer)
            .copied()
            .flatten()
        else {
            return;
        };
        for attach in &plan.attachments {
            let Some(bone) = self.models[base_model].model.bone(attach.bone) else {
                continue;
            };
            let parent = self.base_world[bone as usize];
            let AttachKind::Item { source, context } = &attach.kind else {
                continue;
            };
            let stack = match source {
                ItemSource::Equipment(slot) => v.item(*slot),
                ItemSource::Data(index) => v.stack(*index),
            };
            let Some(stack) = stack else { continue };
            let Some(geo) = self.items.item(stack, *context) else {
                continue;
            };
            for part in &geo.parts {
                let model_id = *part.model_id.get_or_init(|| self.gpu.add_model(&part.mesh));
                let tex = self
                    .textures
                    .resolve(&part.texture, &mut self.gpu, &self.assets);
                let Some((base, out)) = self.gpu.alloc_bones(1) else {
                    continue;
                };
                out[0] = crate::gpu::Bone34::from_mat(&(parent * attach.local * part.local));
                self.gpu.push(&PartInstance {
                    model: model_id,
                    texture: tex,
                    pipeline: part.pipeline,
                    bone_base: base,
                    tint: part.tint,
                    hurt: false,
                    fullbright: false,
                    block_light: v.light[0],
                    sky_light: v.light[1],
                    uv_offset: [0.0; 2],
                    origin: v.pos,
                });
                self.stats.parts += 1;
            }
        }
    }

    fn emit_free(&mut self, v: &EntityView<'_>, plan: &Plan) {
        let push_geo =
            |this: &mut Self, geo: &crate::items::ItemGeo, matrix: Mat4, v: &EntityView<'_>| {
                for part in &geo.parts {
                    let model_id = *part.model_id.get_or_init(|| this.gpu.add_model(&part.mesh));
                    let tex = this
                        .textures
                        .resolve(&part.texture, &mut this.gpu, &this.assets);
                    let Some((base, out)) = this.gpu.alloc_bones(1) else {
                        continue;
                    };
                    out[0] = crate::gpu::Bone34::from_mat(&(matrix * part.local));
                    this.gpu.push(&PartInstance {
                        model: model_id,
                        texture: tex,
                        pipeline: part.pipeline,
                        bone_base: base,
                        tint: part.tint,
                        hurt: false,
                        fullbright: false,
                        block_light: v.light[0],
                        sky_light: v.light[1],
                        uv_offset: [0.0; 2],
                        origin: v.pos,
                    });
                    this.stats.parts += 1;
                }
            };
        for item in &plan.free_items {
            let stack = match item.source {
                ItemSource::Equipment(s) => v.item(s),
                ItemSource::Data(i) => v.stack(i),
            };
            let Some(stack) = stack else { continue };
            let Some(geo) = self.items.item(stack, item.context) else {
                continue;
            };
            let lift = if item.ground_lift {
                Mat4::from_translation(Vec3::new(0.0, geo.ground_offset, 0.0))
            } else {
                Mat4::IDENTITY
            };
            push_geo(self, &geo, item.local * lift, v);
        }
        for g in &plan.free_geo {
            match g {
                crate::plan::FreeGeo::Block { state, local } => {
                    if let Some(geo) = self.items.block(*state) {
                        push_geo(self, &geo, *local, v);
                    }
                }
                crate::plan::FreeGeo::Model { id, local } => {
                    if let Some(geo) = self.items.model(id) {
                        push_geo(self, &geo, *local, v);
                    }
                }
                crate::plan::FreeGeo::Quad {
                    texture,
                    size,
                    local,
                    tint,
                } => {
                    let tex = self.textures.resolve(texture, &mut self.gpu, &self.assets);
                    let model_id = self.quad_model();
                    let Some((base, out)) = self.gpu.alloc_bones(1) else {
                        continue;
                    };
                    out[0] = crate::gpu::Bone34::from_mat(
                        &(*local * Mat4::from_scale(Vec3::new(size[0], size[1], 1.0))),
                    );
                    self.gpu.push(&PartInstance {
                        model: model_id,
                        texture: tex,
                        pipeline: crate::gpu::Pipeline::Cutout,
                        bone_base: base,
                        tint: *tint,
                        hurt: false,
                        fullbright: false,
                        block_light: v.light[0],
                        sky_light: v.light[1],
                        uv_offset: [0.0; 2],
                        origin: v.pos,
                    });
                    self.stats.parts += 1;
                }
            }
        }
    }

    fn quad_model(&mut self) -> ModelId {
        if let Some(id) = self.quad {
            return id;
        }
        let n = [0.0, 0.0, -1.0];
        let vertices = [
            ([0.5, 0.5], [0.0, 0.0]),
            ([-0.5, 0.5], [1.0, 0.0]),
            ([-0.5, -0.5], [1.0, 1.0]),
            ([0.5, -0.5], [0.0, 1.0]),
        ]
        .iter()
        .map(|(p, uv)| crate::cube::Vertex {
            pos: [p[0], p[1], 0.0],
            bone: 0,
            normal: n,
            uv: *uv,
        })
        .collect();
        let id = self
            .gpu
            .add_model(&Model::from_geometry(vertices, vec![0, 1, 2, 0, 2, 3]));
        self.quad = Some(id);
        id
    }

    /// Screen-space name tags of the last `update`; `viewport` is the target size in pixels.
    pub fn name_tags(&self, frame: &FrameParams, viewport: [f32; 2]) -> Vec<NameTag> {
        let mut out = Vec::new();
        for tag in &self.tags {
            let rel = Vec3::new(
                (tag.anchor[0] - frame.camera[0]) as f32,
                (tag.anchor[1] - frame.camera[1]) as f32,
                (tag.anchor[2] - frame.camera[2]) as f32,
            );
            let clip = frame.view_proj * Vec4::new(rel.x, rel.y, rel.z, 1.0);
            if clip.w <= 0.0 {
                continue;
            }
            let ndc = clip.truncate() / clip.w;
            if ndc.x.abs() > 1.2 || ndc.y.abs() > 1.2 {
                continue;
            }
            out.push(NameTag {
                entity: tag.entity,
                text: tag.text.clone(),
                screen: [
                    (ndc.x * 0.5 + 0.5) * viewport[0],
                    (0.5 - ndc.y * 0.5) * viewport[1],
                ],
                distance: rel.length(),
            });
        }
        out
    }

    /// Uploads this frame's instances and records the draws into `pass`.
    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>, frame: &FrameParams) {
        self.prepare(frame);
        self.gpu.draw(pass);
    }

    /// The upload half of `render`, for callers that record passes separately.
    pub fn prepare(&mut self, frame: &FrameParams) {
        self.gpu.prepare(frame.view_proj, frame.camera, frame.light);
        self.stats.draws = self.gpu.draw_count();
        self.stats.models = self.gpu.model_count();
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw(pass);
    }
}

/// Maps model space (sixteenths, y down) to world space around the entity's own origin.
fn entity_transform(v: &EntityView<'_>, plan: &Plan) -> Mat4 {
    let yaw = if plan.skip_rotation {
        0.0
    } else {
        v.body_yaw + plan.body_yaw_offset
    };
    let mut m = Mat4::from_scale(Vec3::splat(v.scale * plan.scale));
    if !plan.skip_rotation {
        m *= Mat4::from_rotation_y((180.0 - yaw).to_radians());
    }
    if plan.spin != 0.0 {
        m *= Mat4::from_rotation_y(plan.spin);
    }
    if plan.tilt != 0.0 {
        m *= Mat4::from_rotation_z(plan.tilt.to_radians());
    }
    if v.death_time > 0.0 {
        let fall = ((v.death_time - 1.0) / 20.0 * 1.6).max(0.0).sqrt().min(1.0);
        m *= Mat4::from_quat(Quat::from_rotation_z((fall * plan.flip).to_radians()));
    }
    m * Mat4::from_translation(plan.extra_offset)
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        * Mat4::from_scale(plan.extra_scale)
        * Mat4::from_translation(Vec3::new(0.0, -1.501, 0.0))
        * Mat4::from_scale(Vec3::splat(MODEL_SCALE))
}
