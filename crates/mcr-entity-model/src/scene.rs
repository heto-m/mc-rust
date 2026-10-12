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
pub(crate) struct ModelEntry {
    pub(crate) id: ModelId,
    pub(crate) model: Model,
    pub(crate) binding: Binding,
    pub(crate) rest: Vec<Pose>,
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

/// Top surface height (0..=1) of the block cell `(x, y, z)`, or `None` when it is not solid.
pub type BlockTop = Box<dyn Fn(i32, i32, i32) -> Option<f32> + Send + Sync>;

const MAX_SHADOW_DROP: i32 = 4;
const PARALLEL_MIN: usize = 256;

type BoundMap = FxHashMap<(usize, &'static str), Option<BoundAnimation>>;

#[derive(Debug)]
struct PartJob {
    mi: usize,
    tex: crate::gpu::TexId,
    part: usize,
    bone_off: u32,
}

/// One entity's resolved plan and its pose results, reused across frames.
#[derive(Debug, Default)]
struct Job {
    plan: Plan,
    xf: Mat4,
    parts: Vec<PartJob>,
    bones: usize,
    base_model: Option<usize>,
    attach_world: Vec<Mat4>,
    attach_pose: Vec<Pose>,
}

thread_local! {
    static SCRATCH: std::cell::RefCell<(Vec<Pose>, Vec<Mat4>)> =
        const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}

fn eval_job(
    job: &mut Job,
    out: &mut [crate::gpu::Bone34],
    models: &[ModelEntry],
    bound: &BoundMap,
) {
    SCRATCH.with_borrow_mut(|(pose, world)| {
        let Job {
            plan,
            xf,
            parts,
            attach_world,
            attach_pose,
            ..
        } = job;
        let mut cursor = 0;
        let mut cached = None;
        for pj in parts.iter() {
            let part = &plan.parts[pj.part];
            let entry = &models[pj.mi];
            let n = entry.rest.len();
            let reuse =
                cached == Some((pj.mi, part.pose)) && part.pose != crate::plan::Pose::Keyframes;
            if !reuse {
                pose.clear();
                pose.extend_from_slice(&entry.rest);
                match part.pose {
                    crate::plan::Pose::Rig => animate(plan.rig, &plan.anim, &entry.binding, pose),
                    crate::plan::Pose::Rest => {}
                    crate::plan::Pose::Keyframes => {
                        for &(name, seconds, weight) in &plan.keyframes {
                            if let Some(Some(anim)) = bound.get(&(pj.mi, name)) {
                                anim.apply(seconds, weight, pose);
                            }
                        }
                        animate(plan.rig, &plan.anim, &entry.binding, pose);
                    }
                }
                if matches!(
                    part.pose,
                    crate::plan::Pose::Rig | crate::plan::Pose::Keyframes
                ) {
                    for slot in &plan.hide {
                        if let Some(i) = entry.binding.get(*slot) {
                            pose[i].scale = Vec3::ZERO;
                        }
                    }
                    for (slot, rot) in &plan.rot_overrides {
                        if let Some(i) = entry.binding.get(*slot) {
                            pose[i].rot = Vec3::from(*rot) * (std::f32::consts::PI / 180.0);
                        }
                    }
                }
                world.resize(n, Mat4::IDENTITY);
                evaluate(&entry.model, pose, world);
            }
            cached = Some((pj.mi, part.pose));
            if pj.part == 0 && !plan.attachments.is_empty() {
                let mut zero_scale = false;
                attach_pose.clear();
                attach_pose.extend(pose.iter().map(|p| {
                    zero_scale |= p.scale.cmpeq(Vec3::ZERO).any();
                    Pose {
                        scale: Vec3::select(p.scale.cmpeq(Vec3::ZERO), Vec3::ONE, p.scale),
                        ..*p
                    }
                }));
                attach_world.resize(n, Mat4::IDENTITY);
                if zero_scale {
                    evaluate(&entry.model, attach_pose, attach_world);
                } else {
                    attach_world.copy_from_slice(world);
                }
                for m in attach_world.iter_mut() {
                    *m = *xf * *m;
                }
            }
            let part_xf = *xf
                * Mat4::from_translation(part.offset)
                * Mat4::from_scale(Vec3::splat(part.scale));
            for (o, w) in out[cursor..cursor + n].iter_mut().zip(world.iter()) {
                *o = crate::gpu::Bone34::from_mat(&(part_xf * *w));
            }
            cursor += n;
        }
    });
}
const MAX_BOLTS: usize = 64;

pub struct EntityScene {
    pub(crate) gpu: EntityRenderer,
    pub(crate) assets: Arc<SceneAssets>,
    pub(crate) models: Vec<ModelEntry>,
    model_index: FxHashMap<Box<str>, Option<usize>>,
    pub(crate) textures: TextureCache,
    bound: FxHashMap<(usize, &'static str), Option<BoundAnimation>>,
    quad: Option<ModelId>,
    pub(crate) catalog: Catalog,
    pub(crate) items: ItemGeometry,
    jobs: Vec<Job>,
    camera: [f64; 3],
    billboard: Mat4,
    flash: bool,
    sprite_models: FxHashMap<[u32; 4], ModelId>,
    bolts: FxHashMap<u32, ModelId>,
    block_top: Option<BlockTop>,
    shadows: bool,
    pub(crate) maps: crate::maps::MapTable,
    tags: Vec<TagSource>,
    pub(crate) stats: SceneStats,
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
        textures.register("dyn:black", || {
            crate::assets::ImageRgba::solid(2, 2, [0, 0, 0, 255])
        });
        textures.register("dyn:white", || {
            crate::assets::ImageRgba::solid(2, 2, [255, 255, 255, 255])
        });
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
            jobs: Vec::new(),
            camera: [0.0; 3],
            billboard: Mat4::IDENTITY,
            flash: false,
            sprite_models: FxHashMap::default(),
            bolts: FxHashMap::default(),
            block_top: None,
            shadows: true,
            maps: Default::default(),
            tags: Vec::new(),
            stats: SceneStats::default(),
        })
    }

    /// Camera used for sprite entities and fishing lines; `yaw` and `pitch` in degrees (vanilla convention).
    pub fn set_camera(&mut self, position: [f64; 3], yaw: f32, pitch: f32) {
        self.camera = position;
        self.billboard = camera_orientation(yaw, pitch);
    }

    /// Lets entities cast blob shadows on the top surface of the block below them.
    pub fn set_block_top(&mut self, block_top: Option<BlockTop>) {
        self.block_top = block_top;
    }

    pub fn set_shadows(&mut self, enabled: bool) {
        self.shadows = enabled;
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

    pub(crate) fn model_entry(&mut self, layer: &str) -> Option<usize> {
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
        let top = self.block_top.take();
        self.update_with(
            registries,
            entities,
            top.as_deref()
                .map(|f| f as &dyn Fn(i32, i32, i32) -> Option<f32>),
        );
        self.block_top = top;
    }

    /// Like [`Self::update`] with a borrowed ground lookup, for callers whose world cannot be
    /// moved into a [`BlockTop`].
    pub fn update_grounded<'a>(
        &mut self,
        registries: &dyn Registries,
        entities: impl Iterator<Item = EntityView<'a>>,
        block_top: &dyn Fn(i32, i32, i32) -> Option<f32>,
    ) {
        self.update_with(registries, entities, Some(block_top));
    }

    fn update_with<'a>(
        &mut self,
        registries: &dyn Registries,
        entities: impl Iterator<Item = EntityView<'a>>,
        block_top: Option<&dyn Fn(i32, i32, i32) -> Option<f32>>,
    ) {
        self.gpu.begin_frame();
        self.items.begin_frame();
        for id in self.items.take_evicted() {
            self.gpu.free_model(id);
        }
        self.tags.clear();
        self.stats = SceneStats::default();
        let mut views: Vec<EntityView<'a>> = Vec::new();
        let mut jobs = std::mem::take(&mut self.jobs);
        let mut total_bones = 0usize;
        let mut count = 0usize;
        for view in entities {
            self.stats.entities += 1;
            if jobs.len() == count {
                jobs.push(Job::default());
            }
            self.plan_entity(registries, &view, &mut jobs[count], &mut total_bones);
            views.push(view);
            count += 1;
        }
        let jobs_slice = &mut jobs[..count];
        let Some((base, bones)) = self.gpu.alloc_bones(total_bones) else {
            self.stats.skipped += count;
            self.jobs = jobs;
            return;
        };
        let mut rest = bones;
        let mut slices: Vec<&mut [crate::gpu::Bone34]> = Vec::with_capacity(count);
        for job in jobs_slice.iter() {
            let (head, tail) = rest.split_at_mut(job.bones);
            slices.push(head);
            rest = tail;
        }
        let models = &self.models;
        let bound = &self.bound;
        if count >= PARALLEL_MIN {
            use rayon::prelude::*;
            jobs_slice
                .par_iter_mut()
                .zip(slices.into_par_iter())
                .for_each(|(job, out)| eval_job(job, out, models, bound));
        } else {
            for (job, out) in jobs_slice.iter_mut().zip(slices) {
                eval_job(job, out, models, bound);
            }
        }
        for (job, view) in jobs_slice.iter().zip(&views) {
            self.finish(view, job, base, block_top);
        }
        self.jobs = jobs;
    }

    fn plan_entity(
        &mut self,
        registries: &dyn Registries,
        v: &EntityView<'_>,
        job: &mut Job,
        total: &mut usize,
    ) {
        let plan = &mut job.plan;
        plan.clear();
        plan.anim = crate::catalog::base_anim(v);
        {
            let mut cx = PlanCx {
                reg: registries,
                assets: &self.assets,
                tex: &mut self.textures,
            };
            self.catalog.build(&mut cx, v, plan);
        }
        self.textures.flush(&mut self.gpu);
        job.xf = entity_transform(v, plan);
        job.parts.clear();
        job.bones = 0;
        job.base_model = None;
        for (index, part) in plan.parts.iter().enumerate() {
            let Some(mi) = self.model_entry(&part.layer) else {
                self.stats.skipped += 1;
                continue;
            };
            let n = self.models[mi].rest.len();
            if *total + n > crate::gpu::MAX_BONES {
                self.stats.skipped += 1;
                continue;
            }
            if part.pose == crate::plan::Pose::Keyframes {
                for &(name, _, _) in &plan.keyframes {
                    let assets = &self.assets;
                    let model = &self.models[mi].model;
                    self.bound.entry((mi, name)).or_insert_with(|| {
                        assets
                            .animations
                            .get(name)
                            .map(|def| BoundAnimation::bind(def, model))
                    });
                }
            }
            let tex = self
                .textures
                .resolve(&part.texture, &mut self.gpu, &self.assets);
            if index == 0 {
                job.base_model = Some(mi);
            }
            job.parts.push(PartJob {
                mi,
                tex,
                part: index,
                bone_off: *total as u32,
            });
            *total += n;
            job.bones += n;
        }
    }

    fn finish(
        &mut self,
        v: &EntityView<'_>,
        job: &Job,
        base: u32,
        block_top: Option<&dyn Fn(i32, i32, i32) -> Option<f32>>,
    ) {
        let plan = &job.plan;
        let hurt = v.hurt || v.death_time > 0.0;
        for pj in &job.parts {
            let part = &plan.parts[pj.part];
            self.gpu.push(&PartInstance {
                model: self.models[pj.mi].id,
                texture: pj.tex,
                pipeline: part.pipeline,
                bone_base: base + pj.bone_off,
                tint: part.tint,
                hurt: hurt && part.hurt,
                flash: plan.flash_parts && part.hurt,
                fullbright: part.fullbright,
                block_light: if plan.bright { 15 } else { v.light[0] },
                sky_light: v.light[1],
                uv_offset: part.uv_offset,
                origin: v.pos,
            });
            self.stats.parts += 1;
        }
        if let Some(bm) = job.base_model
            && !plan.attachments.is_empty()
            && job.parts.first().is_some_and(|p| p.part == 0)
        {
            self.emit_attachments(v, plan, job, bm);
        }
        self.emit_free(v, plan);
        self.emit_shadow(v, plan, block_top);
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
    }

    fn emit_attachments(&mut self, v: &EntityView<'_>, plan: &Plan, job: &Job, base_model: usize) {
        let xf = job.xf;
        for attach in &plan.attachments {
            let model = &self.models[base_model].model;
            let Some(bone) = model.bone(attach.bone) else {
                continue;
            };
            let mut frame = if attach.offset == Vec3::ZERO {
                job.attach_world[bone as usize]
            } else {
                let parent = model.bones[bone as usize]
                    .parent
                    .map_or(xf, |p| job.attach_world[p as usize]);
                let mut pose = job.attach_pose[bone as usize];
                pose.pos += attach.offset;
                parent * pose.matrix()
            };
            frame *= attach.pre;
            if let Some(name) = attach.rx_of
                && let Some(i) = model.bone(name)
            {
                frame *= Mat4::from_rotation_x(job.attach_pose[i as usize].rot.x);
            }
            frame *= attach.post;
            let geo = match &attach.kind {
                AttachKind::Item { source, context } => {
                    let stack = match source {
                        ItemSource::Equipment(slot) => v.item(*slot),
                        ItemSource::Data(index) => v.stack(*index),
                    };
                    let Some(stack) = stack else { continue };
                    self.items.item(stack, *context)
                }
                AttachKind::Block { state } => self.items.block(*state),
            };
            let Some(geo) = geo else { continue };
            let glint = match &attach.kind {
                AttachKind::Item { source, .. } => match source {
                    ItemSource::Equipment(slot) => v.item(*slot),
                    ItemSource::Data(index) => v.stack(*index),
                }
                .is_some_and(crate::equipment::has_glint),
                AttachKind::Block { .. } => false,
            };
            self.push_geo(&geo, frame, v.light, v.pos, v.age_ticks, glint);
        }
    }

    pub(crate) fn push_geo(
        &mut self,
        geo: &crate::items::ItemGeo,
        matrix: Mat4,
        light: [u8; 2],
        origin: [f64; 3],
        age_ticks: f32,
        glint: bool,
    ) {
        for part in &geo.parts {
            let model_id = *part.model_id.get_or_init(|| self.gpu.add_model(&part.mesh));
            let tex = self
                .textures
                .resolve(&part.texture, &mut self.gpu, &self.assets);
            let Some((base, out)) = self.gpu.alloc_bones(1) else {
                continue;
            };
            out[0] = crate::gpu::Bone34::from_mat(&(matrix * part.local));
            self.gpu.push(&PartInstance {
                model: model_id,
                texture: tex,
                pipeline: part.pipeline,
                bone_base: base,
                tint: part.tint,
                hurt: false,
                flash: self.flash,
                fullbright: false,
                block_light: light[0],
                sky_light: light[1],
                uv_offset: [0.0; 2],
                origin,
            });
            self.stats.parts += 1;
            if glint {
                let tex =
                    self.textures
                        .resolve("misc/enchanted_glint_item", &mut self.gpu, &self.assets);
                let Some((base, out)) = self.gpu.alloc_bones(1) else {
                    continue;
                };
                out[0] = crate::gpu::Bone34::from_mat(&(matrix * part.local));
                self.gpu.push(&PartInstance {
                    model: model_id,
                    texture: tex,
                    pipeline: crate::gpu::Pipeline::Additive,
                    bone_base: base,
                    tint: [160, 100, 255, 150],
                    hurt: false,
                    flash: false,
                    fullbright: true,
                    block_light: light[0],
                    sky_light: light[1],
                    uv_offset: glint_scroll(age_ticks),
                    origin,
                });
                self.stats.parts += 1;
            }
        }
    }

    fn emit_free(&mut self, v: &EntityView<'_>, plan: &Plan) {
        let mut lit = *v;
        if plan.bright {
            lit.light[0] = 15;
        }
        let v = &lit;
        self.flash = plan.flash;
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
            let mut m = item.local * lift;
            if item.billboard {
                m = m * self.billboard * item.post;
            }
            self.push_geo(
                &geo,
                m,
                v.light,
                v.pos,
                v.age_ticks,
                crate::equipment::has_glint(stack),
            );
        }
        for g in &plan.free_geo {
            match g {
                crate::plan::FreeGeo::Block { state, local } => {
                    if let Some(geo) = self.items.block(*state) {
                        self.push_geo(&geo, *local, v.light, v.pos, v.age_ticks, false);
                    }
                }
                crate::plan::FreeGeo::Model { id, local } => {
                    if let Some(geo) = self.items.model(id) {
                        self.push_geo(&geo, *local, v.light, v.pos, v.age_ticks, false);
                    }
                }
                crate::plan::FreeGeo::Sprite {
                    texture,
                    uv,
                    local,
                    post,
                    tint,
                    translucent,
                    billboard,
                } => {
                    let mut m = *local;
                    if *billboard {
                        m *= self.billboard;
                    }
                    m *= *post;
                    let pipeline = if *translucent {
                        crate::gpu::Pipeline::Translucent
                    } else {
                        crate::gpu::Pipeline::Cutout
                    };
                    self.push_sprite(texture, *uv, m, *tint, pipeline, v);
                }
                crate::plan::FreeGeo::Line { a, b, width } => {
                    let rel = Vec3::new(
                        (v.pos[0] - self.camera[0]) as f32,
                        (v.pos[1] - self.camera[1]) as f32,
                        (v.pos[2] - self.camera[2]) as f32,
                    );
                    let mid = (*a + *b) * 0.5;
                    let view = (rel + mid).normalize_or_zero();
                    let along = *b - *a;
                    let side = along.cross(view).normalize_or_zero() * *width;
                    let m = Mat4::from_cols(
                        side.extend(0.0),
                        along.extend(0.0),
                        view.extend(0.0),
                        mid.extend(1.0),
                    );
                    self.push_sprite(
                        "dyn:black",
                        [0.0, 0.0, 1.0, 1.0],
                        m,
                        [255; 4],
                        crate::gpu::Pipeline::Cutout,
                        v,
                    );
                }
                crate::plan::FreeGeo::Bolt { seed } => self.push_bolt(*seed, v),
                crate::plan::FreeGeo::Map { id, local } => {
                    self.emit_map(*id, *local, v.light, false, v.pos);
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
                        flash: false,
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
        self.flash = false;
    }

    fn push_sprite(
        &mut self,
        texture: &str,
        uv: [f32; 4],
        matrix: Mat4,
        tint: [u8; 4],
        pipeline: crate::gpu::Pipeline,
        v: &EntityView<'_>,
    ) {
        let key = uv.map(f32::to_bits);
        let model = match self.sprite_models.get(&key) {
            Some(m) => *m,
            None => {
                let n = [0.0, 0.0, 1.0];
                let vertices = [
                    ([-0.5, -0.5], [uv[0], uv[3]]),
                    ([0.5, -0.5], [uv[2], uv[3]]),
                    ([0.5, 0.5], [uv[2], uv[1]]),
                    ([-0.5, 0.5], [uv[0], uv[1]]),
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
                self.sprite_models.insert(key, id);
                id
            }
        };
        let tex = self.textures.resolve(texture, &mut self.gpu, &self.assets);
        let Some((base, out)) = self.gpu.alloc_bones(1) else {
            return;
        };
        out[0] = crate::gpu::Bone34::from_mat(&matrix);
        self.gpu.push(&PartInstance {
            model,
            texture: tex,
            pipeline,
            bone_base: base,
            tint,
            hurt: false,
            flash: false,
            fullbright: pipeline != crate::gpu::Pipeline::Cutout || tint[3] < 255,
            block_light: v.light[0],
            sky_light: v.light[1],
            uv_offset: [0.0; 2],
            origin: v.pos,
        });
        self.stats.parts += 1;
    }

    fn push_bolt(&mut self, seed: u32, v: &EntityView<'_>) {
        if self.bolts.len() >= MAX_BOLTS {
            let ids: Vec<ModelId> = self.bolts.drain().map(|(_, id)| id).collect();
            for id in ids {
                self.gpu.free_model(id);
            }
        }
        let model = match self.bolts.get(&seed) {
            Some(m) => *m,
            None => {
                let id = self.gpu.add_model(&crate::effects::lightning_model(seed));
                self.bolts.insert(seed, id);
                id
            }
        };
        let tex = self
            .textures
            .resolve("dyn:white", &mut self.gpu, &self.assets);
        let Some((base, out)) = self.gpu.alloc_bones(1) else {
            return;
        };
        out[0] = crate::gpu::Bone34::from_mat(&Mat4::IDENTITY);
        self.gpu.push(&PartInstance {
            model,
            texture: tex,
            pipeline: crate::gpu::Pipeline::Additive,
            bone_base: base,
            tint: [115, 115, 128, 30],
            hurt: false,
            flash: false,
            fullbright: true,
            block_light: 15,
            sky_light: 15,
            uv_offset: [0.0; 2],
            origin: v.pos,
        });
        self.stats.parts += 1;
    }

    fn emit_shadow(
        &mut self,
        v: &EntityView<'_>,
        plan: &Plan,
        top: Option<&dyn Fn(i32, i32, i32) -> Option<f32>>,
    ) {
        if !self.shadows || v.invisible || plan.parts.is_empty() {
            return;
        }
        let Some(top) = top else { return };
        let Some(name) = mcr_data::v26_3().entity_type_name(v.type_id as u32) else {
            return;
        };
        let Some(radius) = self
            .assets
            .renderers
            .0
            .get(name)
            .and_then(|r| r.shadow_radius)
        else {
            return;
        };
        let radius = radius * v.scale * plan.scale * plan.extra_scale.x.abs().max(1.0);
        if radius <= 0.0 || v.death_time > 0.0 {
            return;
        }
        let (bx, bz) = (v.pos[0].floor() as i32, v.pos[2].floor() as i32);
        let by = (v.pos[1] - 1e-4).floor() as i32;
        let mut ground = None;
        for dy in 0..=MAX_SHADOW_DROP {
            if let Some(h) = top(bx, by - dy, bz) {
                ground = Some(f64::from(by - dy) + f64::from(h.clamp(0.0, 1.0)));
                break;
            }
        }
        let Some(ground) = ground else { return };
        let drop = (v.pos[1] - ground) as f32;
        let strength = 0.5 * (1.0 - drop / (2.0 * radius.max(0.5)));
        if drop < -0.05 || strength <= 0.0 {
            return;
        }
        let alpha = (strength.clamp(0.0, 1.0) * 255.0) as u8;
        let matrix = Mat4::from_translation(Vec3::new(0.0, -drop + 0.015, 0.0))
            * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
            * Mat4::from_scale(Vec3::splat(radius * 2.0));
        self.push_sprite(
            "misc/shadow",
            [0.0, 0.0, 1.0, 1.0],
            matrix,
            [0, 0, 0, alpha],
            crate::gpu::Pipeline::Translucent,
            v,
        );
    }

    pub(crate) fn quad_model(&mut self) -> ModelId {
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

    pub fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw_opaque(pass);
    }

    pub fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw_blended(pass);
    }
}

/// Vanilla `Camera.rotation()`: billboards drawn with it face the camera.
pub(crate) fn camera_orientation(yaw: f32, pitch: f32) -> Mat4 {
    Mat4::from_quat(Quat::from_euler(
        glam::EulerRot::YXZ,
        std::f32::consts::PI - yaw.to_radians(),
        -pitch.to_radians(),
        0.0,
    ))
}

/// Maps model space (sixteenths, y down) to world space around the entity's own origin.
fn entity_transform(v: &EntityView<'_>, plan: &Plan) -> Mat4 {
    let yaw = if plan.skip_rotation {
        0.0
    } else {
        v.body_yaw + plan.body_yaw_offset
    };
    let mut m = Mat4::from_scale(Vec3::splat(v.scale * plan.scale))
        * Mat4::from_translation(plan.pre_offset);
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
    m *= plan.post_rot;
    m * Mat4::from_translation(plan.extra_offset)
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        * Mat4::from_scale(plan.extra_scale)
        * Mat4::from_translation(Vec3::new(0.0, -plan.model_y, 0.0))
        * Mat4::from_scale(Vec3::splat(MODEL_SCALE))
}

impl std::fmt::Debug for EntityScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityScene")
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

fn glint_scroll(age: f32) -> [f32; 2] {
    [(age * 0.01).fract(), (age * 0.007).fract()]
}
