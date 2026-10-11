use std::collections::HashMap;
use std::io::Cursor;

use serde::Deserialize;

use crate::bake::{SpriteId, SpriteTable};
use crate::error::{AssetError, Result};
use crate::resources::ResourceManager;

pub const DEFAULT_MIP_LEVELS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Transparency {
    Opaque,
    Cutout,
    Translucent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationFrame {
    /// Layer offset from the sprite's first layer.
    pub layer_offset: u32,
    pub ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    pub frames: Vec<AnimationFrame>,
    pub interpolate: bool,
}

/// `MipmapStrategy` of a texture's `.mcmeta`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MipStrategy {
    #[default]
    Auto,
    Mean,
    Cutout,
    StrictCutout,
    DarkCutout,
}

/// The `texture` section of a `.mcmeta`: how the mip chain keeps cutout coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MipSettings {
    pub strategy: MipStrategy,
    pub cutoff_bias: f32,
}

impl MipSettings {
    /// The alpha test the coverage is preserved for, or `None` for a plain mean.
    fn cutout_ref(self, frames: &[Vec<u8>]) -> Option<f32> {
        let strategy = match self.strategy {
            MipStrategy::Auto
                if frames
                    .iter()
                    .any(|f| f.as_chunks::<4>().0.iter().any(|p| p[3] == 0)) =>
            {
                MipStrategy::Cutout
            }
            MipStrategy::Auto => MipStrategy::Mean,
            s => s,
        };
        match strategy {
            MipStrategy::StrictCutout => Some(STRICT_ALPHA_REF),
            MipStrategy::Cutout | MipStrategy::DarkCutout => Some(ALPHA_REF),
            MipStrategy::Mean | MipStrategy::Auto => None,
        }
    }
}

const ALPHA_REF: f32 = 0.5;
const STRICT_ALPHA_REF: f32 = 0.3;

#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
    pub animation: Option<Animation>,
    pub mip: MipSettings,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FrameJson {
    Index(u32),
    Timed { index: u32, time: Option<u32> },
}

#[derive(Deserialize)]
struct AnimationJson {
    frametime: Option<u32>,
    interpolate: Option<bool>,
    width: Option<u32>,
    height: Option<u32>,
    frames: Option<Vec<FrameJson>>,
}

#[derive(Deserialize)]
struct TextureJson {
    mipmap_strategy: Option<MipStrategy>,
    alpha_cutoff_bias: Option<f32>,
}

#[derive(Deserialize)]
struct McmetaJson {
    animation: Option<AnimationJson>,
    texture: Option<TextureJson>,
}

fn decode_png(name: &str, bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let png_err = |e: &dyn std::fmt::Display| AssetError::Png {
        path: name.to_owned(),
        message: e.to_string(),
    };
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| png_err(&e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| png_err(&"image too large"))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| png_err(&e))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err(png_err(&"unexpanded palette")),
    };
    Ok((info.width, info.height, rgba))
}

pub fn decode_sprite(name: &str, png: &[u8], mcmeta: Option<&[u8]>) -> Result<Sprite> {
    let (width, height, rgba) = decode_png(name, png)?;
    let (animation, texture) = match mcmeta {
        Some(bytes) => {
            let meta: McmetaJson =
                serde_json::from_slice(bytes).map_err(|source| AssetError::Json {
                    path: format!("{name}.mcmeta"),
                    source,
                })?;
            (meta.animation, meta.texture)
        }
        None => (None, None),
    };
    let mip = texture.map_or_else(MipSettings::default, |t| MipSettings {
        strategy: t.mipmap_strategy.unwrap_or_default(),
        cutoff_bias: t
            .alpha_cutoff_bias
            .filter(|b| b.is_finite())
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0),
    });
    let Some(anim) = animation else {
        return Ok(Sprite {
            width,
            height,
            frames: vec![rgba],
            animation: None,
            mip,
        });
    };

    let (fw, fh) = match (anim.width, anim.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, height),
        (None, Some(h)) => (width, h),
        (None, None) => {
            let m = width.min(height);
            (m, m)
        }
    };
    if fw == 0 || fh == 0 || width % fw != 0 || height % fh != 0 {
        return Err(AssetError::invalid(
            name,
            format!("{width}x{height} image is not a multiple of {fw}x{fh} frames"),
        ));
    }
    let columns = width / fw;
    let count = columns * (height / fh);
    let row_bytes = (fw * 4) as usize;
    let frames: Vec<Vec<u8>> = (0..count)
        .map(|i| {
            let (cx, cy) = (i % columns, i / columns);
            let mut data = Vec::with_capacity(row_bytes * fh as usize);
            for y in 0..fh {
                let start = (((cy * fh + y) * width + cx * fw) * 4) as usize;
                data.extend_from_slice(&rgba[start..start + row_bytes]);
            }
            data
        })
        .collect();

    let default_ticks = anim.frametime.unwrap_or(1).max(1);
    let sequence: Vec<AnimationFrame> = match anim.frames {
        Some(list) => list
            .into_iter()
            .map(|f| match f {
                FrameJson::Index(index) => (index, default_ticks),
                FrameJson::Timed { index, time } => (index, time.unwrap_or(default_ticks).max(1)),
            })
            .map(|(index, ticks)| AnimationFrame {
                layer_offset: index,
                ticks,
            })
            .collect(),
        None => (0..count)
            .map(|layer_offset| AnimationFrame {
                layer_offset,
                ticks: default_ticks,
            })
            .collect(),
    };
    if sequence.iter().any(|f| f.layer_offset >= count) {
        return Err(AssetError::invalid(
            name,
            "animation frame index out of range",
        ));
    }
    let animation = (count > 1).then_some(Animation {
        frames: sequence,
        interpolate: anim.interpolate.unwrap_or(false),
    });
    Ok(Sprite {
        width: fw,
        height: fh,
        frames,
        animation,
        mip,
    })
}

/// The decoded texture; `None` when absent or when a (possibly hostile) pack's file does not decode.
pub(crate) fn load_sprite(resources: &ResourceManager, path: &str) -> Option<Sprite> {
    let png = match resources.read(path) {
        Ok(png) => png?,
        Err(e) => {
            tracing::warn!("{path}: {e}");
            return None;
        }
    };
    let meta = resources.read(&format!("{path}.mcmeta")).ok().flatten();
    match decode_sprite(path, &png, meta.as_deref()) {
        Ok(sprite) => Some(sprite),
        Err(e) => {
            tracing::warn!("{e}");
            decode_sprite(path, &png, None).ok()
        }
    }
}

pub fn missing_sprite() -> Sprite {
    let data = (0..16 * 16)
        .flat_map(|i| {
            let (x, y) = (i % 16, i / 16);
            if (x < 8) == (y < 8) {
                [248, 0, 248, 255]
            } else {
                [0, 0, 0, 255]
            }
        })
        .collect();
    Sprite {
        width: 16,
        height: 16,
        frames: vec![data],
        animation: None,
        mip: MipSettings::default(),
    }
}

pub fn classify(rgba: &[u8]) -> Transparency {
    let mut result = Transparency::Opaque;
    for px in rgba.as_chunks::<4>().0 {
        match px[3] {
            255 => {}
            0 => result = result.max(Transparency::Cutout),
            _ => return Transparency::Translucent,
        }
    }
    result
}

/// `MipmapGenerator.alphaTestCoverage`: the share of 4x4 bilinear subsamples that pass `alpha_ref`.
fn alpha_coverage(rgba: &[u8], width: u32, height: u32, alpha_ref: f32, scale: f32) -> f32 {
    if width < 2 || height < 2 {
        // Vanilla divides by zero here; the NaN keeps the scale at 1 and only the bias applies.
        return f32::NAN;
    }
    let a = |x: u32, y: u32| {
        (f32::from(rgba[((y * width + x) * 4 + 3) as usize]) / 255.0 * scale).clamp(0.0, 1.0)
    };
    let mut coverage = 0.0;
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            let (a00, a10, a01, a11) = (a(x, y), a(x + 1, y), a(x, y + 1), a(x + 1, y + 1));
            let mut hits = 0u32;
            for sy in 0..4 {
                let fy = (sy as f32 + 0.5) / 4.0;
                for sx in 0..4 {
                    let fx = (sx as f32 + 0.5) / 4.0;
                    let v = a00 * (1.0 - fx) * (1.0 - fy)
                        + a10 * fx * (1.0 - fy)
                        + a01 * (1.0 - fx) * fy
                        + a11 * fx * fy;
                    hits += u32::from(v > alpha_ref);
                }
            }
            coverage += hits as f32 / 16.0;
        }
    }
    coverage / ((width - 1) * (height - 1)) as f32
}

/// `MipmapGenerator.scaleAlphaToCoverage`: rescales a mip's alpha so as much of it passes the cutout
/// test as at the base level, then adds the texture's cutoff bias.
fn keep_coverage(rgba: &mut [u8], width: u32, height: u32, cut: CutoutMip) {
    let (mut lo, mut hi, mut scale) = (0.0f32, 4.0f32, 1.0f32);
    let (mut best, mut best_err) = (1.0f32, f32::MAX);
    for _ in 0..5 {
        let current = alpha_coverage(rgba, width, height, cut.alpha_ref, scale);
        let err = (current - cut.coverage).abs();
        if err < best_err {
            (best, best_err) = (scale, err);
        }
        if current < cut.coverage {
            lo = scale;
        } else if current > cut.coverage {
            hi = scale;
        } else {
            break;
        }
        scale = (lo + hi) * 0.5;
    }
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = f32::from(p[3]) / 255.0 * best + cut.bias + 0.025;
        p[3] = (a.clamp(0.0, 1.0) * 255.0) as u8;
    }
}

/// What one layer's mips must preserve, from its base level.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CutoutMip {
    alpha_ref: f32,
    coverage: f32,
    bias: f32,
}

/// Alpha-weighted 2x2 box filter; odd edges clamp.
pub fn downsample(src: &[u8], width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((width / 2).max(1), (height / 2).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 3];
            let mut plain = [0u32; 3];
            let mut alpha = 0u32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(width - 1);
                let sy = (y * 2 + dy).min(height - 1);
                let i = ((sy * width + sx) * 4) as usize;
                let a = u32::from(src[i + 3]);
                for c in 0..3 {
                    sum[c] += u32::from(src[i + c]) * a;
                    plain[c] += u32::from(src[i + c]);
                }
                alpha += a;
            }
            for c in 0..3 {
                let v = (sum[c] + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(plain[c] / 4);
                out.push(v as u8);
            }
            out.push(((alpha + 2) / 4) as u8);
        }
    }
    (nw, nh, out)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteSlot {
    pub group: u32,
    pub first_layer: u32,
    pub layer_count: u32,
    pub transparency: Transparency,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureGroup {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    /// One buffer per mip level, each holding every layer back to back as RGBA8.
    pub mips: Vec<Vec<u8>>,
}

impl TextureGroup {
    pub fn mip_size(&self, level: usize) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    pub fn layer_bytes(&self, level: usize, layer: u32) -> &[u8] {
        let (w, h) = self.mip_size(level);
        let size = (w * h * 4) as usize;
        let start = layer as usize * size;
        &self.mips[level][start..start + size]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureArrays {
    pub groups: Vec<TextureGroup>,
    slots: Vec<Option<SpriteSlot>>,
    animations: HashMap<SpriteId, Animation>,
}

impl TextureArrays {
    pub fn slot(&self, id: SpriteId) -> Option<&SpriteSlot> {
        self.slots.get(id.0 as usize)?.as_ref()
    }

    pub fn animation(&self, id: SpriteId) -> Option<&Animation> {
        self.animations.get(&id)
    }

    pub fn animated(&self) -> impl Iterator<Item = (SpriteId, &Animation)> {
        self.animations.iter().map(|(id, a)| (*id, a))
    }
}

#[derive(Debug)]
struct GroupBuilder {
    width: u32,
    height: u32,
    data: Vec<u8>,
    layers: u32,
    cutouts: Vec<Option<CutoutMip>>,
}

#[derive(Debug)]
pub struct TextureArrayBuilder {
    max_mips: u32,
    groups: Vec<GroupBuilder>,
    by_size: HashMap<(u32, u32), usize>,
    slots: Vec<Option<SpriteSlot>>,
    animations: HashMap<SpriteId, Animation>,
}

impl Default for TextureArrayBuilder {
    fn default() -> Self {
        Self::new(DEFAULT_MIP_LEVELS)
    }
}

impl TextureArrayBuilder {
    pub fn new(max_mips: u32) -> Self {
        Self {
            max_mips,
            groups: Vec::new(),
            by_size: HashMap::new(),
            slots: Vec::new(),
            animations: HashMap::new(),
        }
    }

    pub fn add(&mut self, id: SpriteId, sprite: &Sprite) -> SpriteSlot {
        let key = (sprite.width, sprite.height);
        let group_idx = *self.by_size.entry(key).or_insert_with(|| {
            self.groups.push(GroupBuilder {
                width: key.0,
                height: key.1,
                data: Vec::new(),
                layers: 0,
                cutouts: Vec::new(),
            });
            self.groups.len() - 1
        });
        let group = &mut self.groups[group_idx];
        let first_layer = group.layers;
        let mut transparency = Transparency::Opaque;
        let alpha_ref = sprite.mip.cutout_ref(&sprite.frames);
        for frame in &sprite.frames {
            group.data.extend_from_slice(frame);
            group.layers += 1;
            group.cutouts.push(alpha_ref.map(|alpha_ref| CutoutMip {
                alpha_ref,
                coverage: alpha_coverage(frame, sprite.width, sprite.height, alpha_ref, 1.0),
                bias: sprite.mip.cutoff_bias,
            }));
            transparency = transparency.max(classify(frame));
        }
        let slot = SpriteSlot {
            group: group_idx as u32,
            first_layer,
            layer_count: sprite.frames.len() as u32,
            transparency,
        };
        let idx = id.0 as usize;
        if self.slots.len() <= idx {
            self.slots.resize(idx + 1, None);
        }
        self.slots[idx] = Some(slot);
        if let Some(anim) = &sprite.animation {
            self.animations.insert(id, anim.clone());
        }
        slot
    }

    pub fn add_from_resources(
        &mut self,
        resources: &ResourceManager,
        sprites: &SpriteTable,
    ) -> Result<Vec<SpriteId>> {
        let mut missing = Vec::new();
        for (id, name) in sprites.iter() {
            if name.path() == "missingno" {
                self.add(id, &missing_sprite());
                continue;
            }
            let path = name.resource_path("textures", ".png");
            let sprite = match load_sprite(resources, &path) {
                Some(sprite) => sprite,
                None => {
                    missing.push(id);
                    missing_sprite()
                }
            };
            self.add(id, &sprite);
        }
        Ok(missing)
    }

    pub fn build(self) -> TextureArrays {
        let groups = self
            .groups
            .into_iter()
            .map(|g| {
                let levels = (g.width.min(g.height).ilog2()).min(self.max_mips) as usize;
                let mut mips = vec![g.data];
                let (mut w, mut h) = (g.width, g.height);
                for _ in 0..levels {
                    let layer_len = (w * h * 4) as usize;
                    let prev = mips.last().unwrap();
                    let mut next = Vec::new();
                    let mut dims = (w, h);
                    for (layer, cut) in prev.chunks_exact(layer_len).zip(&g.cutouts) {
                        let (nw, nh, mut data) = downsample(layer, w, h);
                        if let Some(cut) = cut {
                            keep_coverage(&mut data, nw, nh, *cut);
                        }
                        dims = (nw, nh);
                        next.extend_from_slice(&data);
                    }
                    (w, h) = dims;
                    mips.push(next);
                }
                TextureGroup {
                    width: g.width,
                    height: g.height,
                    layers: g.layers,
                    mips,
                }
            })
            .collect();
        TextureArrays {
            groups,
            slots: self.slots,
            animations: self.animations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{png_bytes, solid_png};

    fn strands() -> Vec<u8> {
        (0..16 * 16)
            .flat_map(|i| {
                if i % 4 == 0 {
                    [40, 120, 40, 255]
                } else {
                    [0; 4]
                }
            })
            .collect()
    }

    fn mip_coverage(meta: Option<&[u8]>) -> Vec<f32> {
        let sprite = decode_sprite("kelp", &png_bytes(16, 16, &strands()), meta).unwrap();
        let mut b = TextureArrayBuilder::new(4);
        b.add(SpriteId(0), &sprite);
        let g = &b.build().groups[0];
        (1..4)
            .map(|level| {
                let (w, h) = g.mip_size(level);
                alpha_coverage(g.layer_bytes(level, 0), w, h, ALPHA_REF, 1.0)
            })
            .collect()
    }

    #[test]
    fn cutout_mips_keep_thin_strands_visible() {
        let base = alpha_coverage(&strands(), 16, 16, ALPHA_REF, 1.0);
        let biased = br#"{"texture":{"alpha_cutoff_bias":0.1}}"#;
        for (level, c) in mip_coverage(Some(biased)).into_iter().take(2).enumerate() {
            assert!(c >= base * 0.75, "mip {}: {c} vs base {base}", level + 1);
        }
        let mean = br#"{"texture":{"mipmap_strategy":"mean"}}"#;
        assert_eq!(
            mip_coverage(Some(mean))[1],
            0.0,
            "a plain mean loses the strands"
        );
    }

    #[test]
    fn texture_section_parses_strategy_and_bias() {
        let meta = br#"{"texture":{"mipmap_strategy":"strict_cutout","alpha_cutoff_bias":0.1}}"#;
        let s = decode_sprite("t", &solid_png(16, 16, [1, 2, 3, 255]), Some(meta)).unwrap();
        assert_eq!(s.mip.strategy, MipStrategy::StrictCutout);
        assert!((s.mip.cutoff_bias - 0.1).abs() < 1e-6);
        assert_eq!(s.mip.cutout_ref(&s.frames), Some(STRICT_ALPHA_REF));
        let opaque = decode_sprite("t", &solid_png(16, 16, [1, 2, 3, 255]), None).unwrap();
        assert_eq!(opaque.mip.cutout_ref(&opaque.frames), None);
    }

    #[test]
    fn decodes_plain_png_of_any_size() {
        let s = decode_sprite("t", &solid_png(32, 8, [1, 2, 3, 255]), None).unwrap();
        assert_eq!((s.width, s.height, s.frames.len()), (32, 8, 1));
        assert!(s.animation.is_none());
    }

    #[test]
    fn splits_animation_strip() {
        let mut data = Vec::new();
        for frame in 0..3u8 {
            for _ in 0..16 * 16 {
                data.extend_from_slice(&[frame, 0, 0, 255]);
            }
        }
        let png = png_bytes(16, 48, &data);
        let meta = br#"{"animation":{"frametime":4,"interpolate":true,"frames":[2,{"index":0,"time":10},1]}}"#;
        let s = decode_sprite("t", &png, Some(meta)).unwrap();
        assert_eq!((s.width, s.height, s.frames.len()), (16, 16, 3));
        assert_eq!(s.frames[2][0], 2);
        let anim = s.animation.unwrap();
        assert!(anim.interpolate);
        let seq: Vec<_> = anim
            .frames
            .iter()
            .map(|f| (f.layer_offset, f.ticks))
            .collect();
        assert_eq!(seq, [(2, 4), (0, 10), (1, 4)]);
    }

    #[test]
    fn groups_by_size_and_assigns_layers() {
        let mut b = TextureArrayBuilder::new(4);
        let a = decode_sprite("a", &solid_png(16, 16, [9, 9, 9, 255]), None).unwrap();
        let big = decode_sprite("b", &solid_png(32, 32, [9, 9, 9, 128]), None).unwrap();
        let c = decode_sprite("c", &solid_png(16, 16, [9, 9, 9, 0]), None).unwrap();
        b.add(SpriteId(0), &a);
        b.add(SpriteId(1), &big);
        b.add(SpriteId(2), &c);
        let arrays = b.build();
        assert_eq!(arrays.groups.len(), 2);
        let s0 = arrays.slot(SpriteId(0)).unwrap();
        let s1 = arrays.slot(SpriteId(1)).unwrap();
        let s2 = arrays.slot(SpriteId(2)).unwrap();
        assert_ne!(s0.group, s1.group);
        assert_eq!((s2.group, s2.first_layer), (s0.group, 1));
        assert_eq!(s0.transparency, Transparency::Opaque);
        assert_eq!(s1.transparency, Transparency::Translucent);
        assert_eq!(s2.transparency, Transparency::Cutout);
        let g = &arrays.groups[s0.group as usize];
        assert_eq!(g.mips.len(), 5);
        assert_eq!(g.mip_size(4), (1, 1));
        assert_eq!(g.layer_bytes(4, 0), [9, 9, 9, 255]);
    }

    #[test]
    fn mip_filter_is_alpha_weighted() {
        let src = [200, 100, 50, 255, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0];
        let (w, h, out) = downsample(&src, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, [200, 100, 50, 64]);
    }
}
