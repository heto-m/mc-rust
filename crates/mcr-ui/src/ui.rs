use std::ops::Range;

use mcr_assets::ResourceManager;
use mcr_protocol::types::text::Component;

use crate::error::Result;
use crate::font::{FontOptions, Fonts};
use crate::lang::DEFAULT_LANGUAGE;
use crate::sprites::{SpriteId, Sprites};
use crate::style::{Cell, Style, scale_rgb};
use crate::text::{Lang, flatten, parse_legacy, wrap};

pub const LINE_HEIGHT: i32 = 9;
const MIN_GUI_WIDTH: u32 = 320;
const MIN_GUI_HEIGHT: u32 = 240;

/// Vanilla's auto GUI scale: the largest integer that still fits 320x240 GUI pixels.
pub fn gui_scale(width: u32, height: u32, max_scale: u32) -> u32 {
    let mut scale = 1;
    while (max_scale == 0 || scale < max_scale)
        && width / (scale + 1) >= MIN_GUI_WIDTH
        && height / (scale + 1) >= MIN_GUI_HEIGHT
    {
        scale += 1;
    }
    scale
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Sprite,
    Glyph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRef {
    pub kind: PageKind,
    pub index: u32,
}

/// How a draw command meets the framebuffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    /// Premultiplied alpha.
    #[default]
    Normal,
    /// Vanilla's `CROSSHAIR` pipeline: white inverts what is underneath.
    Invert,
    /// Vanilla's `ADDITIVE` blend function (the nausea overlay).
    Additive,
}

#[derive(Clone, Debug)]
pub struct DrawCmd {
    pub page: PageRef,
    pub scissor: [u32; 4],
    pub blend: Blend,
    pub quads: Range<u32>,
}

#[derive(Debug, Default)]
pub struct Batch {
    pub vertices: Vec<Vertex>,
    pub cmds: Vec<DrawCmd>,
    pub scale: u32,
    pub framebuffer: (u32, u32),
    /// First quad drawn above the app's 3D screen preview, when a screen marked one.
    pub preview_split: Option<u32>,
    blend: Blend,
}

impl Batch {
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 4
    }

    fn push(
        &mut self,
        page: PageRef,
        scissor: [u32; 4],
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        let quad = self.quad_count() as u32;
        for (pos, uv) in corners.into_iter().zip(uv) {
            self.vertices.push(Vertex { pos, uv, color });
        }
        let blend = self.blend;
        match self.cmds.last_mut() {
            Some(cmd) if cmd.page == page && cmd.scissor == scissor && cmd.blend == blend => {
                cmd.quads.end = quad + 1
            }
            _ => self.cmds.push(DrawCmd {
                page,
                scissor,
                blend,
                quads: quad..quad + 1,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Placed {
    x: f32,
    effect_x0: f32,
    glyph: u32,
    style: Style,
    text_color: u32,
    shadow_color: u32,
    bold_offset: f32,
    shadow_offset: f32,
    advance: f32,
}

#[derive(Debug)]
pub struct Ui {
    fonts: Fonts,
    sprites: Sprites,
    lang: std::sync::Arc<Lang>,
    batch: Batch,
    placed_scratch: Vec<Placed>,
    rng: u32,
    frames: u64,
}

impl Ui {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        Self::load_with(resources, FontOptions::default())
    }

    pub fn load_with(resources: &ResourceManager, options: FontOptions) -> Result<Self> {
        Ok(Self {
            fonts: Fonts::load(resources, options)?,
            sprites: Sprites::load(resources)?,
            lang: std::sync::Arc::new(Lang::load(resources, DEFAULT_LANGUAGE)?),
            batch: Batch::default(),
            frames: 0,
            placed_scratch: Vec::new(),
            rng: 0x2545_F491,
        })
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    pub fn fonts_mut(&mut self) -> &mut Fonts {
        &mut self.fonts
    }

    pub fn sprites(&self) -> &Sprites {
        &self.sprites
    }

    pub fn sprites_mut(&mut self) -> &mut Sprites {
        &mut self.sprites
    }

    pub fn lang(&self) -> &Lang {
        &self.lang
    }

    /// The current language, for code that draws text off this `Ui` (chat trust checks).
    pub fn shared_lang(&self) -> std::sync::Arc<Lang> {
        self.lang.clone()
    }

    pub fn set_language(&mut self, resources: &ResourceManager, code: &str) -> Result<()> {
        self.lang = std::sync::Arc::new(Lang::load(resources, code)?);
        Ok(())
    }

    pub fn batch(&self) -> &Batch {
        &self.batch
    }

    pub fn sprite_id(&self, name: &str) -> Option<SpriteId> {
        self.sprites.id(name)
    }

    pub fn flatten(&self, component: &Component) -> Vec<Cell> {
        flatten(&self.lang, component)
    }

    pub fn width(&mut self, cells: &[Cell]) -> f32 {
        cells
            .iter()
            .map(|c| self.fonts.advance(c.style.font, c.ch, c.style.bold()))
            .sum()
    }

    pub fn text_width(&mut self, text: &str) -> f32 {
        self.width(&parse_legacy(text))
    }

    pub fn wrap(&mut self, cells: &[Cell], max_width: f32) -> Vec<Range<usize>> {
        let fonts = &mut self.fonts;
        wrap(cells, max_width, |c| {
            fonts.advance(c.style.font, c.ch, c.style.bold())
        })
    }

    /// Starts a frame with the automatic GUI scale.
    pub fn frame(&mut self, width: u32, height: u32) -> UiFrame<'_> {
        self.frame_scaled(width, height, 0)
    }

    /// `max_scale` of 0 means unlimited, like vanilla's "Auto" GUI scale option.
    pub fn frame_scaled(&mut self, width: u32, height: u32, max_scale: u32) -> UiFrame<'_> {
        let scale = gui_scale(width, height, max_scale);
        self.frames += 1;
        self.batch.vertices.clear();
        self.batch.cmds.clear();
        self.batch.preview_split = None;
        self.batch.blend = Blend::Normal;
        self.batch.scale = scale;
        self.batch.framebuffer = (width, height);
        UiFrame {
            gui_width: width.div_ceil(scale) as i32,
            gui_height: height.div_ceil(scale) as i32,
            scale,
            scissors: Vec::new(),
            text_xf: None,
            ui: self,
        }
    }
}

#[derive(Debug)]
pub struct UiFrame<'a> {
    ui: &'a mut Ui,
    scale: u32,
    gui_width: i32,
    gui_height: i32,
    scissors: Vec<[i32; 4]>,
    text_xf: Option<(f32, f32, f32)>,
}

fn color_bytes(argb: u32) -> [u8; 4] {
    [
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    ]
}

fn rect_corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[0], r[3]], [r[2], r[3]], [r[2], r[1]]]
}

fn uv_corners(uv: [f32; 4]) -> [[f32; 2]; 4] {
    [
        [uv[0], uv[1]],
        [uv[0], uv[3]],
        [uv[2], uv[3]],
        [uv[2], uv[1]],
    ]
}

impl UiFrame<'_> {
    pub fn gui_size(&self) -> (i32, i32) {
        (self.gui_width, self.gui_height)
    }

    /// Everything drawn after this goes above the screen's 3D preview, like vanilla's background layer.
    pub fn mark_preview_layer(&mut self) {
        self.ui.batch.preview_split = Some(self.ui.batch.quad_count() as u32);
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// Blend mode of everything drawn until the next call.
    pub fn set_blend(&mut self, blend: Blend) {
        self.ui.batch.blend = blend;
    }

    /// Number of UI frames started so far.
    pub fn number(&self) -> u64 {
        self.ui.frames
    }

    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    pub fn push_scissor(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let mut rect = [x0, y0, x1.max(x0), y1.max(y0)];
        if let Some(top) = self.scissors.last() {
            rect = [
                rect[0].max(top[0]),
                rect[1].max(top[1]),
                rect[2].min(top[2]).max(rect[0].max(top[0])),
                rect[3].min(top[3]).max(rect[1].max(top[1])),
            ];
        }
        self.scissors.push(rect);
    }

    pub fn pop_scissor(&mut self) {
        self.scissors.pop();
    }

    fn scissor_px(&self) -> [u32; 4] {
        let (fw, fh) = self.ui.batch.framebuffer;
        let Some(r) = self.scissors.last() else {
            return [0, 0, fw, fh];
        };
        let s = self.scale as i32;
        let clamp = |v: i32, max: u32| (v * s).clamp(0, max as i32) as u32;
        let (x0, y0) = (clamp(r[0], fw), clamp(r[1], fh));
        let (x1, y1) = (clamp(r[2], fw), clamp(r[3], fh));
        [x0, y0, x1 - x0, y1 - y0]
    }

    fn white(&self, page: PageRef) -> [f32; 2] {
        match page.kind {
            PageKind::Sprite => self.ui.sprites.atlas().white_uv(),
            PageKind::Glyph => self.ui.fonts.atlas().white_uv(),
        }
    }

    fn fill_f(&mut self, rect: [f32; 4], argb: u32) {
        let scissor = self.scissor_px();
        let page = match self.ui.batch.cmds.last() {
            Some(cmd) if cmd.scissor == scissor && cmd.blend == self.ui.batch.blend => cmd.page,
            _ => PageRef {
                kind: PageKind::Sprite,
                index: 0,
            },
        };
        let w = self.white(page);
        self.ui
            .batch
            .push(page, scissor, rect_corners(rect), [w; 4], color_bytes(argb));
    }

    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, argb: u32) {
        if x1 > x0 && y1 > y0 && argb >> 24 != 0 {
            self.fill_f([x0 as f32, y0 as f32, x1 as f32, y1 as f32], argb);
        }
    }

    /// `fill_rect` over a fractional GUI rectangle `[x0, y0, x1, y1]`.
    pub fn fill_rect_f(&mut self, rect: [f32; 4], argb: u32) {
        if rect[2] > rect[0] && rect[3] > rect[1] && argb >> 24 != 0 {
            self.fill_f(rect, argb);
        }
    }

    pub fn sprite(&mut self, id: SpriteId, x: i32, y: i32, w: i32, h: i32) {
        self.sprite_tinted(id, x, y, w, h, 0xFFFF_FFFF);
    }

    pub fn sprite_tinted(&mut self, id: SpriteId, x: i32, y: i32, w: i32, h: i32, argb: u32) {
        let info = *self.ui.sprites.info(id);
        let size = self.ui.sprites.atlas().size();
        let scissor = self.scissor_px();
        let page = PageRef {
            kind: PageKind::Sprite,
            index: info.page,
        };
        let batch = &mut self.ui.batch;
        info.layout(size, (x, y, w, h), |q| {
            batch.push(
                page,
                scissor,
                rect_corners(q.dst),
                uv_corners(q.uv),
                color_bytes(argb),
            );
        });
    }

    /// Stretches the whole sprite over a fractional GUI rectangle `[x0, y0, x1, y1]`.
    pub fn sprite_f(&mut self, id: SpriteId, dst: [f32; 4], argb: u32) {
        let info = *self.ui.sprites.info(id);
        let q = info.stretched(self.ui.sprites.atlas().size(), dst);
        let page = PageRef {
            kind: PageKind::Sprite,
            index: info.page,
        };
        let scissor = self.scissor_px();
        self.ui.batch.push(
            page,
            scissor,
            rect_corners(q.dst),
            uv_corners(q.uv),
            color_bytes(argb),
        );
    }

    /// Maps sprite fractions `uv` onto any GUI quad `corners` (top-left, bottom-left, bottom-right,
    /// top-right), for overlays that are projected rather than axis-aligned.
    pub fn sprite_quad(
        &mut self,
        id: SpriteId,
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        argb: u32,
    ) {
        let info = *self.ui.sprites.info(id);
        let q = info.stretched(self.ui.sprites.atlas().size(), [0.0, 0.0, 1.0, 1.0]);
        let page = PageRef {
            kind: PageKind::Sprite,
            index: info.page,
        };
        let scissor = self.scissor_px();
        let [u0, v0, u1, v1] = q.uv;
        let atlas_uv = uv.map(|[u, v]| [u0 + (u1 - u0) * u, v0 + (v1 - v0) * v]);
        self.ui
            .batch
            .push(page, scissor, corners, atlas_uv, color_bytes(argb));
    }

    /// Draws the `w`x`h` window of a sprite starting at pixel `(tx, ty)`, unscaled.
    pub fn sprite_region(
        &mut self,
        id: SpriteId,
        (tx, ty): (i32, i32),
        (x, y, w, h): (i32, i32, i32, i32),
    ) {
        let info = *self.ui.sprites.info(id);
        let size = self.ui.sprites.atlas().size();
        let q = info.region(size, (tx, ty), (x, y, w, h));
        let page = PageRef {
            kind: PageKind::Sprite,
            index: info.page,
        };
        let scissor = self.scissor_px();
        self.ui.batch.push(
            page,
            scissor,
            rect_corners(q.dst),
            uv_corners(q.uv),
            [255; 4],
        );
    }

    pub fn sprite_region_scaled(
        &mut self,
        id: SpriteId,
        src: (i32, i32, i32, i32),
        dst: (i32, i32, i32, i32),
    ) {
        let info = *self.ui.sprites.info(id);
        let size = self.ui.sprites.atlas().size();
        let q = info.region_scaled(size, src, dst);
        let page = PageRef {
            kind: PageKind::Sprite,
            index: info.page,
        };
        let scissor = self.scissor_px();
        self.ui.batch.push(
            page,
            scissor,
            rect_corners(q.dst),
            uv_corners(q.uv),
            [255; 4],
        );
    }

    /// Draws `§`-formatted text and returns its width.
    pub fn text(&mut self, text: &str, x: f32, y: f32, argb: u32, shadow: bool) -> f32 {
        let cells = parse_legacy(text);
        self.cells(&cells, x, y, argb, shadow)
    }

    /// Draws text scaled around `(x, y)`; returns the scaled width.
    pub fn cells_scaled(
        &mut self,
        cells: &[Cell],
        x: f32,
        y: f32,
        argb: u32,
        shadow: bool,
        scale: f32,
    ) -> f32 {
        self.text_xf = Some((x, y, scale));
        let width = self.cells(cells, x, y, argb, shadow);
        self.text_xf = None;
        width * scale
    }

    fn xf(&self, p: [f32; 2]) -> [f32; 2] {
        match self.text_xf {
            Some((ox, oy, s)) => [ox + (p[0] - ox) * s, oy + (p[1] - oy) * s],
            None => p,
        }
    }

    /// Vertical gradient from `top` to `bottom` (ARGB), drawn as thin strips.
    pub fn fill_gradient(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, top: u32, bottom: u32) {
        let height = y1 - y0;
        if height <= 0 || x1 <= x0 {
            return;
        }
        let strips = height.min(32);
        for i in 0..strips {
            let (a, b) = (y0 + height * i / strips, y0 + height * (i + 1) / strips);
            let t = (i as f32 + 0.5) / strips as f32;
            let lerp = |shift: u32| {
                let (p, q) = (
                    ((top >> shift) & 0xFF) as f32,
                    ((bottom >> shift) & 0xFF) as f32,
                );
                ((p + (q - p) * t).round() as u32) << shift
            };
            let color = lerp(24) | lerp(16) | lerp(8) | lerp(0);
            self.fill_rect(x0, a, x1, b, color);
        }
    }

    pub fn component(
        &mut self,
        component: &Component,
        x: f32,
        y: f32,
        argb: u32,
        shadow: bool,
    ) -> f32 {
        let cells = self.ui.flatten(component);
        self.cells(&cells, x, y, argb, shadow)
    }

    pub fn cells(&mut self, cells: &[Cell], x: f32, y: f32, argb: u32, shadow: bool) -> f32 {
        let argb = if argb >> 24 < 4 {
            argb | 0xFF00_0000
        } else {
            argb
        };
        let mut placed = std::mem::take(&mut self.ui.placed_scratch);
        placed.clear();
        let mut cx = x;
        for (i, cell) in cells.iter().enumerate() {
            let style = cell.style;
            let ui = &mut *self.ui;
            let mut glyph = ui.fonts.glyph(style.font, cell.ch);
            if style.obfuscated() && cell.ch != ' ' {
                glyph = ui.fonts.random_glyph(style.font, cell.ch, &mut ui.rng);
            }
            let info = ui.fonts.info(glyph);
            let bold = style.bold();
            let text_color = match style.color {
                Some(rgb) => (argb & 0xFF00_0000) | rgb,
                None => argb,
            };
            let shadow_color = match style.shadow {
                Some(s) => {
                    let text_alpha = (text_color >> 24) as f32 / 255.0;
                    if text_alpha != 1.0 {
                        let a = ((text_alpha * (s >> 24) as f32 / 255.0) * 255.0) as u32;
                        (a << 24) | (s & 0xFF_FFFF)
                    } else {
                        s
                    }
                }
                None if shadow => scale_rgb(text_color, 0.25),
                None => 0,
            };
            let advance = info.advance(bold);
            placed.push(Placed {
                x: cx,
                effect_x0: if i == 0 { cx - 1.0 } else { cx },
                glyph,
                style,
                text_color,
                shadow_color,
                bold_offset: if bold { info.bold_offset } else { 0.0 },
                shadow_offset: info.shadow_offset,
                advance,
            });
            cx += advance;
        }
        let placed_ref = &placed;
        for p in placed_ref.iter().filter(|p| p.shadow_color != 0) {
            let (ox, oy) = (p.shadow_offset, p.shadow_offset);
            self.glyph(p, p.x + ox, y + oy, p.shadow_color);
            if p.bold_offset != 0.0 {
                self.glyph(p, p.x + p.bold_offset + ox, y + oy, p.shadow_color);
            }
        }
        for p in placed_ref.iter().filter(|p| p.shadow_color != 0) {
            self.effects(p, y, p.shadow_offset, p.shadow_color);
        }
        for p in placed_ref.iter() {
            self.glyph(p, p.x, y, p.text_color);
            if p.bold_offset != 0.0 {
                self.glyph(p, p.x + p.bold_offset, y, p.text_color);
            }
        }
        for p in placed_ref.iter() {
            self.effects(p, y, 0.0, p.text_color);
        }
        self.ui.placed_scratch = placed;
        cx - x
    }

    fn effects(&mut self, p: &Placed, y: f32, offset: f32, color: u32) {
        let scissor = self.scissor_px();
        let page = PageRef {
            kind: PageKind::Glyph,
            index: 0,
        };
        let w = self.ui.fonts.atlas().white_uv();
        let x1 = p.x + p.advance;
        for (on, bottom) in [(p.style.strikethrough(), 4.5), (p.style.underlined(), 9.0)] {
            if on {
                let rect = [
                    p.effect_x0 + offset,
                    y + bottom - 1.0 + offset,
                    x1 + offset,
                    y + bottom + offset,
                ];
                let corners = rect_corners(rect).map(|c| self.xf(c));
                self.ui
                    .batch
                    .push(page, scissor, corners, [w; 4], color_bytes(color));
            }
        }
    }

    fn glyph(&mut self, p: &Placed, x: f32, y: f32, color: u32) {
        let baked = self.ui.fonts.baked(p.glyph);
        if baked.empty {
            return;
        }
        let italic = p.style.italic();
        let extra = if p.style.bold() { 0.1 } else { 0.0 };
        let (x0, x1) = (x + baked.left, x + baked.right);
        let (y0, y1) = (y + baked.up, y + baked.down);
        let shear_top = if italic { 1.0 - 0.25 * baked.up } else { 0.0 };
        let shear_bottom = if italic { 1.0 - 0.25 * baked.down } else { 0.0 };
        let corners = [
            [x0 + shear_top - extra, y0 - extra],
            [x0 + shear_bottom - extra, y1 + extra],
            [x1 + shear_bottom + extra, y1 + extra],
            [x1 + shear_top + extra, y0 - extra],
        ];
        let corners = corners.map(|c| self.xf(c));
        let scissor = self.scissor_px();
        let page = PageRef {
            kind: PageKind::Glyph,
            index: baked.page,
        };
        self.ui.batch.push(
            page,
            scissor,
            corners,
            uv_corners(baked.uv),
            color_bytes(color),
        );
    }
}
