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
use crate::view::{BodyVisibility, EntityView, Registries};

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

/// Shadow receiver test for the air cell `(x, y, z)`: `Some(brightness)` when the block below is a
/// visible full block and the cell's raw light is above 3, with `Lightmap.getBrightness` applied.
pub type ShadowGround = Box<dyn Fn(i32, i32, i32) -> Option<f32> + Send + Sync>;

/// `LivingEntityRenderer`'s 654311423 colour: an invisible body seen by a spectator.
const INVISIBLE_ALPHA: u16 = 38;
const MAX_SHADOW_PIECES: usize = 256;
/// Frame order of the `fire_0` and `fire_1` strips (`.png.mcmeta`), one tick each.
const FIRE_FRAMES: [u8; 32] = [
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
    10, 11, 12, 13, 14, 15,
];
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
        let anchored = plan.parts.iter().any(|p| p.anchor.is_some());
        let base = parts.first().filter(|p| p.part == 0).map(|p| p.mi);
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
                    crate::plan::Pose::Own(rig) => animate(rig, &plan.anim, &entry.binding, pose),
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
            if pj.part == 0 && (!plan.attachments.is_empty() || anchored) {
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
            let frame = match (part.anchor, base) {
                (Some((bone, local)), Some(bm)) => models[bm]
                    .model
                    .bone(bone)
                    .and_then(|b| attach_world.get(b as usize))
                    .map_or(*xf, |m| *m * local),
                _ => *xf,
            };
            let part_xf = frame
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

const LEASH_STEPS: usize = 24;
const LEASH_WIDTH: f32 = 0.05;
const LEASH_MAX_SQ: f32 = 64.0 * 64.0;

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
    camera_angles: (f32, f32),
    billboard: Mat4,
    flash: bool,
    /// Glow colour of the entity being emitted.
    outline: Option<[u8; 3]>,
    sprite_models: FxHashMap<[u32; 4], ModelId>,
    bolts: FxHashMap<u32, ModelId>,
    shadow_ground: Option<ShadowGround>,
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
        for (key, path) in [
            ("dyn:fire_0", "block/fire_0"),
            ("dyn:fire_1", "block/fire_1"),
        ] {
            if let Some(grid) = assets.read_texture(path).as_ref().and_then(fire_grid) {
                textures.register(key, || grid);
            }
        }
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
            camera_angles: (0.0, 0.0),
            billboard: Mat4::IDENTITY,
            flash: false,
            outline: None,
            sprite_models: FxHashMap::default(),
            bolts: FxHashMap::default(),
            shadow_ground: None,
            shadows: true,
            maps: Default::default(),
            tags: Vec::new(),
            stats: SceneStats::default(),
        })
    }

    /// Camera used for sprite entities and fishing lines; `yaw` and `pitch` in degrees (vanilla convention).
    pub fn set_camera(&mut self, position: [f64; 3], yaw: f32, pitch: f32) {
        self.camera = position;
        self.camera_angles = (yaw, pitch);
        self.billboard = camera_orientation(yaw, pitch);
    }

    pub fn camera_angles(&self) -> (f32, f32) {
        self.camera_angles
    }

    /// Lets entities cast shadows on the blocks below them.
    pub fn set_shadow_ground(&mut self, ground: Option<ShadowGround>) {
        self.shadow_ground = ground;
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

    pub fn live_layers(&self) -> u32 {
        (0..crate::gpu::CLASS_SIZES.len())
            .map(|c| self.gpu.live_layers(c))
            .sum()
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
        let top = self.shadow_ground.take();
        self.update_with(
            registries,
            entities,
            top.as_deref()
                .map(|f| f as &dyn Fn(i32, i32, i32) -> Option<f32>),
        );
        self.shadow_ground = top;
    }

    /// Like [`Self::update`] with a borrowed ground lookup, for callers whose world cannot be
    /// moved into a [`ShadowGround`].
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
        self.textures.sweep(&mut self.gpu);
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
                camera_angles: self.camera_angles,
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
        self.outline = v.outline;
        self.items.owner = Some(crate::items::ItemOwner {
            id: v.id,
            pos: v.pos,
            yaw: crate::misc::owner_yaw(v),
        });
        for pj in &job.parts {
            let part = &plan.parts[pj.part];
            let (mut pipeline, mut tint, mut hidden) = (part.pipeline, part.tint, false);
            if part.body {
                match v.body {
                    BodyVisibility::Visible => {}
                    BodyVisibility::Translucent => {
                        if pipeline == crate::gpu::Pipeline::Cutout {
                            pipeline = crate::gpu::Pipeline::Translucent;
                        }
                        tint[3] = (u16::from(tint[3]) * INVISIBLE_ALPHA / 255) as u8;
                    }
                    BodyVisibility::Hidden if self.outline.is_some() => hidden = true,
                    BodyVisibility::Hidden => continue,
                }
            }
            self.gpu.push(&PartInstance {
                model: self.models[pj.mi].id,
                texture: pj.tex,
                pipeline,
                bone_base: base + pj.bone_off,
                tint,
                hurt: hurt && part.hurt,
                flash: plan.flash_parts && part.hurt,
                fullbright: part.fullbright,
                block_light: if plan.bright { 15 } else { v.light[0] },
                sky_light: v.light[1],
                uv_offset: part.uv_offset,
                origin: v.pos,
                outline: self.outline,
                hidden,
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
        self.outline = None;
        if v.on_fire {
            self.push_flame(v);
        }
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
                    let using = match source {
                        ItemSource::Equipment(slot) => use_state(v, *slot),
                        ItemSource::Data(_) => None,
                    };
                    self.items.item_using(stack, *context, using)
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
                outline: self.outline,
                hidden: false,
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
                    outline: self.outline,
                    hidden: false,
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
            let cluster = item.cluster.map_or(Vec3::ZERO, |(i, n)| {
                crate::misc::cluster_offset(v.id, i, n, geo.depth)
            });
            let mut m = item.local * Mat4::from_translation(cluster) * lift;
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
        if let Some((start, end)) = v.leash {
            let outline = self.outline.take();
            self.push_leash(v, start, end);
            self.outline = outline;
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
                        outline: self.outline,
                        hidden: false,
                    });
                    self.stats.parts += 1;
                }
            }
        }
        self.flash = false;
    }

    fn push_leash(&mut self, v: &EntityView<'_>, start: [f64; 3], end: [f64; 3]) {
        const STEPS: usize = LEASH_STEPS;
        let rel = |p: [f64; 3]| {
            Vec3::new(
                (p[0] - v.pos[0]) as f32,
                (p[1] - v.pos[1]) as f32,
                (p[2] - v.pos[2]) as f32,
            )
        };
        let (a, b) = (rel(start), rel(end));
        let d = b - a;
        if !d.is_finite() || d.length_squared() > LEASH_MAX_SQ {
            return;
        }
        let eye = Vec3::new(
            (v.pos[0] - self.camera[0]) as f32,
            (v.pos[1] - self.camera[1]) as f32,
            (v.pos[2] - self.camera[2]) as f32,
        );
        for k in 0..STEPS {
            let p0 = a + d * (k as f32 / STEPS as f32);
            let p1 = a + d * ((k + 1) as f32 / STEPS as f32);
            let mid = (p0 + p1) * 0.5;
            let view = (eye + mid).normalize_or_zero();
            let along = p1 - p0;
            let side = along.cross(view).normalize_or_zero() * LEASH_WIDTH;
            let m = Mat4::from_cols(
                side.extend(0.0),
                along.extend(0.0),
                view.extend(0.0),
                mid.extend(1.0),
            );
            let shade = if k % 2 == 0 { 1.0 } else { 0.7 };
            let tint = [
                (127.5 * shade) as u8,
                (102.0 * shade) as u8,
                (76.5 * shade) as u8,
                255,
            ];
            self.push_sprite(
                "dyn:white",
                [0.0, 0.0, 1.0, 1.0],
                m,
                tint,
                crate::gpu::Pipeline::Cutout,
                v,
            );
        }
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
            outline: self.outline,
            hidden: false,
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
            outline: self.outline,
            hidden: false,
        });
        self.stats.parts += 1;
    }

    /// `EntityRenderer#extractShadow`: one piece per receiving block, fading out by 16 blocks away.
    fn emit_shadow(
        &mut self,
        v: &EntityView<'_>,
        plan: &Plan,
        ground: Option<&dyn Fn(i32, i32, i32) -> Option<f32>>,
    ) {
        if !self.shadows || v.invisible {
            return;
        }
        let Some(ground) = ground else { return };
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
        let radius = (radius * v.scale * plan.scale * plan.extra_scale.x.abs().max(1.0)).min(32.0);
        if radius.is_nan() || radius <= 0.0 || !v.pos.iter().all(|c| c.is_finite()) {
            return;
        }
        let strength = match name {
            "minecraft:item" | "minecraft:experience_orb" => 0.75,
            _ => 1.0,
        };
        let dist_sq: f64 = (0..3).map(|i| (v.pos[i] - self.camera[i]).powi(2)).sum();
        let pow = ((1.0 - dist_sq / 256.0) * strength) as f32;
        if pow <= 0.0 {
            return;
        }
        let r = f64::from(radius);
        let [ex, ey, ez] = v.pos;
        let depth = f64::from((pow / 0.5 - 1.0).min(radius));
        let (y0, y1) = ((ey - depth).floor() as i32, ey.floor() as i32);
        let mut pieces = 0;
        for z in (ez - r).floor() as i32..=(ez + r).floor() as i32 {
            for x in (ex - r).floor() as i32..=(ex + r).floor() as i32 {
                for y in y0..=y1 {
                    if pieces >= MAX_SHADOW_PIECES {
                        return;
                    }
                    let Some(light) = ground(x, y, z) else {
                        continue;
                    };
                    let power = pow - (ey - f64::from(y)) as f32 * 0.5;
                    let alpha = (power * 0.5 * light).clamp(0.0, 1.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    let rel = [
                        (f64::from(x) - ex) as f32,
                        (f64::from(y) - ey) as f32,
                        (f64::from(z) - ez) as f32,
                    ];
                    self.push_shadow_piece(v, rel, radius, alpha);
                    pieces += 1;
                }
            }
        }
    }

    /// The block top at `rel` clipped to the shadow square, its texture spread over `2 * radius`.
    fn push_shadow_piece(&mut self, v: &EntityView<'_>, rel: [f32; 3], radius: f32, alpha: f32) {
        let (xa, xb) = (rel[0].max(-radius), (rel[0] + 1.0).min(radius));
        let (za, zb) = (rel[2].max(-radius), (rel[2] + 1.0).min(radius));
        if xa >= xb || za >= zb {
            return;
        }
        let mut tex = self
            .textures
            .resolve("misc/shadow", &mut self.gpu, &self.assets);
        let model = self.quad_model();
        let Some((base, out)) = self.gpu.alloc_bones(1) else {
            return;
        };
        out[0] = crate::gpu::Bone34::from_mat(&Mat4::from_cols(
            Vec4::new(-(xb - xa), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -(zb - za), 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new((xa + xb) * 0.5, rel[1] + 0.01, (za + zb) * 0.5, 1.0),
        ));
        let to_uv = |p: f32| -p / (2.0 * radius) + 0.5;
        let uv_offset = [tex.uv_scale[0] * to_uv(xa), tex.uv_scale[1] * to_uv(za)];
        tex.uv_scale = [
            tex.uv_scale[0] * -(xb - xa) / (2.0 * radius),
            tex.uv_scale[1] * -(zb - za) / (2.0 * radius),
        ];
        self.gpu.push(&PartInstance {
            model,
            texture: tex,
            pipeline: crate::gpu::Pipeline::Translucent,
            bone_base: base,
            tint: [0, 0, 0, (alpha * 255.0) as u8],
            hurt: false,
            flash: false,
            fullbright: true,
            block_light: 15,
            sky_light: 15,
            uv_offset,
            origin: v.pos,
            outline: None,
            hidden: false,
        });
        self.stats.parts += 1;
    }

    /// `FlameFeatureRenderer`: stacked fire sprites turned toward the camera around the vertical axis.
    fn push_flame(&mut self, v: &EntityView<'_>) {
        let Some(info) = u32::try_from(v.type_id)
            .ok()
            .and_then(|t| mcr_data::v26_3().entity_type_info(t))
        else {
            return;
        };
        let age = if crate::catalog::is_baby(v) { 0.5 } else { 1.0 };
        let (w, h) = (info.width * v.scale * age, info.height * v.scale * age);
        let s = w * 1.4;
        if !(s > 0.0 && s.is_finite() && h.is_finite()) {
            return;
        }
        let mut h = (h / s).min(64.0);
        let frame = usize::from(FIRE_FRAMES[(v.age_ticks.max(0.0) as usize) % FIRE_FRAMES.len()]);
        let (col, row) = ((frame % 8) as f32, (frame / 8) as f32);
        let base = Mat4::from_scale(Vec3::splat(s))
            * camera_orientation(self.camera_angles.0, 0.0)
            * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.3 - h.floor() * 0.02));
        let model = self.quad_model();
        let (mut r, mut y, mut z, mut k) = (0.5f32, 0.0f32, 0.0f32, 0u32);
        while h > 0.0 {
            let key = if k % 2 == 0 {
                "dyn:fire_0"
            } else {
                "dyn:fire_1"
            };
            let mut tex = self.textures.resolve(key, &mut self.gpu, &self.assets);
            let flip = if (k / 2) % 2 == 0 { -1.0 } else { 1.0 };
            let Some((bone, out)) = self.gpu.alloc_bones(1) else {
                return;
            };
            out[0] = crate::gpu::Bone34::from_mat(
                &(base
                    * Mat4::from_translation(Vec3::new(0.0, y + 0.7, z))
                    * Mat4::from_scale(Vec3::new(2.0 * r * flip, 1.4, 1.0))),
            );
            let uv_offset = [tex.uv_scale[0] * col / 8.0, tex.uv_scale[1] * row / 4.0];
            tex.uv_scale = [tex.uv_scale[0] / 8.0, tex.uv_scale[1] / 4.0];
            self.gpu.push(&PartInstance {
                model,
                texture: tex,
                pipeline: crate::gpu::Pipeline::Cutout,
                bone_base: bone,
                tint: [255; 4],
                hurt: false,
                flash: false,
                fullbright: false,
                block_light: 15,
                sky_light: v.light[1],
                uv_offset,
                origin: v.pos,
                outline: None,
                hidden: false,
            });
            self.stats.parts += 1;
            h -= 0.45;
            y += 0.45;
            r *= 0.9;
            z -= 0.03;
            k += 1;
        }
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

    /// Compass and clock inputs for item models drawn from now on.
    pub fn set_item_world(&mut self, world: crate::items::ItemWorld) {
        self.items.world = world;
    }

    /// The owner of items drawn outside entity plans (first-person hands).
    pub fn set_item_owner(&mut self, owner: Option<crate::items::ItemOwner>) {
        self.items.owner = owner;
    }

    pub fn has_outlines(&self) -> bool {
        self.gpu.has_outlines()
    }

    pub fn draw_outlines(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw_outlines(pass);
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
    let mut yaw = if plan.skip_rotation {
        0.0
    } else {
        v.body_yaw + plan.body_yaw_offset
    };
    if v.shaking {
        yaw += ((v.age_ticks.floor() * 3.25).cos() * std::f32::consts::PI * 0.4).to_degrees();
    }
    let sleeping = v.pose() == mcr_protocol::types::entity_data::Pose::Sleeping;
    let mut m = Mat4::IDENTITY;
    if sleeping && let Some(bed) = v.bed {
        let eye = u32::try_from(v.type_id)
            .ok()
            .and_then(|t| mcr_data::v26_3().entity_type_info(t))
            .map_or(1.62, |i| i.eye_height);
        let head = eye * v.scale - 0.1;
        let [x, z] = bed.step();
        m = Mat4::from_translation(Vec3::new(-x * head, 0.0, -z * head));
    }
    m *= Mat4::from_scale(Vec3::splat(v.scale * plan.scale))
        * Mat4::from_translation(plan.pre_offset);
    if !plan.skip_rotation && !sleeping {
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
    } else if sleeping {
        let angle = v.bed.map_or(yaw, |b| b.sleep_rotation());
        m *= Mat4::from_rotation_y(angle.to_radians())
            * Mat4::from_rotation_z(plan.flip.to_radians())
            * Mat4::from_rotation_y(270f32.to_radians());
    } else if v.upside_down {
        let height = u32::try_from(v.type_id)
            .ok()
            .and_then(|t| mcr_data::v26_3().entity_type_info(t))
            .map_or(1.8, |i| i.height);
        let scale = (v.scale * plan.scale).max(1.0e-3);
        m *= Mat4::from_translation(Vec3::new(0.0, (height * v.scale + 0.1) / scale, 0.0))
            * Mat4::from_rotation_z(std::f32::consts::PI);
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

/// Long enough for every item that animates with use (bows, crossbows, tridents, shields).
const USE_DURATION: f32 = 72000.0;

/// `(ticks used, ticks remaining)` when the holder is using the item in equipment `slot`.
fn use_state(v: &EntityView<'_>, slot: usize) -> Option<(f32, f32)> {
    if !v.living_flag(0) {
        return None;
    }
    let off_hand = v.living_flag(1);
    let used = match slot {
        crate::view::slot::MAINHAND => !off_hand,
        crate::view::slot::OFFHAND => off_hand,
        _ => false,
    };
    used.then(|| {
        let ticks = v.use_ticks.clamp(0.0, USE_DURATION);
        (ticks, USE_DURATION - ticks)
    })
}

/// Lays the `fire_0` strip out as 8 by 4 frames so it fits a small texture class.
fn fire_grid(strip: &crate::assets::ImageRgba) -> Option<crate::assets::ImageRgba> {
    let w = strip.width.min(128);
    if w == 0
        || strip.height < w * 32
        || strip.rgba.len() < (strip.width * strip.height * 4) as usize
    {
        return None;
    }
    let (gw, gh) = (w * 8, w * 4);
    let mut rgba = vec![0u8; (gw * gh * 4) as usize];
    for f in 0..32 {
        let (cx, cy) = ((f % 8) * w, (f / 8) * w);
        for y in 0..w {
            let src = (((f * w + y) * strip.width) * 4) as usize;
            let dst = (((cy + y) * gw + cx) * 4) as usize;
            rgba[dst..dst + (w * 4) as usize]
                .copy_from_slice(&strip.rgba[src..src + (w * 4) as usize]);
        }
    }
    Some(crate::assets::ImageRgba {
        width: gw,
        height: gh,
        rgba,
    })
}

pub(crate) fn glint_scroll(age: f32) -> [f32; 2] {
    [(age * 0.01).fract(), (age * 0.007).fract()]
}
