//! Full-screen effects drawn under the HUD: vanilla's `ScreenEffectRenderer` (in-block view, water,
//! fire, totem) and `Hud#extractCameraOverlays` (vignette, scope, pumpkin, powder snow, portal,
//! nausea).

use std::f32::consts::PI;

use mcr_audio::SoundSource;
use mcr_client::ClientState;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, EquipmentSlot, component_type_id};
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::Hand;
use mcr_render::glam::{Mat4, Vec3};
use mcr_ui::{ItemIcons, ItemView, SpriteId, StackView, UiFrame};

use crate::assets::Assets;

const TOTEM_TICKS: i32 = 40;
const SCOPE_TARGET: f32 = 1.125;
const SCOPE_REST: f32 = 0.5;
const MAX_SCOPE_PIXELS: u32 = 1024;

#[derive(Debug)]
struct Totem {
    ticks: i32,
    offset: [f32; 2],
    stack: ItemStack,
}

/// Most textures a server's `camera_overlay` components may load before the cache stops growing.
const MAX_OVERLAY_TEXTURES: usize = 8;
const FIRE_GRID: usize = 8;
const UNDERWATER: &str = "minecraft:misc/underwater";
const POWDER_SNOW: &str = "minecraft:misc/powder_snow_outline";
const NAUSEA: &str = "minecraft:misc/nausea";
const PORTAL: &str = "minecraft:block/nether_portal";
const FIRE: &str = "minecraft:block/fire_1";
const PUMPKIN_BLUR: &str = "minecraft:misc/pumpkinblur";
const IN_BLOCK_TINT: u32 = 0xFF19_1919;
const FIRE_TINT: u32 = 0xE5FF_FFFF;
const TICKS_TO_FREEZE: f32 = 140.0;

/// What `ScreenEffectRenderer` draws over the hand: the in-block view, water and fire.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScreenEffects {
    /// The block state whose particle texture covers the view.
    pub block: Option<u32>,
    /// The light brightness at the eye while it is under water (first person only).
    pub water: Option<f32>,
    pub fire: bool,
    /// The hand projection's vertical FOV in degrees.
    pub hud_fov: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// Everything [`Overlays::draw`] needs for one frame.
#[derive(Debug, Clone, Copy)]
pub struct OverlayFrame<'a> {
    pub state: &'a ClientState,
    pub screen: ScreenEffects,
    /// The HUD is hidden (F1): only the screen effects draw.
    pub hidden: bool,
    pub first_person: bool,
    pub scoping: bool,
    pub border_warning: Option<f32>,
    pub portal: f32,
    pub nausea: f32,
    pub screen_effect_scale: f32,
    pub partial: f32,
    pub ticks: u64,
}

#[derive(Debug)]
struct Texture {
    key: String,
    sprite: Option<SpriteId>,
    /// Animation frames stacked vertically (square frames).
    frames: u32,
    /// The `.mcmeta` asks for `blur`: vanilla samples it linearly.
    linear: bool,
}

#[derive(Debug, Default)]
pub struct Overlays {
    scope: Option<SpriteId>,
    scope_loaded: bool,
    scope_scale: f32,
    totem: Option<Totem>,
    seed: u32,
    vignette: Option<SpriteId>,
    vignette_loaded: bool,
    vignette_linear: bool,
    vignette_brightness: f32,
    textures: Vec<Texture>,
    block: Option<(u16, Option<SpriteId>)>,
    owned: mcr_ui::OwnedSprites,
}

/// Vanilla's `Lightmap.getBrightness` for a raw light level, without the dimension's ambient light.
pub fn level_brightness(raw: u8) -> f32 {
    let v = f32::from(raw.min(15)) / 15.0;
    v / (4.0 - 3.0 * v)
}

/// The vignette tint: darkening `alpha`, plus a red share near the world border.
pub fn vignette_tint(brightness: f32, warning: f32) -> (f32, f32) {
    let brightness = brightness.clamp(0.0, 1.0);
    if warning > 0.0 {
        let warning = warning.clamp(0.0, 1.0);
        let red = brightness * (1.0 - warning);
        let green_blue = brightness + (1.0 - brightness) * warning;
        (green_blue, (green_blue - red) * 0.5)
    } else {
        (brightness, 0.0)
    }
}

/// `Hud#extractPortalOverlay`'s alpha curve.
pub fn portal_alpha(intensity: f32) -> f32 {
    if intensity < 1.0 {
        let a = intensity * intensity;
        a * a * 0.8 + 0.2
    } else {
        intensity
    }
}

/// `Player#getPercentFrozen` from the synced `ticks_frozen` (entity data 7).
pub fn percent_frozen(state: &ClientState) -> f32 {
    match state.player.data.get(&7) {
        Some(EntityDataValue::Int(t)) if *t > 0 => {
            (*t as f32).min(TICKS_TO_FREEZE) / TICKS_TO_FREEZE
        }
        _ => 0.0,
    }
}

/// `Equippable#cameraOverlay` of a worn stack: its patch, else the carved pumpkin's default blur.
pub fn camera_overlay(stack: &ItemStack, slot: EquipmentSlot) -> Option<&str> {
    if stack.is_empty() {
        return None;
    }
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::Equippable(e) => Some(e),
        _ => None,
    });
    if let Some(e) = patched {
        return e
            .camera_overlay
            .as_ref()
            .filter(|_| e.slot == slot)
            .map(|id| id.as_str());
    }
    let removed =
        component_type_id("equippable").is_some_and(|id| stack.components.removed.contains(&id));
    let pumpkin = u32::try_from(stack.item)
        .ok()
        .and_then(|i| mcr_data::v26_3().item_name(i))
        == Some("minecraft:carved_pumpkin");
    (!removed && pumpkin && slot == EquipmentSlot::Head).then_some(PUMPKIN_BLUR)
}

fn vignette_alpha(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [255, 255, 255, p[0]])
        .collect()
}

fn decode_scope(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (w, h) = (info.width, info.height);
    if w == 0 || h == 0 || w > MAX_SCOPE_PIXELS || h > MAX_SCOPE_PIXELS {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => return None,
    };
    (rgba.len() == (w * h * 4) as usize).then_some((w, h, rgba))
}

const VIGNETTE_FILE: &str = "assets/minecraft/textures/misc/vignette.png";

/// `blur` in a texture's `.mcmeta`.
fn blurred(assets: &Assets, file: &str) -> bool {
    assets
        .read(&format!("{file}.mcmeta"))
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("texture")?.get("blur")?.as_bool())
        .unwrap_or(false)
}

/// Sprite fractions half a texel inside the edges, so bilinear taps stay off atlas neighbours.
fn inset_uv(width: u32, height: u32) -> [[f32; 2]; 4] {
    let (du, dv) = (0.5 / width.max(1) as f32, 0.5 / height.max(1) as f32);
    let (u0, u1, v0, v1) = (du, 1.0 - du, dv, 1.0 - dv);
    [[u0, v0], [u0, v1], [u1, v1], [u1, v0]]
}

fn blit(frame: &mut UiFrame<'_>, sprite: SpriteId, rect: [f32; 4], argb: u32, linear: bool) {
    if !linear {
        frame.sprite_f(sprite, rect, argb);
        return;
    }
    let info = *frame.ui().sprites().info(sprite);
    let [x0, y0, x1, y1] = rect;
    frame.set_linear(true);
    frame.sprite_quad(
        sprite,
        [[x0, y0], [x0, y1], [x1, y1], [x1, y0]],
        inset_uv(info.width, info.height),
        argb,
    );
    frame.set_linear(false);
}

/// `ns:path` to the texture file `textures/path.png`; rejects paths that climb out of `textures`.
fn texture_file(id: &str) -> Option<String> {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    (!path.contains("..") && !path.is_empty() && path.len() < 256)
        .then(|| format!("assets/{ns}/textures/{path}.png"))
}

/// `t` in 0..1 along a repeating texture coordinate running from `a` to `b`: pieces that each stay
/// inside one repeat, as `[t0, t1, fraction at t0, fraction at t1]`.
fn tile_segments(a: f32, b: f32, out: &mut [[f32; 4]; 6]) -> usize {
    let span = b - a;
    if !span.is_finite() || span == 0.0 {
        return 0;
    }
    let up = span > 0.0;
    let (mut t, mut u, mut n) = (0.0f32, a, 0);
    while n < out.len() && (b - u) * span.signum() > 1.0e-5 {
        let cell = if up { u.floor() } else { u.ceil() - 1.0 };
        let next = if up { (cell + 1.0).min(b) } else { cell.max(b) };
        let t_next = (next - a) / span;
        out[n] = [t, t_next, u - cell, next - cell];
        n += 1;
        t = t_next;
        u = next;
    }
    n
}

/// Projects camera-space points through the hand projection to GUI pixels.
#[derive(Debug, Clone, Copy)]
struct Projector {
    tan_half: f32,
    aspect: f32,
    size: (f32, f32),
}

impl Projector {
    fn new(fov_degrees: f32, size: (i32, i32)) -> Self {
        let size = (size.0.max(1) as f32, size.1.max(1) as f32);
        Self {
            tan_half: (fov_degrees.clamp(1.0, 179.0).to_radians() * 0.5).tan(),
            aspect: size.0 / size.1,
            size,
        }
    }

    fn project(&self, p: Vec3) -> [f32; 2] {
        let depth = (-p.z).max(1.0e-3);
        let x = p.x / depth / (self.tan_half * self.aspect);
        let y = p.y / depth / self.tan_half;
        [(x + 1.0) * 0.5 * self.size.0, (1.0 - y) * 0.5 * self.size.1]
    }

    /// Draws the camera-space quad `c` (x0y0, x1y0, x1y1, x0y1) with sprite fractions `uv` at those
    /// corners, split into a `grid`² mesh so perspective stays close to vanilla's 3D quad.
    #[allow(clippy::too_many_arguments)]
    fn quad(
        &self,
        frame: &mut UiFrame<'_>,
        sprite: SpriteId,
        c: [Vec3; 4],
        uv: [[f32; 2]; 4],
        argb: u32,
        grid: usize,
    ) {
        let at = |s: f32, t: f32| -> (Vec3, [f32; 2]) {
            let bottom = c[0].lerp(c[1], s);
            let top = c[3].lerp(c[2], s);
            let lerp2 = |a: [f32; 2], b: [f32; 2], k: f32| {
                [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
            };
            let uv_bottom = lerp2(uv[0], uv[1], s);
            let uv_top = lerp2(uv[3], uv[2], s);
            (bottom.lerp(top, t), lerp2(uv_bottom, uv_top, t))
        };
        let n = grid.max(1);
        for i in 0..n {
            for j in 0..n {
                let (s0, s1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let (t0, t1) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
                let corners = [at(s0, t1), at(s0, t0), at(s1, t0), at(s1, t1)];
                frame.sprite_quad(
                    sprite,
                    corners.map(|(p, _)| self.project(p)),
                    corners.map(|(_, uv)| uv),
                    argb,
                );
            }
        }
    }
}

impl Overlays {
    pub fn new() -> Self {
        Self {
            scope_scale: SCOPE_REST,
            seed: 0x9e37_79b9,
            vignette_brightness: 1.0,
            ..Self::default()
        }
    }

    fn random(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    pub fn start_totem(&mut self, stack: ItemStack) {
        self.totem = Some(Totem {
            ticks: TOTEM_TICKS,
            offset: [self.random(), self.random()],
            stack,
        });
    }

    /// Eases the vignette toward the darkness at the camera, 1% a tick like vanilla.
    pub fn tick_vignette(&mut self, level_brightness: f32) {
        let target = (1.0 - level_brightness).clamp(0.0, 1.0);
        self.vignette_brightness += (target - self.vignette_brightness) * 0.01;
    }

    pub fn tick(&mut self, scoping: bool) {
        if let Some(t) = &mut self.totem {
            t.ticks -= 1;
            if t.ticks <= 0 {
                self.totem = None;
            }
        }
        let target = if scoping { SCOPE_TARGET } else { SCOPE_REST };
        self.scope_scale += (target - self.scope_scale) * 0.5;
    }

    /// The sprite of `id` (`ns:path` under `textures/`), decoded once and kept.
    fn texture(
        &mut self,
        frame: &mut UiFrame<'_>,
        assets: &Assets,
        id: &str,
    ) -> Option<(SpriteId, u32)> {
        if let Some(t) = self.textures.iter().find(|t| t.key == id) {
            return t.sprite.map(|s| (s, t.frames));
        }
        if self.textures.len() >= MAX_OVERLAY_TEXTURES {
            return None;
        }
        let file = texture_file(id);
        let decoded = file
            .as_deref()
            .and_then(|file| assets.read(file))
            .and_then(|b| decode_scope(&b));
        let linear = file.is_some_and(|f| blurred(assets, &f));
        let (sprite, frames) = match decoded {
            Some((w, h, px)) => (
                self.owned.add(frame.ui().sprites_mut(), w, h, &px),
                (h / w).max(1),
            ),
            None => (None, 1),
        };
        self.textures.push(Texture {
            key: id.to_owned(),
            sprite,
            frames,
            linear,
        });
        sprite.map(|s| (s, frames))
    }

    fn block_sprite(
        &mut self,
        frame: &mut UiFrame<'_>,
        assets: &Assets,
        state: u32,
    ) -> Option<SpriteId> {
        let particle = crate::effects::block_particle(assets, state)?;
        if let Some((layer, sprite)) = self.block
            && layer == particle.layer
        {
            return sprite;
        }
        if let Some((_, Some(old))) = self.block.take() {
            self.owned.free(frame.ui().sprites_mut(), old);
        }
        let sprite = self.owned.add(
            frame.ui().sprites_mut(),
            particle.width,
            particle.height,
            particle.rgba,
        );
        self.block = Some((particle.layer, sprite));
        sprite
    }

    /// Screen effects, the totem, then the HUD's camera overlays, in vanilla's order.
    pub fn draw(
        &mut self,
        frame: &mut UiFrame<'_>,
        icons: &mut ItemIcons,
        assets: &Assets,
        f: &OverlayFrame<'_>,
    ) {
        self.draw_screen_effects(frame, assets, f);
        if f.hidden {
            return;
        }
        self.draw_totem(frame, icons, f.partial);
        if let Some(warning) = f.border_warning {
            self.draw_vignette(frame, assets, warning);
        }
        if f.first_person {
            if f.scoping {
                self.draw_scope(frame, assets);
            } else {
                self.draw_equipment(frame, assets, f.state);
            }
        }
        let frozen = percent_frozen(f.state);
        if frozen > 0.0 {
            self.full_screen(frame, assets, POWDER_SNOW, white(frozen));
        }
        if f.portal > 0.0 {
            self.draw_portal(frame, assets, f.portal, f.ticks);
        } else if f.nausea > 0.0 && f.screen_effect_scale < 1.0 {
            self.draw_nausea(frame, assets, f.nausea * (1.0 - f.screen_effect_scale));
        }
    }

    fn draw_screen_effects(
        &mut self,
        frame: &mut UiFrame<'_>,
        assets: &Assets,
        f: &OverlayFrame<'_>,
    ) {
        let s = f.screen;
        let projector = Projector::new(s.hud_fov, frame.gui_size());
        let flat = |x0: f32, y0: f32, x1: f32, y1: f32| {
            [
                Vec3::new(x0, y0, -0.5),
                Vec3::new(x1, y0, -0.5),
                Vec3::new(x1, y1, -0.5),
                Vec3::new(x0, y1, -0.5),
            ]
        };
        if let Some(state) = s.block
            && let Some(sprite) = self.block_sprite(frame, assets, state)
        {
            let uv = [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
            projector.quad(
                frame,
                sprite,
                flat(-1.0, -1.0, 1.0, 1.0),
                uv,
                IN_BLOCK_TINT,
                1,
            );
        }
        if let Some(brightness) = s.water
            && let Some((sprite, _)) = self.texture(frame, assets, UNDERWATER)
        {
            let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
            let argb = (channel(0.1) << 24) | (channel(brightness) * 0x0001_0101);
            let u = (-s.yaw / 64.0).rem_euclid(1.0);
            let v = (s.pitch / 64.0).rem_euclid(1.0);
            let (mut us, mut vs) = ([[0.0; 4]; 6], [[0.0; 4]; 6]);
            let nu = tile_segments(u + 4.0, u, &mut us);
            let nv = tile_segments(v + 4.0, v, &mut vs);
            for a in &us[..nu] {
                for b in &vs[..nv] {
                    let x = |t: f32| -1.0 + 2.0 * t;
                    let quad = flat(x(a[0]), x(b[0]), x(a[1]), x(b[1]));
                    let uv = [[a[2], b[2]], [a[3], b[2]], [a[3], b[3]], [a[2], b[3]]];
                    projector.quad(frame, sprite, quad, uv, argb, 1);
                }
            }
        }
        if s.fire
            && let Some((sprite, frames)) = self.texture(frame, assets, FIRE)
        {
            let (v0, v1) = frame_rows(f.ticks, frames);
            for side in [1.0f32, -1.0] {
                let pose = Mat4::from_translation(Vec3::new(0.24 * side, -0.3, 0.0))
                    * Mat4::from_rotation_y(-side * PI / 18.0);
                let quad = flat(-0.5, -0.5, 0.5, 0.5).map(|p| pose.transform_point3(p));
                let uv = [[1.0, v1], [0.0, v1], [0.0, v0], [1.0, v0]];
                projector.quad(frame, sprite, quad, uv, FIRE_TINT, FIRE_GRID);
            }
        }
    }

    fn full_screen(&mut self, frame: &mut UiFrame<'_>, assets: &Assets, id: &str, argb: u32) {
        if argb >> 24 == 0 {
            return;
        }
        let Some((sprite, _)) = self.texture(frame, assets, id) else {
            return;
        };
        let (w, h) = frame.gui_size();
        let rect = [0.0, 0.0, w as f32, h as f32];
        blit(frame, sprite, rect, argb, self.linear(id));
    }

    fn linear(&self, id: &str) -> bool {
        self.textures.iter().any(|t| t.key == id && t.linear)
    }

    fn draw_equipment(&mut self, frame: &mut UiFrame<'_>, assets: &Assets, state: &ClientState) {
        const ARMOR: [(i32, EquipmentSlot); 4] = [
            (36, EquipmentSlot::Feet),
            (37, EquipmentSlot::Legs),
            (38, EquipmentSlot::Chest),
            (39, EquipmentSlot::Head),
        ];
        for (index, slot) in ARMOR {
            let Some(stack) = mcr_client::inventory_index_to_menu_slot(index)
                .and_then(|i| state.inventory.player.slots.get(i))
            else {
                continue;
            };
            if let Some(id) = camera_overlay(stack, slot) {
                self.full_screen(frame, assets, id, 0xFFFF_FFFF);
            }
        }
    }

    fn draw_portal(
        &mut self,
        frame: &mut UiFrame<'_>,
        assets: &Assets,
        intensity: f32,
        ticks: u64,
    ) {
        let Some((sprite, frames)) = self.texture(frame, assets, PORTAL) else {
            return;
        };
        let (v0, v1) = frame_rows(ticks, frames);
        let (w, h) = frame.gui_size();
        let (w, h) = (w as f32, h as f32);
        frame.sprite_quad(
            sprite,
            [[0.0, 0.0], [0.0, h], [w, h], [w, 0.0]],
            [[0.0, v0], [0.0, v1], [1.0, v1], [1.0, v0]],
            white(portal_alpha(intensity)),
        );
    }

    fn draw_nausea(&mut self, frame: &mut UiFrame<'_>, assets: &Assets, strength: f32) {
        let Some((sprite, _)) = self.texture(frame, assets, NAUSEA) else {
            return;
        };
        let (w, h) = frame.gui_size();
        let (w, h) = (w as f32, h as f32);
        let size = 2.0 + (1.0 - 2.0) * strength.clamp(0.0, 1.0);
        let (hw, hh) = (w * 0.5 * size, h * 0.5 * size);
        let (cx, cy) = (w * 0.5, h * 0.5);
        let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
        let argb = 0xFF00_0000
            | channel(0.2 * strength) << 16
            | channel(0.4 * strength) << 8
            | channel(0.2 * strength);
        frame.set_blend(mcr_ui::Blend::Additive);
        let rect = [cx - hw, cy - hh, cx + hw, cy + hh];
        blit(frame, sprite, rect, argb, self.linear(NAUSEA));
        frame.set_blend(mcr_ui::Blend::Normal);
    }

    fn draw_vignette(&mut self, frame: &mut UiFrame<'_>, assets: &Assets, warning: f32) {
        if !self.vignette_loaded {
            self.vignette_loaded = true;
            self.vignette_linear = blurred(assets, VIGNETTE_FILE);
            self.vignette = assets
                .read(VIGNETTE_FILE)
                .and_then(|b| decode_scope(&b))
                .and_then(|(w, h, px)| {
                    let sprites = frame.ui().sprites_mut();
                    self.owned.add(sprites, w, h, &vignette_alpha(&px))
                });
        }
        let Some(sprite) = self.vignette else { return };
        let (alpha, red) = vignette_tint(self.vignette_brightness, warning);
        let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
        let argb = channel(alpha) << 24 | channel(red) << 16;
        if argb >> 24 == 0 {
            return;
        }
        let (gui_w, gui_h) = frame.gui_size();
        let rect = [0.0, 0.0, gui_w as f32, gui_h as f32];
        blit(frame, sprite, rect, argb, self.vignette_linear);
    }

    fn draw_scope(&mut self, frame: &mut UiFrame<'_>, assets: &Assets) {
        if !self.scope_loaded {
            self.scope_loaded = true;
            self.scope = assets
                .read("assets/minecraft/textures/misc/spyglass_scope.png")
                .and_then(|b| decode_scope(&b))
                .and_then(|(w, h, px)| self.owned.add(frame.ui().sprites_mut(), w, h, &px));
        }
        let Some(sprite) = self.scope else { return };
        let (gui_w, gui_h) = frame.gui_size();
        let size = (gui_w.min(gui_h) as f32 * self.scope_scale) as i32;
        let (x, y) = ((gui_w - size) / 2, (gui_h - size) / 2);
        frame.sprite(sprite, x, y, size, size);
        let black = 0xFF00_0000;
        frame.fill_rect(0, y + size, gui_w, gui_h, black);
        frame.fill_rect(0, 0, gui_w, y, black);
        frame.fill_rect(0, y, x, y + size, black);
        frame.fill_rect(x + size, y, gui_w, y + size, black);
    }

    fn draw_totem(&mut self, frame: &mut UiFrame<'_>, icons: &mut ItemIcons, partial: f32) {
        let Some(t) = &self.totem else { return };
        let Some(view) = StackView::new(&t.stack) else {
            return;
        };
        let frac = ((TOTEM_TICKS - t.ticks) as f32 + partial) / TOTEM_TICKS as f32;
        let (f2, f3) = (frac * frac, frac * frac * frac);
        let yy = 10.25 * f3 * f2 - 24.95 * f2 * f2 + 25.5 * f3 - 13.8 * f2 + 4.0 * frac;
        let angle = yy * std::f32::consts::PI;
        let (gui_w, gui_h) = frame.gui_size();
        let sway = (angle * 2.0).sin().abs();
        let cx = gui_w as f32 / 2.0 + t.offset[0] * (gui_w as f32 / 4.0) * sway;
        let cy = gui_h as f32 / 2.0 + t.offset[1] * (gui_h as f32 / 4.0) * sway;
        let size = 50.0 + 175.0 * angle.sin();
        let spin = (900.0 * angle.sin().abs())
            .to_radians()
            .cos()
            .abs()
            .max(0.12);
        let (w, h) = ((size * spin) as i32, size as i32);
        if h <= 0 {
            return;
        }
        let item = ItemView {
            item: view.name(),
            props: &view,
            glint: false,
            cooldown: 0.0,
            bar: None,
        };
        icons.draw_sized(frame, &item, cx as i32 - w / 2, cy as i32 - h / 2, w, h);
    }
}

fn white(alpha: f32) -> u32 {
    ((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24 | 0x00FF_FFFF
}

/// The rows of the current frame of a vertical animation strip, one frame a tick.
fn frame_rows(ticks: u64, frames: u32) -> (f32, f32) {
    let frames = frames.max(1);
    let i = (ticks % u64::from(frames)) as f32;
    (i / frames as f32, (i + 1.0) / frames as f32)
}

/// Vanilla's `findTotem`: the first hand holding a death-protection item, else a plain totem.
pub fn totem_stack(state: &ClientState) -> ItemStack {
    use mcr_protocol::types::component::DataComponent;
    let totem = mcr_data::v26_3()
        .item_by_name("minecraft:totem_of_undying")
        .map_or(0, |i| i as i32);
    [Hand::Main, Hand::Off]
        .into_iter()
        .map(|hand| state.hand_stack(hand))
        .find(|s| {
            !s.is_empty()
                && (s.item == totem
                    || s.components
                        .added
                        .iter()
                        .any(|c| matches!(c, DataComponent::DeathProtection(_))))
        })
        .unwrap_or_else(|| ItemStack::new(totem, 1))
}

/// The mixer category vanilla's `Entity.getSoundSource` picks for an entity's own sounds.
pub fn entity_sound_source(state: &ClientState, entity_id: i32) -> SoundSource {
    if entity_id == state.player.entity_id {
        return SoundSource::Players;
    }
    let data = mcr_data::v26_3();
    let Some(kind) = state.entities.get(entity_id).map(|e| e.kind as u32) else {
        return SoundSource::Neutral;
    };
    if data.entity_type_name(kind) == Some("minecraft:player") {
        SoundSource::Players
    } else if data
        .entity_type_info(kind)
        .is_some_and(|i| i.category == "monster")
    {
        SoundSource::Hostile
    } else {
        SoundSource::Neutral
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_scale_converges_and_returns() {
        let mut o = Overlays::new();
        for _ in 0..12 {
            o.tick(true);
        }
        assert!(o.scope_scale > 1.1);
        for _ in 0..12 {
            o.tick(false);
        }
        assert!(o.scope_scale < 0.51);
    }

    #[test]
    fn totem_expires_after_forty_ticks() {
        let mut o = Overlays::new();
        o.start_totem(ItemStack::default());
        for _ in 0..39 {
            o.tick(false);
        }
        assert!(o.totem.is_some());
        o.tick(false);
        assert!(o.totem.is_none());
    }

    #[test]
    fn border_warning_tints_red_and_keeps_full_strength_at_the_edge() {
        assert_eq!(vignette_tint(0.3, 0.0), (0.3, 0.0));
        let (alpha, red) = vignette_tint(0.0, 1.0);
        assert_eq!(alpha, 1.0);
        assert!(red > 0.4);
        assert!(level_brightness(15) > 0.99 && level_brightness(0) == 0.0);
    }

    #[test]
    fn vignette_eases_toward_darkness() {
        let mut o = Overlays::new();
        o.tick_vignette(1.0);
        assert!(o.vignette_brightness < 1.0 && o.vignette_brightness > 0.98);
    }

    #[test]
    fn portal_alpha_and_frozen_share() {
        assert!((portal_alpha(0.5) - (0.0625 * 0.8 + 0.2)).abs() < 1e-6);
        assert_eq!(portal_alpha(1.0), 1.0);
        let mut state = ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil());
        assert_eq!(percent_frozen(&state), 0.0);
        state.player.data.insert(7, EntityDataValue::Int(70));
        assert_eq!(percent_frozen(&state), 0.5);
        state.player.data.insert(7, EntityDataValue::Int(i32::MAX));
        assert_eq!(percent_frozen(&state), 1.0);
    }

    #[test]
    fn pumpkin_overlay_only_from_the_head_slot() {
        let pumpkin = mcr_data::v26_3()
            .item_by_name("minecraft:carved_pumpkin")
            .unwrap() as i32;
        let stack = ItemStack::new(pumpkin, 1);
        assert_eq!(
            camera_overlay(&stack, EquipmentSlot::Head),
            Some(PUMPKIN_BLUR)
        );
        assert_eq!(camera_overlay(&stack, EquipmentSlot::Chest), None);
        assert_eq!(
            camera_overlay(&ItemStack::default(), EquipmentSlot::Head),
            None
        );
        assert!(texture_file("minecraft:../../secret").is_none());
        assert_eq!(
            texture_file("minecraft:misc/pumpkinblur").as_deref(),
            Some("assets/minecraft/textures/misc/pumpkinblur.png")
        );
    }

    #[test]
    fn water_tiles_split_at_texture_repeats() {
        let mut out = [[0.0; 4]; 6];
        let n = tile_segments(4.25, 0.25, &mut out);
        assert_eq!(n, 5);
        assert_eq!(out[0], [0.0, 0.0625, 0.25, 0.0]);
        assert_eq!(out[4][3], 0.25);
        let total: f32 = out[..n].iter().map(|s| s[1] - s[0]).sum();
        assert!((total - 1.0).abs() < 1e-5);
        assert_eq!(tile_segments(1.0, 1.0, &mut out), 0);
    }

    #[test]
    fn overlays_draw_in_vanilla_order_with_their_alpha() {
        let jar = std::path::Path::new("../../.cache/versions/26.3/client.jar");
        if !jar.exists() {
            return;
        }
        let assets = Assets::load(jar).unwrap();
        let mut ui = mcr_ui::Ui::load(&assets.resources).unwrap();
        let mut icons = ItemIcons::load(&assets.resources).unwrap();
        let data = mcr_data::v26_3();
        let mut state = ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil());
        let pumpkin = data.item_by_name("minecraft:carved_pumpkin").unwrap() as i32;
        let head = mcr_client::inventory_index_to_menu_slot(39).unwrap();
        state.inventory.player.slots[head] = ItemStack::new(pumpkin, 1);
        state.player.data.insert(7, EntityDataValue::Int(70));
        let stone = data.block_by_name("minecraft:stone").unwrap();
        let (stone, _) = data.block_info(stone).unwrap().states;
        let frame_input = OverlayFrame {
            state: &state,
            screen: ScreenEffects {
                block: Some(stone),
                water: Some(1.0),
                fire: true,
                hud_fov: 70.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            hidden: false,
            first_person: true,
            scoping: false,
            border_warning: None,
            portal: 0.5,
            nausea: 1.0,
            screen_effect_scale: 0.0,
            partial: 0.0,
            ticks: 3,
        };
        let mut overlays = Overlays::new();
        let mut frame = ui.frame(854, 480);
        overlays.draw(&mut frame, &mut icons, &assets, &frame_input);
        drop(frame);
        let mut colors: Vec<[u8; 4]> = ui.batch().vertices.chunks(4).map(|q| q[0].color).collect();
        colors.dedup();
        let first = &ui.batch().vertices[..4];
        let xs = first.iter().map(|v| v.pos[0]);
        assert!(xs.clone().fold(f32::MAX, f32::min) < 0.0 && xs.fold(f32::MIN, f32::max) > 427.0);
        assert_eq!(
            colors,
            vec![
                [0x19, 0x19, 0x19, 255],
                [255, 255, 255, 25],
                [255, 255, 255, 0xE5],
                [255, 255, 255, 255],
                [255, 255, 255, 127],
                [255, 255, 255, 63],
            ]
        );
        let mut hidden = ui.frame(854, 480);
        overlays.draw(
            &mut hidden,
            &mut icons,
            &assets,
            &OverlayFrame {
                hidden: true,
                ..frame_input
            },
        );
        drop(hidden);
        assert_eq!(
            ui.batch().vertices.chunks(4).last().unwrap()[0].color[3],
            0xE5
        );
    }

    #[test]
    fn hostile_scope_textures_are_rejected() {
        assert!(decode_scope(&[1, 2, 3]).is_none());
    }

    #[test]
    fn linear_overlays_stay_half_a_texel_inside() {
        let uv = inset_uv(256, 4);
        assert_eq!(uv[0], [0.5 / 256.0, 0.125]);
        assert_eq!(uv[2], [1.0 - 0.5 / 256.0, 0.875]);
        assert_eq!(inset_uv(0, 0)[2], [0.5, 0.5]);
    }
}
