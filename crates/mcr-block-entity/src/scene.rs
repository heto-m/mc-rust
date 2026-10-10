use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::Vec3f;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::{
    Bone34, EntityRenderer, EntityScene, EntityView, Error, FrameParams, ImageRgba, Model, ModelId,
    PartInstance, Pipeline, Pose, SceneAssets, TargetFormats, TexId, default_skin, evaluate,
    missing_image,
};
use mcr_ui::Ui;
use rustc_hash::FxHashMap;

use crate::anim::{Anim, MAX_CATCH_UP_TICKS};
use crate::data::{
    BannerData, BeaconData, BlockEntityData, DyeColor, ItemRef, PotData, SignData, SkullData,
    offline_uuid,
};
use crate::flat::{FlatBind, FlatKind, FlatRenderer, FlatVertex};
use crate::kind::BlockEntityKind;
use crate::place::{ChestType, Dir, Oxidation, Placement};
use crate::text::{SideLayout, TextQuad, layout_side};

const WHITE: [u8; 4] = [255; 4];
const OUTLINE_DISTANCE: f64 = 16.0;

/// Per-frame inputs. `camera_rotation` is the camera-to-world orientation (used by the conduit eye).
#[derive(Debug, Clone, Copy)]
pub struct BlockEntityFrame {
    pub params: FrameParams,
    pub game_time: i64,
    pub partial_ticks: f32,
    pub camera_rotation: Quat,
    pub player: Option<[f64; 3]>,
    pub render_distance_blocks: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockEntityStats {
    pub entries: usize,
    pub drawn: usize,
    pub culled: usize,
    pub draw_calls: usize,
    pub text_quads: usize,
}

#[derive(Debug)]
struct Layer {
    id: ModelId,
    model: Model,
    rest: Vec<Pose>,
}

#[derive(Debug)]
struct Entry {
    kind: BlockEntityKind,
    place: Placement,
    data: BlockEntityData,
    anim: Anim,
    sign: Option<Box<[SideLayout; 2]>>,
}

struct Display {
    entity: &'static str,
    pos: [f64; 3],
    data: FxHashMap<u8, EntityDataValue>,
    yaw: f32,
    scale: f32,
    light: [u8; 2],
}

struct Ctx<'a> {
    frame: &'a BlockEntityFrame,
    light: &'a dyn Fn([i32; 3]) -> [u8; 2],
    planes: [Vec4; 5],
}

fn t(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

fn ry(deg: f32) -> Mat4 {
    Mat4::from_rotation_y(deg.to_radians())
}

fn around(m: Mat4, p: Vec3) -> Mat4 {
    Mat4::from_translation(p) * m * Mat4::from_translation(-p)
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

fn light_curve(level: f32) -> f32 {
    let v = level / 15.0;
    (v / (4.0 - 3.0 * v)).max(0.04)
}

fn shade(color: [u8; 4], level: f32) -> [u8; 4] {
    let k = light_curve(level);
    [
        (f32::from(color[0]) * k) as u8,
        (f32::from(color[1]) * k) as u8,
        (f32::from(color[2]) * k) as u8,
        color[3],
    ]
}

fn rgb_tint(rgb: u32) -> [u8; 4] {
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]
}

fn shulker_rotation(dir: Dir) -> Quat {
    let x = |a| Quat::from_rotation_x(a);
    let xyz = |a, b, c| Quat::from_euler(EulerRot::XYZ, a, b, c);
    match dir {
        Dir::Down => x(PI),
        Dir::Up => Quat::IDENTITY,
        Dir::North => xyz(FRAC_PI_2, 0.0, PI),
        Dir::South => x(FRAC_PI_2),
        Dir::West => xyz(FRAC_PI_2, 0.0, FRAC_PI_2),
        Dir::East => xyz(FRAC_PI_2, 0.0, -FRAC_PI_2),
    }
}

fn days_from_unix(secs: u64) -> (u32, u32) {
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let _ = era;
    (month, day)
}

fn extended_christmas() -> bool {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (month, day) = days_from_unix(secs);
    month == 12 && (24..=26).contains(&day)
}

#[derive(Debug)]
pub struct BlockEntityScene {
    gpu: EntityRenderer,
    flat: FlatRenderer,
    display: EntityScene,
    assets: Arc<SceneAssets>,
    layer_index: FxHashMap<String, Option<usize>>,
    layers: Vec<Layer>,
    textures: FxHashMap<String, TexId>,
    missing: Option<TexId>,
    skins: FxHashMap<String, TexId>,
    wanted_skins: Vec<String>,
    entries: FxHashMap<[i32; 3], Entry>,
    last_tick: Option<i64>,
    christmas: bool,
    scratch_pose: Vec<Pose>,
    scratch_world: Vec<Mat4>,
    stats: BlockEntityStats,
    time: f32,
    has_gateway: bool,
}

impl BlockEntityScene {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        formats: TargetFormats,
        assets: Arc<SceneAssets>,
    ) -> Result<Self, Error> {
        let mut flat = FlatRenderer::new(device, queue, formats);
        if let (Some(sky), Some(portal)) = (
            assets.read_texture("environment/end_sky"),
            assets.read_texture("entity/end_portal/end_portal"),
        ) {
            flat.set_portal_textures(&sky, &portal);
        }
        for (gateway, key) in [
            (false, "entity/beacon/beacon_beam"),
            (true, "entity/end_portal/end_gateway_beam"),
        ] {
            if let Some(img) = assets.read_texture(key) {
                flat.set_beam_texture(gateway, &img);
            }
        }
        Ok(BlockEntityScene {
            gpu: EntityRenderer::new(device, queue, formats),
            flat,
            display: EntityScene::new(device, queue, formats, assets.clone())?,
            assets,
            layer_index: FxHashMap::default(),
            layers: Vec::new(),
            textures: FxHashMap::default(),
            missing: None,
            skins: FxHashMap::default(),
            wanted_skins: Vec::new(),
            entries: FxHashMap::default(),
            last_tick: None,
            christmas: extended_christmas(),
            scratch_pose: Vec::new(),
            scratch_world: Vec::new(),
            stats: BlockEntityStats::default(),
            time: 0.0,
            has_gateway: false,
        })
    }

    pub fn set_christmas(&mut self, on: bool) {
        self.christmas = on;
    }

    pub fn stats(&self) -> BlockEntityStats {
        self.stats
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds or replaces the block entity at `pos`; animation state survives replacement.
    pub fn set(&mut self, pos: [i32; 3], state: u32, kind: BlockEntityKind, data: BlockEntityData) {
        let place = Placement::from_state(state);
        let old = self.entries.remove(&pos);
        let mut anim = old.map(|e| e.anim).unwrap_or_default();
        if let BlockEntityData::Spawner(s) = &data {
            anim.delay = i32::from(s.delay);
        }
        if let BlockEntityData::Gateway(g) = &data {
            anim.age = g.age;
        }
        self.entries.insert(
            pos,
            Entry {
                kind,
                place,
                data,
                anim,
                sign: None,
            },
        );
    }

    pub fn remove(&mut self, pos: [i32; 3]) {
        self.entries.remove(&pos);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn block_event(&mut self, pos: [i32; 3], b0: u8, b1: u8, game_time: i64) {
        if let Some(e) = self.entries.get_mut(&pos) {
            e.anim.event(e.kind, b0, b1, game_time);
        }
    }

    /// Registers a 64x64 RGBA skin for a profile texture URL reported by `pending_skins`.
    pub fn set_skin(&mut self, url: &str, image: &ImageRgba) {
        if let Some(id) = self.gpu.add_texture(image) {
            self.skins.insert(url.to_owned(), id);
        }
        self.wanted_skins.retain(|u| u != url);
    }

    /// Skin URLs of player heads that have no registered texture yet.
    pub fn pending_skins(&self) -> &[String] {
        &self.wanted_skins
    }

    fn layer(&mut self, name: &str) -> Option<usize> {
        if let Some(hit) = self.layer_index.get(name) {
            return *hit;
        }
        let full = if name.contains('#') {
            format!("minecraft:{name}")
        } else {
            format!("minecraft:{name}#main")
        };
        let index = self.assets.layers.get(&full).map(|def| {
            let model = Model::from_layer(def);
            self.layers.push(Layer {
                id: self.gpu.add_model(&model),
                rest: model.rest_pose(),
                model,
            });
            self.layers.len() - 1
        });
        self.layer_index.insert(name.to_owned(), index);
        index
    }

    fn tex(&mut self, key: &str) -> TexId {
        if let Some(id) = self.textures.get(key) {
            return *id;
        }
        let id = self
            .assets
            .read_texture(key)
            .and_then(|img| self.gpu.add_texture(&img))
            .unwrap_or_else(|| self.missing_tex());
        self.textures.insert(key.to_owned(), id);
        id
    }

    fn has_tex(&self, key: &str) -> bool {
        self.assets.read_texture(key).is_some()
    }

    fn missing_tex(&mut self) -> TexId {
        if let Some(id) = self.missing {
            return id;
        }
        let id = self
            .gpu
            .add_texture(&missing_image())
            .expect("16x16 fits the smallest class");
        self.missing = Some(id);
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn put(
        &mut self,
        layer: &str,
        tex: TexId,
        pipeline: Pipeline,
        tint: [u8; 4],
        xf: Mat4,
        pose: &mut dyn FnMut(&Model, &mut [Pose]),
        origin: [f64; 3],
        light: [u8; 2],
        fullbright: bool,
    ) {
        let Some(li) = self.layer(layer) else { return };
        let entry = &self.layers[li];
        let n = entry.rest.len();
        self.scratch_pose.clear();
        self.scratch_pose.extend_from_slice(&entry.rest);
        pose(&entry.model, &mut self.scratch_pose);
        self.scratch_world.resize(n, Mat4::IDENTITY);
        evaluate(&entry.model, &self.scratch_pose, &mut self.scratch_world);
        let model = entry.id;
        let Some((base, out)) = self.gpu.alloc_bones(n) else {
            return;
        };
        let local = xf * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        for (o, w) in out.iter_mut().zip(&self.scratch_world) {
            *o = Bone34::from_mat(&(local * *w));
        }
        self.gpu.push(&PartInstance {
            model,
            texture: tex,
            pipeline,
            bone_base: base,
            tint,
            hurt: false,
            fullbright,
            block_light: light[0],
            sky_light: light[1],
            uv_offset: [0.0; 2],
            origin,
        });
    }

    fn tick_all(&mut self, frame: &BlockEntityFrame) {
        let now = frame.game_time;
        let steps = match self.last_tick {
            Some(last) => (now - last).clamp(0, MAX_CATCH_UP_TICKS),
            None => 0,
        };
        self.last_tick = Some(now);
        for _ in 0..steps {
            for (pos, e) in &mut self.entries {
                let near = match (e.kind, frame.player) {
                    (BlockEntityKind::EnchantingTable, Some(p)) => {
                        let dx = p[0] - (f64::from(pos[0]) + 0.5);
                        let dy = p[1] - (f64::from(pos[1]) + 0.5);
                        let dz = p[2] - (f64::from(pos[2]) + 0.5);
                        (dx * dx + dy * dy + dz * dz < 9.0).then(|| dz.atan2(dx) as f32)
                    }
                    _ => None,
                };
                e.anim.tick(e.kind, e.place.powered, near);
            }
        }
    }

    /// Resolves every entry into draw instances for this frame.
    pub fn update(
        &mut self,
        frame: &BlockEntityFrame,
        mut ui: Option<&mut Ui>,
        light: &dyn Fn([i32; 3]) -> [u8; 2],
    ) {
        self.tick_all(frame);
        self.gpu.begin_frame();
        self.flat.begin_frame();
        self.wanted_skins.clear();
        self.time = (frame.game_time.rem_euclid(24000) as f32 + frame.partial_ticks) / 24000.0;
        self.has_gateway = false;
        let m = frame.params.view_proj.transpose();
        let (r0, r1, r2, r3) = (m.x_axis, m.y_axis, m.z_axis, m.w_axis);
        let _ = r2;
        let ctx = Ctx {
            frame,
            light,
            planes: [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3],
        };
        if let Some(ui) = ui.as_deref_mut() {
            let atlas = ui.fonts().atlas();
            self.flat.sync_pages(
                atlas.revision(),
                atlas.size(),
                atlas.pages().iter().map(|p| p.pixels.as_slice()),
            );
        }
        let mut entries = std::mem::take(&mut self.entries);
        let mut displays: Vec<Display> = Vec::new();
        let mut stats = BlockEntityStats {
            entries: entries.len(),
            ..Default::default()
        };
        for (pos, e) in &mut entries {
            if !self.visible(pos, e.kind, &ctx) {
                stats.culled += 1;
                continue;
            }
            stats.drawn += 1;
            let lt = (ctx.light)(*pos);
            self.emit(*pos, e, lt, &ctx, &mut displays, ui.as_deref_mut());
        }
        self.entries = entries;
        let views: Vec<EntityView<'_>> = displays
            .iter()
            .map(|d| {
                let mut v = EntityView::named(d.entity, &d.data);
                v.pos = d.pos;
                v.yaw = d.yaw;
                v.body_yaw = d.yaw;
                v.head_yaw = d.yaw;
                v.scale = d.scale;
                v.light = d.light;
                v
            })
            .collect();
        self.display.update(&(), views.into_iter());
        stats.text_quads = self.flat.quad_count();
        self.stats = stats;
    }

    fn visible(&self, pos: &[i32; 3], kind: BlockEntityKind, ctx: &Ctx<'_>) -> bool {
        let cam = ctx.frame.params.camera;
        let rel = [
            f64::from(pos[0]) + 0.5 - cam[0],
            f64::from(pos[1]) + 0.5 - cam[1],
            f64::from(pos[2]) + 0.5 - cam[2],
        ];
        let limit = kind.view_distance(ctx.frame.render_distance_blocks);
        let horizontal = rel[0] * rel[0] + rel[2] * rel[2];
        let dist2 = if kind == BlockEntityKind::Beacon {
            horizontal
        } else {
            horizontal + rel[1] * rel[1]
        };
        if dist2 > limit * limit {
            return false;
        }
        if kind == BlockEntityKind::Beacon {
            return true;
        }
        let c = Vec3::new(rel[0] as f32, rel[1] as f32, rel[2] as f32);
        let radius = 1.8;
        ctx.planes
            .iter()
            .all(|p| p.truncate().dot(c) + p.w > -radius * p.truncate().length())
    }

    fn emit(
        &mut self,
        pos: [i32; 3],
        e: &mut Entry,
        light: [u8; 2],
        ctx: &Ctx<'_>,
        displays: &mut Vec<Display>,
        ui: Option<&mut Ui>,
    ) {
        use BlockEntityKind as K;
        let origin = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
        let pt = ctx.frame.partial_ticks;
        let p = e.place.clone();
        match e.kind {
            K::Chest | K::TrappedChest | K::EnderChest => {
                self.emit_chest(e.kind, &p, &e.anim, pt, origin, light)
            }
            K::ShulkerBox => self.emit_shulker(&p, &e.anim, pt, origin, light),
            K::Bell => self.emit_bell(&p, &e.anim, pt, origin, light),
            K::Skull => {
                if let BlockEntityData::Skull(s) = &e.data {
                    self.emit_skull(&p, s, &e.anim, pt, origin, light);
                } else {
                    self.emit_skull(&p, &SkullData::default(), &e.anim, pt, origin, light);
                }
            }
            K::Banner => {
                let data = match &e.data {
                    BlockEntityData::Banner(b) => b.clone(),
                    _ => BannerData::default(),
                };
                self.emit_banner(&p, &data, pos, ctx, origin, light);
            }
            K::Lectern => self.emit_lectern(&p, origin, light),
            K::EnchantingTable => self.emit_enchant(&e.anim, pt, origin, light),
            K::DecoratedPot => {
                let data = match &e.data {
                    BlockEntityData::Pot(d) => d.clone(),
                    _ => PotData::default(),
                };
                self.emit_pot(&p, &data, &e.anim, ctx, origin, light);
            }
            K::Conduit => self.emit_conduit(e, ctx, origin, light),
            K::CopperGolemStatue => self.emit_statue(&p, origin, light),
            K::Beacon => {
                if let BlockEntityData::Beacon(b) = &e.data {
                    self.emit_beacon(b, pos, ctx);
                }
            }
            K::EndPortal => self.emit_portal(pos, ctx, false, 0x3F),
            K::EndGateway => self.emit_gateway(&e.anim, pos, ctx),
            K::Sign | K::HangingSign => {
                if let BlockEntityData::Sign(s) = &e.data {
                    let s = s.clone();
                    self.emit_sign(e, &s, pos, light, ctx, ui);
                }
            }
            K::Campfire => self.emit_campfire(e, origin, light, displays),
            K::Spawner | K::TrialSpawner => self.emit_spawner(e, origin, light, pt, displays),
            K::Vault => self.emit_vault(e, origin, light, pt, displays),
            K::Brushable => self.emit_brushable(e, origin, light, displays),
            K::Shelf => self.emit_shelf(e, origin, light, displays),
            K::Piston => self.emit_piston(e, origin, light, pt, displays),
        }
    }

    fn emit_chest(
        &mut self,
        kind: BlockEntityKind,
        p: &Placement,
        anim: &Anim,
        pt: f32,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let open = 1.0 - anim.lid(pt);
        let eased = 1.0 - open * open * open;
        let (layer, suffix) = match p.chest_type {
            ChestType::Single => ("chest", ""),
            ChestType::Left => ("double_chest_left", "_left"),
            ChestType::Right => ("double_chest_right", "_right"),
        };
        let base = if p.block.contains("copper_chest") {
            match p.oxidation {
                Oxidation::None => "copper",
                Oxidation::Exposed => "copper_exposed",
                Oxidation::Weathered => "copper_weathered",
                Oxidation::Oxidized => "copper_oxidized",
            }
        } else if kind == BlockEntityKind::EnderChest {
            "ender"
        } else if self.christmas {
            "christmas"
        } else if kind == BlockEntityKind::TrappedChest {
            "trapped"
        } else {
            "normal"
        };
        let suffix = if base == "ender" { "" } else { suffix };
        let layer = if base == "ender" { "chest" } else { layer };
        let tex = self.tex(&format!("entity/chest/{base}{suffix}"));
        let xf = around(ry(-p.facing.yrot()), Vec3::new(0.5, 0.0, 0.5));
        let angle = -(eased * FRAC_PI_2);
        self.put(
            layer,
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| {
                for name in ["lid", "lock"] {
                    if let Some(b) = m.bone(name) {
                        poses[b as usize].rot.x = angle;
                    }
                }
            },
            origin,
            light,
            false,
        );
    }

    fn emit_shulker(
        &mut self,
        p: &Placement,
        anim: &Anim,
        pt: f32,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let progress = anim.lid(pt);
        let key = match p.color {
            Some(c) => format!("entity/shulker/shulker_{}", c.name()),
            None => "entity/shulker/shulker".to_owned(),
        };
        let tex = self.tex(&key);
        let xf = t(0.5, 0.5, 0.5)
            * Mat4::from_scale(Vec3::splat(0.9995))
            * Mat4::from_quat(shulker_rotation(p.facing))
            * Mat4::from_scale(Vec3::new(1.0, -1.0, -1.0))
            * t(0.0, -1.0, 0.0);
        self.put(
            "shulker_box",
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| {
                if let Some(b) = m.bone("lid") {
                    let pose = &mut poses[b as usize];
                    pose.pos.y = 24.0 - progress * 0.5 * 16.0;
                    pose.rot.y = (270.0 * progress).to_radians();
                }
            },
            origin,
            light,
            false,
        );
    }

    fn emit_bell(&mut self, p: &Placement, anim: &Anim, pt: f32, origin: [f64; 3], light: [u8; 2]) {
        let _ = p;
        let (mut xr, mut zr) = (0.0, 0.0);
        if anim.bell_shaking {
            let ticks = anim.bell_ticks as f32 + pt;
            let base = (ticks / PI).sin() / (4.0 + ticks / 3.0);
            match anim.bell_dir {
                Dir::North => xr = -base,
                Dir::South => xr = base,
                Dir::East => zr = -base,
                Dir::West => zr = base,
                _ => {}
            }
        }
        let tex = self.tex("entity/bell/bell_body");
        self.put(
            "bell",
            tex,
            Pipeline::Cutout,
            WHITE,
            Mat4::IDENTITY,
            &mut |m, poses| {
                if let Some(b) = m.bone("bell_body") {
                    poses[b as usize].rot.x = xr;
                    poses[b as usize].rot.z = zr;
                }
            },
            origin,
            light,
            false,
        );
    }

    fn skull_texture(&mut self, name: &str, data: &SkullData) -> TexId {
        match name {
            "player" => {
                let profile = data.profile.as_ref();
                if let Some(url) = profile.and_then(|p| p.skin_url.as_deref()) {
                    if let Some(id) = self.skins.get(url) {
                        return *id;
                    }
                    if !self.wanted_skins.iter().any(|u| u == url) {
                        self.wanted_skins.push(url.to_owned());
                    }
                }
                let uuid =
                    profile.and_then(|p| p.id.or_else(|| p.name.as_deref().map(offline_uuid)));
                let key = match uuid {
                    Some(u) => {
                        let (n, slim) = default_skin(u);
                        format!("entity/player/{}/{n}", if slim { "slim" } else { "wide" })
                    }
                    None => "entity/player/wide/steve".to_owned(),
                };
                self.tex(&key)
            }
            "skeleton" => self.tex("entity/skeleton/skeleton"),
            "wither_skeleton" => self.tex("entity/skeleton/wither_skeleton"),
            "zombie" => self.tex("entity/zombie/zombie"),
            "creeper" => self.tex("entity/creeper/creeper"),
            "dragon" => self.tex("entity/enderdragon/dragon"),
            _ => self.tex("entity/piglin/piglin"),
        }
    }

    fn emit_skull(
        &mut self,
        p: &Placement,
        data: &SkullData,
        anim: &Anim,
        pt: f32,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let block = &*p.block;
        let (ty, layer) = if block.starts_with("wither_skeleton") {
            ("wither_skeleton", "wither_skeleton_skull")
        } else if block.starts_with("skeleton") {
            ("skeleton", "skeleton_skull")
        } else if block.starts_with("zombie") {
            ("zombie", "zombie_head")
        } else if block.starts_with("creeper") {
            ("creeper", "creeper_head")
        } else if block.starts_with("dragon") {
            ("dragon", "dragon_skull")
        } else if block.starts_with("piglin") {
            ("piglin", "piglin_head")
        } else {
            ("player", "player_head")
        };
        let tex = self.skull_texture(ty, data);
        let flip = Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0));
        let xf = if p.wall {
            let s = p.facing.step();
            t(0.5 - s[0] as f32 * 0.25, 0.25, 0.5 - s[2] as f32 * 0.25)
                * ry(-p.facing.opposite().yrot())
                * flip
        } else {
            t(0.5, 0.0, 0.5) * ry(-f32::from(p.rotation) * 22.5) * flip
        };
        let animation = if p.powered {
            anim.skull_ticks as f32 + pt
        } else {
            anim.skull_ticks as f32
        };
        self.put(
            layer,
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| match ty {
                "dragon" => {
                    if let Some(b) = m.bone("jaw") {
                        poses[b as usize].rot.x = ((animation * PI * 0.2).sin() + 1.0) * 0.2;
                    }
                }
                "piglin" => {
                    if let Some(b) = m.bone("left_ear") {
                        poses[b as usize].rot.z = -((animation * PI * 0.2 * 1.2).cos() + 2.5) * 0.2;
                    }
                    if let Some(b) = m.bone("right_ear") {
                        poses[b as usize].rot.z = ((animation * PI * 0.2).cos() + 2.5) * 0.2;
                    }
                }
                _ => {}
            },
            origin,
            light,
            false,
        );
    }

    fn emit_banner(
        &mut self,
        p: &Placement,
        data: &BannerData,
        pos: [i32; 3],
        ctx: &Ctx<'_>,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let angle = if p.wall {
            p.facing.yrot()
        } else {
            f32::from(p.rotation) * 22.5
        };
        let xf = t(0.5, 0.0, 0.5)
            * ry(-angle)
            * Mat4::from_scale(Vec3::new(0.6666667, -0.6666667, -0.6666667));
        let main = if p.wall {
            "wall_banner"
        } else {
            "standing_banner"
        };
        let phase = ((i64::from(pos[0]) * 7
            + i64::from(pos[1]) * 9
            + i64::from(pos[2]) * 13
            + ctx.frame.game_time)
            .rem_euclid(100) as f32
            + ctx.frame.partial_ticks)
            / 100.0;
        let wave = (-0.0125 + 0.01 * (2.0 * PI * phase).cos()) * PI;
        let base_tex = self.tex("entity/banner/banner_base");
        self.put(
            main,
            base_tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |_, _| {},
            origin,
            light,
            false,
        );
        let flag_name = format!("{}#flag", main);
        let mut wave_pose = |m: &Model, poses: &mut [Pose]| {
            if let Some(b) = m.bone("flag") {
                poses[b as usize].rot.x = wave;
            }
        };
        self.put_named(
            &flag_name,
            base_tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut wave_pose,
            origin,
            light,
        );
        let base_color = p.color.unwrap_or(DyeColor::WHITE);
        let base = self.tex("entity/banner/base");
        self.put_named(
            &flag_name,
            base,
            Pipeline::Decal,
            rgb_tint(base_color.diffuse_rgb()),
            xf,
            &mut wave_pose,
            origin,
            light,
        );
        for layer in &data.layers {
            let key = format!("entity/banner/{}", layer.pattern);
            if !self.has_tex(&key) {
                continue;
            }
            let tex = self.tex(&key);
            self.put_named(
                &flag_name,
                tex,
                Pipeline::Decal,
                rgb_tint(layer.color.diffuse_rgb()),
                xf,
                &mut wave_pose,
                origin,
                light,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn put_named(
        &mut self,
        full: &str,
        tex: TexId,
        pipeline: Pipeline,
        tint: [u8; 4],
        xf: Mat4,
        pose: &mut dyn FnMut(&Model, &mut [Pose]),
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        self.put(full, tex, pipeline, tint, xf, pose, origin, light, false);
    }

    fn book_pose(model: &Model, poses: &mut [Pose], open: f32, flip1: f32, flip2: f32) {
        let mut set = |name: &str, f: &mut dyn FnMut(&mut Pose)| {
            if let Some(b) = model.bone(name) {
                f(&mut poses[b as usize]);
            }
        };
        let x = open.sin();
        set("left_lid", &mut |p| p.rot.y = PI + open);
        set("right_lid", &mut |p| p.rot.y = -open);
        set("left_pages", &mut |p| {
            p.rot.y = open;
            p.pos.x = x;
        });
        set("right_pages", &mut |p| {
            p.rot.y = -open;
            p.pos.x = x;
        });
        set("flip_page1", &mut |p| {
            p.rot.y = open - open * 2.0 * flip1;
            p.pos.x = x;
        });
        set("flip_page2", &mut |p| {
            p.rot.y = open - open * 2.0 * flip2;
            p.pos.x = x;
        });
    }

    fn emit_lectern(&mut self, p: &Placement, origin: [f64; 3], light: [u8; 2]) {
        if !p.has_book {
            return;
        }
        let y_rot = p.facing.yrot() + 90.0;
        let xf = t(0.5, 1.0625, 0.5)
            * ry(-y_rot)
            * Mat4::from_rotation_z(67.5f32.to_radians())
            * t(0.0, -0.125, 0.0);
        let open = ((0.0f32 * 0.02).sin() * 0.1 + 1.25) * 1.2;
        let tex = self.tex("entity/enchantment/enchanting_table_book");
        self.put(
            "book",
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| Self::book_pose(m, poses, open, 0.1, 0.9),
            origin,
            light,
            false,
        );
    }

    fn emit_enchant(&mut self, anim: &Anim, pt: f32, origin: [f64; 3], light: [u8; 2]) {
        let b = &anim.book;
        let flip = b.o_flip + (b.flip - b.o_flip) * pt;
        let open_t = b.o_open + (b.open - b.o_open) * pt;
        let time = b.time + pt;
        let y_rot = b.rot_lerp(pt);
        let xf = t(0.5, 0.75, 0.5)
            * t(0.0, 0.1 + (time * 0.1).sin() * 0.01, 0.0)
            * Mat4::from_rotation_y(-y_rot)
            * Mat4::from_rotation_z(80.0f32.to_radians());
        let ff1 = (frac(flip + 0.25) * 1.6 - 0.3).clamp(0.0, 1.0);
        let ff2 = (frac(flip + 0.75) * 1.6 - 0.3).clamp(0.0, 1.0);
        let open = ((time * 0.02).sin() * 0.1 + 1.25) * open_t;
        let tex = self.tex("entity/enchantment/enchanting_table_book");
        self.put(
            "book",
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| Self::book_pose(m, poses, open, ff1, ff2),
            origin,
            light,
            false,
        );
    }

    fn emit_pot(
        &mut self,
        p: &Placement,
        data: &PotData,
        anim: &Anim,
        ctx: &Ctx<'_>,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let mut xf = around(ry(180.0 - p.facing.yrot()), Vec3::splat(0.5));
        if let Some(style) = anim.wobble_style {
            let duration = if style == 0 { 7.0 } else { 10.0 };
            let progress = ((ctx.frame.game_time - anim.wobble_start) as f32
                + ctx.frame.partial_ticks)
                / duration;
            if (0.0..=1.0).contains(&progress) {
                let centre = Vec3::new(0.5, 0.0, 0.5);
                if style == 0 {
                    let dt = progress * 2.0 * PI;
                    let tilt_x = -1.5 * (dt.cos() + 0.5) * (dt / 2.0).sin();
                    xf *= around(Mat4::from_rotation_x(tilt_x * 0.015625), centre);
                    xf *= around(Mat4::from_rotation_z(dt.sin() * 0.015625), centre);
                } else {
                    let turn = (-progress * 3.0 * PI).sin() * 0.125;
                    xf *= around(Mat4::from_rotation_y(turn * (1.0 - progress)), centre);
                }
            }
        }
        let base = self.tex("entity/decorated_pot/decorated_pot_base");
        self.put(
            "decorated_pot_base",
            base,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |_, _| {},
            origin,
            light,
            false,
        );
        for (side, sherd) in [
            ("front", &data.front),
            ("back", &data.back),
            ("left", &data.left),
            ("right", &data.right),
        ] {
            let key = sherd
                .as_deref()
                .and_then(|s| s.strip_prefix("minecraft:"))
                .and_then(|s| s.strip_suffix("_pottery_sherd"))
                .map(|n| format!("entity/decorated_pot/{n}_pottery_pattern"))
                .filter(|k| self.has_tex(k))
                .unwrap_or_else(|| "entity/decorated_pot/decorated_pot_side".to_owned());
            let tex = self.tex(&key);
            self.put(
                "decorated_pot_sides",
                tex,
                Pipeline::Cutout,
                WHITE,
                xf,
                &mut |m, poses| {
                    for other in ["front", "back", "left", "right"] {
                        if other != side
                            && let Some(b) = m.bone(other)
                        {
                            poses[b as usize].scale = Vec3::ZERO;
                        }
                    }
                },
                origin,
                light,
                false,
            );
        }
    }

    fn emit_conduit(&mut self, e: &Entry, ctx: &Ctx<'_>, origin: [f64; 3], light: [u8; 2]) {
        let (active, hunting) = match &e.data {
            BlockEntityData::Conduit(c) => (c.active, c.hunting),
            _ => (false, false),
        };
        let pt = ctx.frame.partial_ticks;
        let ticks = e.anim.ticks;
        let rotation = (ticks as f32 + if active { pt } else { 0.0 }) * -0.0375;
        if !active {
            let tex = self.tex("entity/conduit/base");
            let xf = t(0.5, 0.5, 0.5) * ry(rotation.to_degrees());
            self.put(
                "conduit#shell",
                tex,
                Pipeline::Cutout,
                WHITE,
                xf,
                &mut |_, _| {},
                origin,
                light,
                false,
            );
            return;
        }
        let anim_time = ticks as f32 + pt;
        let mut hh = (anim_time * 0.1).sin() / 2.0 + 0.5;
        hh = hh * hh + hh;
        let axis = Vec3::new(0.5, 1.0, 0.5).normalize();
        let cage_xf = t(0.5, 0.3 + hh * 0.2, 0.5) * Mat4::from_axis_angle(axis, rotation);
        let cage = self.tex("entity/conduit/cage");
        self.put(
            "conduit#cage",
            cage,
            Pipeline::Cutout,
            WHITE,
            cage_xf,
            &mut |_, _| {},
            origin,
            light,
            false,
        );
        let phase = (ticks / 66) % 3;
        let (wind_key, wind_xf) = match phase {
            1 => (
                "entity/conduit/wind_vertical",
                Mat4::from_rotation_x(FRAC_PI_2),
            ),
            2 => ("entity/conduit/wind", Mat4::from_rotation_z(FRAC_PI_2)),
            _ => ("entity/conduit/wind", Mat4::IDENTITY),
        };
        let wind = self.wind_tex(wind_key, ctx.frame.game_time);
        let first = t(0.5, 0.5, 0.5) * wind_xf;
        self.put_uv("conduit#wind", wind.0, first, wind.1, origin, light);
        let second = t(0.5, 0.5, 0.5)
            * Mat4::from_scale(Vec3::splat(0.875))
            * Mat4::from_euler(EulerRot::XYZ, PI, 0.0, PI);
        self.put_uv("conduit#wind", wind.0, second, wind.1, origin, light);
        let eye_key = if hunting {
            "entity/conduit/open_eye"
        } else {
            "entity/conduit/closed_eye"
        };
        let eye = self.tex(eye_key);
        let eye_xf = t(0.5, 0.3 + hh * 0.2, 0.5)
            * Mat4::from_scale(Vec3::splat(0.5))
            * Mat4::from_quat(ctx.frame.camera_rotation)
            * Mat4::from_quat(Quat::from_rotation_z(PI) * Quat::from_rotation_y(PI))
            * Mat4::from_scale(Vec3::splat(1.3333334));
        self.put(
            "conduit#eye",
            eye,
            Pipeline::Cutout,
            WHITE,
            eye_xf,
            &mut |_, _| {},
            origin,
            light,
            false,
        );
    }

    fn wind_tex(&mut self, key: &str, game_time: i64) -> (TexId, f32) {
        let tex = self.tex(key);
        let frame = (game_time.rem_euclid(66) / 3) as f32;
        let class = mcr_entity_model::CLASS_SIZES[usize::from(tex.class)] as f32;
        let tex = TexId {
            uv_scale: [tex.uv_scale[0], 32.0 / class],
            ..tex
        };
        (tex, frame * 32.0 / class)
    }

    fn put_uv(
        &mut self,
        layer: &str,
        tex: TexId,
        xf: Mat4,
        v_offset: f32,
        origin: [f64; 3],
        light: [u8; 2],
    ) {
        let Some(li) = self.layer(layer) else { return };
        let entry = &self.layers[li];
        let n = entry.rest.len();
        self.scratch_world.resize(n, Mat4::IDENTITY);
        evaluate(&entry.model, &entry.rest, &mut self.scratch_world);
        let model = entry.id;
        let Some((base, out)) = self.gpu.alloc_bones(n) else {
            return;
        };
        let local = xf * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        for (o, w) in out.iter_mut().zip(&self.scratch_world) {
            *o = Bone34::from_mat(&(local * *w));
        }
        self.gpu.push(&PartInstance {
            model,
            texture: tex,
            pipeline: Pipeline::Cutout,
            bone_base: base,
            tint: WHITE,
            hurt: false,
            fullbright: false,
            block_light: light[0],
            sky_light: light[1],
            uv_offset: [0.0, v_offset],
            origin,
        });
    }

    fn emit_statue(&mut self, p: &Placement, origin: [f64; 3], light: [u8; 2]) {
        let layer = match &*p.pose {
            "running" => "copper_golem_running",
            "sitting" => "copper_golem_sitting",
            "star" => "copper_golem_star",
            _ => "copper_golem",
        };
        let suffix = match p.oxidation {
            Oxidation::None => "",
            Oxidation::Exposed => "_exposed",
            Oxidation::Weathered => "_weathered",
            Oxidation::Oxidized => "_oxidized",
        };
        let tex = self.tex(&format!("entity/copper_golem/copper_golem{suffix}"));
        let xf = t(0.5, 0.0, 0.5) * ry(-p.facing.opposite().yrot()) * Mat4::from_rotation_z(PI);
        self.put(
            layer,
            tex,
            Pipeline::Cutout,
            WHITE,
            xf,
            &mut |m, poses| {
                poses[0].pos.y = 0.0;
                let _ = m;
            },
            origin,
            light,
            false,
        );
    }

    fn emit_beacon(&mut self, data: &BeaconData, pos: [i32; 3], ctx: &Ctx<'_>) {
        if !self.flat.has_beam(false) {
            return;
        }
        let cam = ctx.frame.params.camera;
        let dx = f64::from(pos[0]) + 0.5 - cam[0];
        let dz = f64::from(pos[2]) + 0.5 - cam[2];
        let scale = ((dx * dx + dz * dz).sqrt() as f32 / 96.0).max(1.0);
        let anim_time = ctx.frame.game_time.rem_euclid(40) as f32 + ctx.frame.partial_ticks;
        let mut start = 0;
        for (i, s) in data.sections.iter().enumerate() {
            let height = if i + 1 == data.sections.len() {
                2048
            } else {
                s.height
            };
            self.beam(
                false,
                pos,
                ctx,
                1.0,
                anim_time,
                start,
                height,
                s.color,
                0.2 * scale,
                0.25 * scale,
            );
            start += s.height;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn beam(
        &mut self,
        gateway: bool,
        pos: [i32; 3],
        ctx: &Ctx<'_>,
        scale: f32,
        anim_time: f32,
        start: i32,
        height: i32,
        color: u32,
        solid: f32,
        glow: f32,
    ) {
        let cam = ctx.frame.params.camera;
        let ox = (f64::from(pos[0]) + 0.5 - cam[0]) as f32;
        let oy = (f64::from(pos[1]) - cam[1]) as f32;
        let oz = (f64::from(pos[2]) + 0.5 - cam[2]) as f32;
        let end = start + height;
        let scroll = if height < 0 { anim_time } else { -anim_time };
        let v_off = frac(scroll * 0.2 - (scroll * 0.1).floor());
        let bind = if gateway {
            FlatBind::GatewayBeam
        } else {
            FlatBind::Beam
        };
        let part = |this: &mut Self,
                    kind: FlatKind,
                    rot: f32,
                    rgba: [u8; 4],
                    c: [(f32, f32); 4],
                    v1: f32,
                    v2: f32| {
            let r = Mat4::from_rotation_y(rot.to_radians());
            let place = |x: f32, z: f32, y: i32| {
                let q = r.transform_point3(Vec3::new(x, 0.0, z));
                [ox + q.x, oy + y as f32, oz + q.z]
            };
            let [wn, en, ws, es] = c;
            for ((x1, z1), (x2, z2)) in [(wn, en), (es, ws), (en, es), (ws, wn)] {
                let quad = [
                    FlatVertex {
                        pos: place(x1, z1, end),
                        uv: [1.0, v1],
                        color: rgba,
                    },
                    FlatVertex {
                        pos: place(x1, z1, start),
                        uv: [1.0, v2],
                        color: rgba,
                    },
                    FlatVertex {
                        pos: place(x2, z2, start),
                        uv: [0.0, v2],
                        color: rgba,
                    },
                    FlatVertex {
                        pos: place(x2, z2, end),
                        uv: [0.0, v1],
                        color: rgba,
                    },
                ];
                this.flat.push_quad(kind, bind, quad);
            }
        };
        let rgb = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        let vv2 = -1.0 + v_off;
        let vv1 = height as f32 * scale * (0.5 / solid) + vv2;
        part(
            self,
            FlatKind::BeamSolid,
            anim_time * 2.25 - 45.0,
            [rgb[0], rgb[1], rgb[2], 255],
            [(0.0, solid), (solid, 0.0), (-solid, 0.0), (0.0, -solid)],
            vv1,
            vv2,
        );
        let vv1 = height as f32 * scale + vv2;
        part(
            self,
            FlatKind::BeamGlow,
            0.0,
            [rgb[0], rgb[1], rgb[2], 32],
            [(-glow, -glow), (glow, -glow), (-glow, glow), (glow, glow)],
            vv1,
            vv2,
        );
    }

    fn emit_portal(&mut self, pos: [i32; 3], ctx: &Ctx<'_>, gateway: bool, faces: u8) {
        if !self.flat.has_portal() {
            return;
        }
        let cam = ctx.frame.params.camera;
        let o = [
            (f64::from(pos[0]) - cam[0]) as f32,
            (f64::from(pos[1]) - cam[1]) as f32,
            (f64::from(pos[2]) - cam[2]) as f32,
        ];
        let (y0, y1) = if gateway { (0.0, 1.0) } else { (0.375, 0.75) };
        let c = |x: f32, y: f32, z: f32| [o[0] + x, o[1] + y, o[2] + z];
        let v = |p: [f32; 3]| FlatVertex {
            pos: p,
            uv: [0.0; 2],
            color: WHITE,
        };
        // Order: down, up, north, south, west, east.
        let quads: [[[f32; 3]; 4]; 6] = [
            [
                c(0.0, y0, 1.0),
                c(0.0, y0, 0.0),
                c(1.0, y0, 0.0),
                c(1.0, y0, 1.0),
            ],
            [
                c(0.0, y1, 0.0),
                c(0.0, y1, 1.0),
                c(1.0, y1, 1.0),
                c(1.0, y1, 0.0),
            ],
            [
                c(1.0, y1, 0.0),
                c(1.0, y0, 0.0),
                c(0.0, y0, 0.0),
                c(0.0, y1, 0.0),
            ],
            [
                c(0.0, y1, 1.0),
                c(0.0, y0, 1.0),
                c(1.0, y0, 1.0),
                c(1.0, y1, 1.0),
            ],
            [
                c(0.0, y1, 0.0),
                c(0.0, y0, 0.0),
                c(0.0, y0, 1.0),
                c(0.0, y1, 1.0),
            ],
            [
                c(1.0, y1, 1.0),
                c(1.0, y0, 1.0),
                c(1.0, y0, 0.0),
                c(1.0, y1, 0.0),
            ],
        ];
        for (i, q) in quads.iter().enumerate() {
            if faces & (1 << i) != 0 {
                self.flat.push_quad(
                    FlatKind::Portal,
                    if gateway {
                        FlatBind::Gateway
                    } else {
                        FlatBind::Portal
                    },
                    [v(q[0]), v(q[1]), v(q[2]), v(q[3])],
                );
            }
        }
        self.has_gateway |= gateway;
    }

    fn emit_gateway(&mut self, anim: &Anim, pos: [i32; 3], ctx: &Ctx<'_>) {
        let pt = ctx.frame.partial_ticks;
        let spawning = anim.age < 200;
        let cooling = anim.cooldown > 0;
        if (spawning || cooling) && self.flat.has_beam(true) {
            let percent = if spawning {
                ((anim.age as f32 + pt) / 200.0).clamp(0.0, 1.0)
            } else {
                1.0 - ((anim.cooldown as f32 - pt) / 40.0).clamp(0.0, 1.0)
            };
            let scale = (percent * PI).sin();
            let distance = if spawning { 320.0 } else { 50.0 };
            let height = (scale * distance).floor() as i32;
            if height > 0 {
                let color = if spawning { 0xC74EBD } else { 0x8932B8 };
                let anim_time = ctx.frame.game_time.rem_euclid(40) as f32 + pt;
                self.beam(
                    true,
                    pos,
                    ctx,
                    scale,
                    anim_time,
                    -height,
                    height * 2,
                    color,
                    0.15,
                    0.175,
                );
            }
        }
        self.emit_portal(pos, ctx, true, 0x3F);
    }

    fn emit_sign(
        &mut self,
        e: &mut Entry,
        sign: &SignData,
        pos: [i32; 3],
        light: [u8; 2],
        ctx: &Ctx<'_>,
        ui: Option<&mut Ui>,
    ) {
        let hanging = e.kind == BlockEntityKind::HangingSign;
        if e.sign.is_none() {
            let Some(ui) = ui else { return };
            e.sign = Some(Box::new([
                layout_side(ui, &sign.front, hanging),
                layout_side(ui, &sign.back, hanging),
            ]));
        }
        let Some(layouts) = e.sign.as_deref() else {
            return;
        };
        let p = &e.place;
        let cam = ctx.frame.params.camera;
        let center = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        let dist2: f64 = (0..3).map(|i| (center[i] - cam[i]).powi(2)).sum();
        let near_outline = dist2 < OUTLINE_DISTANCE * OUTLINE_DISTANCE;
        let angle = if p.wall {
            p.facing.yrot()
        } else {
            f32::from(p.rotation) * 22.5
        };
        let o = Vec3::new(
            (f64::from(pos[0]) - cam[0]) as f32,
            (f64::from(pos[1]) - cam[1]) as f32,
            (f64::from(pos[2]) - cam[2]) as f32,
        );
        for (side_index, layout) in layouts.iter().enumerate() {
            let back = side_index == 1;
            if layout.main.is_empty() {
                continue;
            }
            let mut m = if hanging {
                let mut m = t(0.5, 0.9375, 0.5) * ry(-angle) * t(0.0, -0.3125, 0.0);
                if back {
                    m *= ry(180.0);
                }
                m * t(0.0, -0.32, 0.073)
                    * Mat4::from_scale(Vec3::new(0.0140625, -0.0140625, 0.0140625))
            } else {
                let mut m = t(0.5, 0.5, 0.5) * ry(-angle);
                if !hanging && p.wall {
                    m *= t(0.0, -0.3125, -0.4375);
                }
                if back {
                    m *= ry(180.0);
                }
                m * t(0.0, 0.33333334, 0.046666667)
                    * Mat4::from_scale(Vec3::new(0.010416667, -0.010416667, 0.010416667))
            };
            m = Mat4::from_translation(o) * m;
            let face_center = m.transform_point3(Vec3::ZERO);
            let normal = m.transform_vector3(Vec3::Z) * Vec3::new(1.0, -1.0, 1.0);
            let normal = normal.normalize_or_zero();
            if normal.dot(-face_center) <= 0.0 {
                continue;
            }
            let side = if back { &sign.back } else { &sign.front };
            let outline = layout.glowing && (layout.black_text || near_outline);
            let lt = if layout.glowing { [15, 15] } else { light };
            let level =
                f32::from(lt[0]).max(f32::from(lt[1]) * ctx.frame.params.light.sky_brightness);
            let _ = side;
            if outline {
                self.push_text(&layout.outline, &m, 0.0, level, layout.glowing);
            }
            self.push_text(&layout.main, &m, 0.02, level, layout.glowing);
        }
    }

    fn push_text(&mut self, quads: &[TextQuad], m: &Mat4, z: f32, level: f32, fullbright: bool) {
        for q in quads {
            let color = if fullbright {
                q.color
            } else {
                shade(q.color, level)
            };
            let v = |i: usize| FlatVertex {
                pos: m
                    .transform_point3(Vec3::new(q.pos[i][0], q.pos[i][1], z))
                    .to_array(),
                uv: q.uv[i],
                color,
            };
            self.flat.push_quad(
                FlatKind::Text,
                FlatBind::Page(q.page),
                [v(0), v(1), v(2), v(3)],
            );
        }
    }

    fn item_stack(item: &ItemRef) -> Option<ItemStack> {
        let id = mcr_data::v26_3().item_by_name(&item.id)?;
        Some(ItemStack::new(id as i32, item.count))
    }

    fn item_display(
        item: &ItemRef,
        origin: [f64; 3],
        light: [u8; 2],
        local: Mat4,
        context: i8,
        displays: &mut Vec<Display>,
    ) {
        let Some(stack) = Self::item_stack(item) else {
            return;
        };
        let (scale, rot, trans) = local.to_scale_rotation_translation();
        let mut data = FxHashMap::default();
        data.insert(
            11,
            EntityDataValue::Vector3(Vec3f {
                x: trans.x,
                y: trans.y,
                z: trans.z,
            }),
        );
        data.insert(
            12,
            EntityDataValue::Vector3(Vec3f {
                x: scale.x,
                y: scale.y,
                z: scale.z,
            }),
        );
        data.insert(13, EntityDataValue::Quaternion(rot.to_array()));
        data.insert(23, EntityDataValue::ItemStack(stack));
        data.insert(24, EntityDataValue::Byte(context));
        displays.push(Display {
            entity: "minecraft:item_display",
            pos: origin,
            data,
            yaw: 0.0,
            scale: 1.0,
            light,
        });
    }

    fn emit_campfire(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Campfire(c) = &e.data else {
            return;
        };
        for (slot, item) in c.items.iter().enumerate() {
            let Some(item) = item else { continue };
            let dir = Dir::from_data2d((slot as i32 + e.place.facing.data2d()) % 4);
            let m = t(0.5, 0.44921875, 0.5)
                * ry(-dir.yrot())
                * Mat4::from_rotation_x(90.0f32.to_radians())
                * t(-0.3125, -0.3125, 0.0)
                * Mat4::from_scale(Vec3::splat(0.375));
            Self::item_display(item, origin, light, m, 8, displays);
        }
    }

    fn emit_shelf(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Shelf(s) = &e.data else {
            return;
        };
        let y_rot = if matches!(e.place.facing, Dir::Up | Dir::Down) {
            180.0
        } else {
            -e.place.facing.yrot()
        };
        for (slot, item) in s.items.iter().enumerate() {
            let Some(item) = item else { continue };
            let m = t(0.5, 0.5, 0.5)
                * ry(y_rot)
                * t(
                    (slot as f32 - 1.0) * 0.3125,
                    if s.align_to_bottom { -0.25 } else { 0.0 },
                    -0.25,
                )
                * Mat4::from_scale(Vec3::splat(0.25));
            Self::item_display(item, origin, light, m, 8, displays);
        }
    }

    fn emit_vault(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        pt: f32,
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Vault(v) = &e.data else {
            return;
        };
        let Some(item) = &v.display_item else { return };
        let mut d = e.anim.spin - e.anim.o_spin;
        d = (d + 180.0).rem_euclid(360.0) - 180.0;
        let spin = e.anim.o_spin + d * pt;
        let m = t(0.5, 0.4, 0.5) * ry(spin);
        Self::item_display(item, origin, light, m, 7, displays);
    }

    fn emit_brushable(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Brushable(b) = &e.data else {
            return;
        };
        let (Some(item), Some(dir)) = (&b.item, b.hit_direction) else {
            return;
        };
        if e.place.dusted == 0 {
            return;
        }
        let dir = Dir::from_data3d(dir);
        let completion = f32::from(e.place.dusted) / 10.0 * 0.75;
        let mut tr = [0.5, 0.0, 0.5];
        match dir {
            Dir::East => tr[0] = 0.73 + completion,
            Dir::West => tr[0] = 0.25 - completion,
            Dir::Up => tr[1] = 0.25 + completion,
            Dir::Down => tr[1] = -0.23 - completion,
            Dir::North => tr[2] = 0.25 - completion,
            Dir::South => tr[2] = 0.73 + completion,
        }
        let east_west = matches!(dir, Dir::East | Dir::West);
        let m = t(0.0, 0.5, 0.0)
            * t(tr[0], tr[1], tr[2])
            * ry(75.0)
            * ry(if east_west { 90.0 } else { 0.0 } + 11.0)
            * Mat4::from_scale(Vec3::splat(0.5));
        Self::item_display(item, origin, light, m, 8, displays);
    }

    fn emit_spawner(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        pt: f32,
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Spawner(s) = &e.data else {
            return;
        };
        let Some(id) = &s.entity_id else { return };
        let Some(info) = mcr_data::v26_3()
            .entity_type_by_name(id)
            .and_then(|t| mcr_data::v26_3().entity_type_info(t))
        else {
            return;
        };
        let mut scale = 0.53125;
        let longest = info.width.max(info.height);
        if longest > 1.0 {
            scale /= longest;
        }
        let spin = (e.anim.o_spin + (e.anim.spin - e.anim.o_spin) * pt) * 10.0;
        displays.push(Display {
            entity: spawner_entity(id),
            pos: [origin[0] + 0.5, origin[1] + 0.2, origin[2] + 0.5],
            data: FxHashMap::default(),
            yaw: -spin,
            scale,
            light,
        });
    }

    fn emit_piston(
        &mut self,
        e: &Entry,
        origin: [f64; 3],
        light: [u8; 2],
        pt: f32,
        displays: &mut Vec<Display>,
    ) {
        let BlockEntityData::Piston(p) = &e.data else {
            return;
        };
        if p.moved_state == 0 {
            return;
        }
        let _ = pt;
        let dir = Dir::from_data3d(p.facing);
        let amount = if p.extending {
            p.progress - 1.0
        } else {
            1.0 - p.progress
        };
        let s = dir.step();
        let mut data = FxHashMap::default();
        data.insert(
            11,
            EntityDataValue::Vector3(Vec3f {
                x: s[0] as f32 * amount,
                y: s[1] as f32 * amount,
                z: s[2] as f32 * amount,
            }),
        );
        data.insert(23, EntityDataValue::BlockState(p.moved_state as i32));
        displays.push(Display {
            entity: "minecraft:block_display",
            pos: origin,
            data,
            yaw: 0.0,
            scale: 1.0,
            light,
        });
    }

    /// Uploads this frame's instances; call after `update`.
    pub fn prepare(&mut self, frame: &BlockEntityFrame) {
        self.gpu.prepare(
            frame.params.view_proj,
            frame.params.camera,
            frame.params.light,
        );
        self.display.prepare(&frame.params);
        self.flat
            .prepare(frame.params.view_proj, self.time, self.has_gateway);
        self.stats.draw_calls =
            self.gpu.draw_count() + self.display.stats().draws + self.flat.draw_calls();
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw(pass);
        self.display.draw(pass);
        self.flat.draw(pass);
    }

    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>, frame: &BlockEntityFrame) {
        self.prepare(frame);
        self.draw(pass);
    }
}

fn spawner_entity(id: &str) -> &'static str {
    static NAMES: std::sync::OnceLock<FxHashMap<String, &'static str>> = std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| {
        let v = mcr_data::v26_3();
        v.entity_types()
            .iter()
            .map(|(_, n)| (n.to_owned(), n))
            .collect()
    });
    names.get(id).copied().unwrap_or("minecraft:pig")
}
