//! Render pass replay: Java's off-heap command stream becomes one `wgpu::RenderPass`.
//!
//! The stream is native-endian `u32` words: `[op][payload]`, 64-bit values as two words (low first).
//! The descriptor is a list of `u64` slots (see [`PassDesc::parse`]).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use rustc_hash::FxHashMap;

use super::backend::{Backend, ClearValue, OpError, OpResult, PendingClear, State};
use super::pipeline::{Layout, Pipeline, Slot, TextureSample, VariantKey};
use super::resources::SampleClass;
use super::slab::{Handle, Kind};
use super::spv;

pub const P_PIPELINE: u32 = 1;
pub const P_UNIFORM_BUFFER: u32 = 2;
pub const P_UNIFORM_TEXTURE: u32 = 3;
pub const P_UNIFORM_NONE: u32 = 4;
pub const P_PUSH: u32 = 5;
pub const P_SCISSOR: u32 = 6;
pub const P_NO_SCISSOR: u32 = 7;
pub const P_VERTEX: u32 = 8;
pub const P_INDEX: u32 = 9;
pub const P_DRAW: u32 = 10;
pub const P_DRAW_INDEXED: u32 = 11;
pub const P_MULTI_DRAW: u32 = 12;
pub const P_MULTI_DRAW_INDEXED: u32 = 13;
pub const P_DRAW_INDIRECT: u32 = 14;
pub const P_DRAW_INDEXED_INDIRECT: u32 = 15;
pub const P_DEBUG_PUSH: u32 = 16;
pub const P_DEBUG_POP: u32 = 17;

/// Descriptor flags: the pass is cut here and continues in a later call (state is kept), or
/// this call continues a cut pass (state is restored, attachments load).
pub const PASS_CONTINUES: u64 = 1;
pub const PASS_RESUMES: u64 = 2;

/// Uniform slots a pipeline may declare; the push-constant block takes one more.
pub const MAX_UNIFORMS: usize = 16;
pub const MAX_COLOR_ATTACHMENTS: usize = 8;
const MAX_VERTEX_SLOTS: usize = 8;
const MAX_DEBUG_DEPTH: u32 = 64;
const MAX_BIND_CACHE: usize = 8192;
const MAX_DEBUG_LABEL: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum Res {
    #[default]
    None,
    Buffer(Handle, u64),
    Texture(Handle, Handle, bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BindKey {
    layout: u64,
    res: [Res; MAX_UNIFORMS + 1],
}

#[derive(Debug, Default)]
pub struct BindCache {
    map: FxHashMap<BindKey, wgpu::BindGroup>,
}

impl BindCache {
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Drops groups that keep destroyed resources alive.
    pub fn purge(&mut self, destroyed: &[Handle]) {
        if destroyed.is_empty() {
            return;
        }
        let dead: rustc_hash::FxHashSet<Handle> = destroyed.iter().copied().collect();
        self.map.retain(|k, _| {
            !k.res.iter().any(|r| match *r {
                Res::None => false,
                Res::Buffer(h, _) => dead.contains(&h),
                Res::Texture(v, s, _) => dead.contains(&v) || dead.contains(&s),
            })
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorAttachment {
    pub view: Handle,
    pub clear: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassDesc {
    pub colors: Vec<Option<ColorAttachment>>,
    pub depth: Option<(Handle, Option<f32>)>,
    pub area: [u32; 4],
    /// [`PASS_CONTINUES`] | [`PASS_RESUMES`].
    pub flags: u64,
}

impl PassDesc {
    /// `[count, count × (view, has_clear, r, g, b, a), depth_view, depth_has_clear, depth, x, y, w, h,
    /// flags]`, floats as `f32` bits.
    pub fn parse(slots: &[u64]) -> OpResult<Self> {
        let bad = || OpError::Invalid("malformed pass descriptor".into());
        let count = *slots.first().ok_or_else(bad)? as usize;
        if count > MAX_COLOR_ATTACHMENTS || slots.len() != 1 + count * 6 + 8 {
            return Err(bad());
        }
        let f = |v: u64| f32::from_bits(v as u32);
        let colors = (0..count)
            .map(|i| {
                let s = &slots[1 + i * 6..1 + i * 6 + 6];
                (s[0] != 0).then(|| ColorAttachment {
                    view: s[0],
                    clear: (s[1] != 0).then(|| [f(s[2]), f(s[3]), f(s[4]), f(s[5])]),
                })
            })
            .collect();
        let t = &slots[1 + count * 6..];
        let depth = (t[0] != 0).then(|| (t[0], (t[1] != 0).then(|| f(t[2]))));
        let u = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
        Ok(Self {
            colors,
            depth,
            area: [u(t[3]), u(t[4]), u(t[5]), u(t[6])],
            flags: t[7] & (PASS_CONTINUES | PASS_RESUMES),
        })
    }

    pub fn encode(&self) -> Vec<u64> {
        let mut out = vec![self.colors.len() as u64];
        for c in &self.colors {
            match c {
                None => out.extend([0; 6]),
                Some(c) => {
                    let clear = c.clear.unwrap_or_default();
                    out.extend([c.view, u64::from(c.clear.is_some())]);
                    out.extend(clear.map(|v| u64::from(v.to_bits())));
                }
            }
        }
        let (dv, dc) = self.depth.unwrap_or((0, None));
        out.extend([
            dv,
            u64::from(dc.is_some()),
            u64::from(dc.unwrap_or(0.0).to_bits()),
        ]);
        out.extend(self.area.map(u64::from));
        out.push(self.flags);
        out
    }
}

/// Builds pass streams (Rust tests and benches; Java has its own writer).
#[derive(Debug, Default, Clone)]
pub struct StreamWriter {
    pub words: Vec<u32>,
}

impl StreamWriter {
    fn op(&mut self, op: u32) -> &mut Self {
        self.words.push(op);
        self
    }

    fn u64(&mut self, v: u64) -> &mut Self {
        self.words.extend([v as u32, (v >> 32) as u32]);
        self
    }

    pub fn pipeline(&mut self, p: Handle) -> &mut Self {
        self.op(P_PIPELINE).u64(p)
    }

    pub fn uniform_buffer(
        &mut self,
        index: u32,
        buffer: Handle,
        offset: u64,
        size: u64,
    ) -> &mut Self {
        self.op(P_UNIFORM_BUFFER);
        self.words.push(index);
        self.u64(buffer).u64(offset).u64(size)
    }

    pub fn uniform_texture(&mut self, index: u32, view: Handle, sampler: Handle) -> &mut Self {
        self.op(P_UNIFORM_TEXTURE);
        self.words.push(index);
        self.u64(view).u64(sampler)
    }

    pub fn push_constants(&mut self, buffer: Handle, offset: u64, size: u64) -> &mut Self {
        self.op(P_PUSH).u64(buffer).u64(offset).u64(size)
    }

    pub fn scissor(&mut self, x: i32, y: i32, w: i32, h: i32) -> &mut Self {
        self.op(P_SCISSOR);
        self.words.extend([x as u32, y as u32, w as u32, h as u32]);
        self
    }

    pub fn vertex(&mut self, slot: u32, buffer: Handle, offset: u64) -> &mut Self {
        self.op(P_VERTEX);
        self.words.push(slot);
        self.u64(buffer).u64(offset)
    }

    pub fn index(&mut self, buffer: Handle, int: bool) -> &mut Self {
        self.op(P_INDEX).u64(buffer);
        self.words.push(u32::from(int));
        self
    }

    pub fn draw(
        &mut self,
        vertices: u32,
        instances: u32,
        first: u32,
        first_instance: u32,
    ) -> &mut Self {
        self.op(P_DRAW);
        self.words
            .extend([vertices, instances, first, first_instance]);
        self
    }

    pub fn draw_indexed(
        &mut self,
        count: u32,
        instances: u32,
        first: u32,
        base: i32,
        first_instance: u32,
    ) -> &mut Self {
        self.op(P_DRAW_INDEXED);
        self.words
            .extend([count, instances, first, base as u32, first_instance]);
        self
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.words.iter().flat_map(|w| w.to_ne_bytes()).collect()
    }
}

struct Reader<'a> {
    words: &'a [u32],
    at: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> OpResult<u32> {
        let v = *self.words.get(self.at).ok_or_else(|| {
            OpError::Invalid(format!("pass stream truncated at word {}", self.at))
        })?;
        self.at += 1;
        Ok(v)
    }

    fn u64(&mut self) -> OpResult<u64> {
        Ok(u64::from(self.u32()?) | (u64::from(self.u32()?) << 32))
    }

    fn take(&mut self, n: usize) -> OpResult<&[u32]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.words.len())
            .ok_or_else(|| OpError::Invalid("pass stream array overruns the stream".into()))?;
        let s = &self.words[self.at..end];
        self.at = end;
        Ok(s)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PassReport {
    pub ops: u64,
    pub draws: u64,
    pub skipped: u64,
    pub first_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bound {
    None,
    User,
    Fan,
}

#[derive(Debug, Clone, Copy)]
enum Uniform {
    None,
    Buffer(Handle, u64, u64),
    Texture(Handle, Handle),
}

/// The recorded state of a pass that was cut, re-applied when it continues.
#[derive(Debug, Clone)]
pub struct PassState {
    pipeline: Handle,
    uniforms: [Uniform; MAX_UNIFORMS],
    push: Option<(Handle, u64, u64)>,
    vertex: [Option<(Handle, u64)>; MAX_VERTEX_SLOTS],
    index: Option<(Handle, wgpu::IndexFormat)>,
    scissor: Option<[i32; 4]>,
}

#[derive(Clone)]
struct Current {
    handle: Handle,
    unfilterable: u64,
    layout: Arc<Layout>,
}

pub fn encode_pass(
    backend: &Backend,
    state: &mut State,
    encoder: Handle,
    desc: &PassDesc,
    stream: &[u32],
) -> OpResult<PassReport> {
    let start = Instant::now();
    let gpu = backend.gpu;
    let mut fan_slot = state.fan.take();
    let State {
        buffers,
        textures,
        views,
        samplers,
        encoders,
        binds,
        feedback,
        ..
    } = state;
    let enc = encoders
        .get_mut(encoder)
        .ok_or(OpError::Handle(Kind::Encoder, encoder))?;
    let resumed = if desc.flags & PASS_RESUMES != 0 {
        enc.resume.take()
    } else {
        enc.resume = None;
        None
    };
    let world = backend
        .slots
        .world_targets(&gpu.device, backend.slots.current().0);

    // Virtual main targets resolve to mcr's scene (and scene or private depth) in world slots.
    #[allow(clippy::type_complexity)]
    let attachment = |h: Handle| -> OpResult<(
        wgpu::TextureView,
        wgpu::TextureFormat,
        u32,
        u32,
        bool,
        Option<wgpu::Texture>,
    )> {
        let v = views.get(h).ok_or(OpError::Handle(Kind::View, h))?;
        let t = textures
            .get(v.texture)
            .ok_or(OpError::Handle(Kind::Texture, v.texture))?;
        if let (true, Some(w)) = (t.virtual_main && v.base_mip == 0, world.as_ref()) {
            let view = if v.format.has_color_aspect() {
                if v.format != w.color_texture.format() {
                    return Err(OpError::Invalid(
                        "main color format differs from the scene".into(),
                    ));
                }
                w.color.clone()
            } else {
                if v.format != w.depth.texture().format() {
                    return Err(OpError::Invalid(
                        "main depth format differs from the scene".into(),
                    ));
                }
                w.depth.clone()
            };
            let texture = view.texture().clone();
            return Ok((view, v.format, w.size.0, w.size.1, true, Some(texture)));
        }
        let view = v
            .attachment
            .clone()
            .ok_or_else(|| OpError::Invalid("view cannot be a render attachment".into()))?;
        let texture = (v.base_mip == 0).then(|| t.texture.clone());
        Ok((view, v.format, v.width, v.height, t.virtual_main, texture))
    };
    let mut colors = Vec::with_capacity(desc.colors.len());
    let mut targets: [Option<wgpu::Texture>; MAX_COLOR_ATTACHMENTS + 1] = Default::default();
    let mut key = VariantKey {
        colors: [None; MAX_COLOR_ATTACHMENTS],
        color_count: desc.colors.len() as u8,
        depth: None,
        unfilterable: 0,
        flip: false,
        top_down: false,
    };
    let mut scene_color = false;
    let mut size: Option<(u32, u32)> = None;
    let mut virtual_targets: Vec<Handle> = Vec::new();
    let mut same_size = |w: u32, h: u32| -> OpResult<()> {
        match size {
            Some(s) if s != (w, h) => {
                Err(OpError::Invalid("pass attachments differ in size".into()))
            }
            _ => {
                size = Some((w, h));
                Ok(())
            }
        }
    };
    for (i, c) in desc.colors.iter().enumerate() {
        let Some(c) = c else {
            colors.push(None);
            continue;
        };
        let (view, format, w, h, virtual_main, texture) = attachment(c.view)?;
        targets[i] = texture.filter(|_| !virtual_main);
        if !format.has_color_aspect() {
            return Err(OpError::Invalid(
                "depth view used as a color attachment".into(),
            ));
        }
        same_size(w, h)?;
        key.colors[i] = Some(format);
        if virtual_main {
            virtual_targets.extend(views.get(c.view).map(|v| v.texture));
            scene_color |= world.is_some();
        }
        let clear = c.clear.filter(|_| !virtual_main && resumed.is_none());
        colors.push(Some((view, clear)));
    }
    let depth = match desc.depth {
        Some((h, clear)) => {
            let (view, format, w, ht, virtual_main, texture) = attachment(h)?;
            if !format.has_depth_aspect() {
                return Err(OpError::Invalid(
                    "color view used as a depth attachment".into(),
                ));
            }
            same_size(w, ht)?;
            key.depth = Some(format);
            let scene_depth = virtual_main && world.as_ref().is_some_and(|w| w.scene_depth);
            if virtual_main && world.is_some() {
                virtual_targets.extend(views.get(h).map(|v| v.texture));
            }
            targets[MAX_COLOR_ATTACHMENTS] = texture.filter(|_| !scene_depth);
            let clear = clear.filter(|_| !scene_depth && resumed.is_none());
            Some((view, clear, format.has_stencil_aspect()))
        }
        None => None,
    };
    let (width, height) =
        size.ok_or_else(|| OpError::Invalid("pass without attachments".into()))?;

    let mut depth = depth;
    let leftover = if enc.clears.is_empty() {
        Vec::new()
    } else {
        let sampled = sampled_textures(stream, views, textures, world.as_ref());
        take_clears(
            &mut enc.clears,
            resumed.is_some(),
            sampled.as_deref(),
            &targets,
            &mut colors,
            depth.as_mut().map(|(_, clear, _)| clear),
        )
    };

    let color_atts: Vec<Option<wgpu::RenderPassColorAttachment>> = colors
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|(view, clear)| wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match clear {
                            Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(c[0]),
                                g: f64::from(c[1]),
                                b: f64::from(c[2]),
                                a: f64::from(c[3]),
                            }),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })
        })
        .collect();
    let depth_att = depth.as_ref().map(|(view, clear, stencil)| {
        let ops = |clear: Option<f32>| wgpu::Operations {
            load: clear.map_or(wgpu::LoadOp::Load, |d| {
                wgpu::LoadOp::Clear(d.clamp(0.0, 1.0))
            }),
            store: wgpu::StoreOp::Store,
        };
        wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(ops(*clear)),
            stencil_ops: stencil.then(|| wgpu::Operations {
                load: if clear.is_some() {
                    wgpu::LoadOp::Clear(0)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
        }
    });

    let top_down = scene_color && world.as_ref().is_some_and(|w| w.gui);
    let clamp_rect =
        |x: i64, y: i64, w: i64, h: i64| target_rect([x, y, w, h], (width, height), top_down);
    let [ax, ay, aw, ah] = desc.area;
    let area = clamp_rect(i64::from(ax), i64::from(ay), i64::from(aw), i64::from(ah));

    let limits = &gpu.limits;
    let ubo_align = u64::from(limits.min_uniform_buffer_offset_alignment);
    let max_ubo = limits.max_uniform_buffer_binding_size;

    let mut report = PassReport::default();
    let fail = |report: &mut PassReport, e: OpError| {
        report.skipped += 1;
        if report.first_error.is_none() {
            report.first_error = Some(e.to_string());
        }
    };

    let cmd = enc.cmd(&gpu.device);
    if !leftover.is_empty() {
        super::backend::record_clears(cmd, leftover);
    }
    let mut sampled = Vec::new();
    if let Some(r) = &resumed {
        sampled.extend(r.uniforms.iter().filter_map(|u| match *u {
            Uniform::Texture(v, _) => Some(v),
            _ => None,
        }));
    }
    let subst = if virtual_targets.is_empty() && world.is_none() {
        FxHashMap::default()
    } else {
        feedback_copies(
            gpu,
            cmd,
            &Sources {
                textures,
                views,
                world: world.as_ref(),
                targets: &virtual_targets,
            },
            feedback,
            &mut sampled,
            stream,
        )
    };
    let mut pass = cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("java pass"),
        color_attachments: &color_atts,
        depth_stencil_attachment: depth_att,
        ..Default::default()
    });
    pass.set_scissor_rect(area.0, area.1, area.2, area.3);

    let mut r = Reader {
        words: stream,
        at: 0,
    };
    let mut pipeline_handle: Handle = 0;
    let mut pipeline: Option<Arc<Pipeline>> = None;
    let mut current: Option<Current> = None;
    let mut uniforms = [Uniform::None; MAX_UNIFORMS];
    let mut push: Option<(Handle, u64, u64)> = None;
    let mut vertex: [Option<(Handle, u64)>; MAX_VERTEX_SLOTS] = [None; MAX_VERTEX_SLOTS];
    let mut index: Option<(Handle, wgpu::IndexFormat)> = None;
    let mut scissor: Option<[i32; 4]> = None;
    let mut bound_index = Bound::None;
    let mut bound_group: Option<(BindKey, [u32; MAX_UNIFORMS + 1], usize)> = None;
    let mut debug_depth = 0u32;
    let flip_world = world.as_ref().is_some_and(|w| !w.gui);
    if let Some(r) = resumed {
        pipeline = backend.pipeline(r.pipeline);
        pipeline_handle = if pipeline.is_some() { r.pipeline } else { 0 };
        (uniforms, push, index, scissor) = (r.uniforms, r.push, r.index, r.scissor);
        for (slot, v) in r.vertex.iter().enumerate() {
            if let Some((h, off)) = *v
                && let Some(b) = buffers.get(h)
                && off <= b.buffer.size()
            {
                pass.set_vertex_buffer(slot as u32, b.buffer.slice(off..));
                vertex[slot] = Some((h, off));
            }
        }
        if let Some([x, y, w, h]) = scissor {
            let (x, y, w, h) = clamp_rect(i64::from(x), i64::from(y), i64::from(w), i64::from(h));
            pass.set_scissor_rect(x, y, w, h);
        }
    }

    // Resolves the pipeline variant and bind group for the next draw.
    macro_rules! prepare {
        () => {{
            let result: OpResult<Arc<Pipeline>> = (|| {
                let p = pipeline
                    .clone()
                    .ok_or_else(|| OpError::Invalid("draw without a pipeline".into()))?;
                let mut unfilterable = 0u64;
                let mut res = [Res::None; MAX_UNIFORMS + 1];
                let mut offsets = [0u32; MAX_UNIFORMS + 1];
                let mut n_offsets = 0;
                for (i, slot) in p.slots.iter().enumerate() {
                    match (*slot, uniforms[i]) {
                        (Slot::Unused, _) => {}
                        (Slot::Buffer, Uniform::Buffer(h, off, len)) => {
                            let len = len.max(p.block_sizes[i]);
                            let b = buffers.get(h).ok_or(OpError::Handle(Kind::Buffer, h))?;
                            if !b.buffer.usage().contains(wgpu::BufferUsages::UNIFORM)
                                || !off.is_multiple_of(ubo_align)
                                || len == 0
                                || len > max_ubo
                                || off.checked_add(len).is_none_or(|e| e > b.buffer.size())
                            {
                                return Err(OpError::Invalid(format!(
                                    "uniform {i} of {}: bad slice {off}+{len}",
                                    p.name
                                )));
                            }
                            res[i] = Res::Buffer(h, len);
                            offsets[n_offsets] = off as u32;
                            n_offsets += 1;
                        }
                        (Slot::Texture { sample, dimension }, Uniform::Texture(vh, sh)) => {
                            let v = views.get(vh).ok_or(OpError::Handle(Kind::View, vh))?;
                            textures
                                .get(v.texture)
                                .ok_or(OpError::Handle(Kind::Texture, v.texture))?;
                            samplers.get(sh).ok_or(OpError::Handle(Kind::Sampler, sh))?;
                            if v.dimension != dimension {
                                return Err(OpError::Invalid(format!(
                                    "uniform {i} of {} wants a {dimension:?} view",
                                    p.name
                                )));
                            }
                            let nearest = match (sample, v.sample) {
                                (TextureSample::Float, SampleClass::Filterable) => false,
                                (TextureSample::Float, SampleClass::Unfilterable) => {
                                    unfilterable |= 1 << i;
                                    true
                                }
                                (TextureSample::Uint, SampleClass::Uint)
                                | (TextureSample::Sint, SampleClass::Sint) => true,
                                _ => {
                                    return Err(OpError::Invalid(format!(
                                        "uniform {i} of {}: {:?} view for a {sample:?} binding",
                                        p.name, v.sample
                                    )));
                                }
                            };
                            res[i] = Res::Texture(vh, sh, nearest);
                        }
                        _ => {
                            return Err(OpError::Invalid(format!(
                                "uniform {i} of {} is missing or of the wrong type",
                                p.name
                            )));
                        }
                    }
                }
                if p.push_constants {
                    let (h, off, len) = push.ok_or_else(|| {
                        OpError::Invalid(format!("{} needs push constants", p.name))
                    })?;
                    let len = len.max(p.block_sizes[p.slots.len()]);
                    let b = buffers.get(h).ok_or(OpError::Handle(Kind::Buffer, h))?;
                    if !off.is_multiple_of(ubo_align)
                        || len == 0
                        || len > max_ubo
                        || off + len > b.buffer.size()
                    {
                        return Err(OpError::Invalid("bad push constant slice".into()));
                    }
                    res[MAX_UNIFORMS] = Res::Buffer(h, len);
                    offsets[n_offsets] = off as u32;
                    n_offsets += 1;
                }
                let same = current
                    .as_ref()
                    .is_some_and(|c| c.handle == pipeline_handle && c.unfilterable == unfilterable);
                if !same {
                    let mut k = key.clone();
                    k.unfilterable = unfilterable;
                    k.flip = flip_world && p.uses_projection;
                    k.top_down = top_down && p.uses_projection;
                    let (rp, layout) = p.variant(&gpu.device, &k)?;
                    pass.set_pipeline(&rp);
                    current = Some(Current {
                        handle: pipeline_handle,
                        unfilterable,
                        layout,
                    });
                    bound_group = None;
                }
                let layout = &current.as_ref().expect("set above").layout;
                let bind_key = BindKey {
                    layout: layout.id,
                    res,
                };
                let substituted = !subst.is_empty()
                    && res
                        .iter()
                        .any(|r| matches!(r, Res::Texture(v, _, _) if subst.contains_key(v)));
                if bound_group
                    .as_ref()
                    .is_some_and(|(k, o, n)| *k == bind_key && *o == offsets && *n == n_offsets)
                {
                    return Ok(p);
                }
                let cached = if substituted {
                    None
                } else {
                    binds.map.get(&bind_key)
                };
                let group = match cached {
                    Some(g) => {
                        backend.stats.bind_hits.fetch_add(1, Ordering::Relaxed);
                        g.clone()
                    }
                    None => {
                        backend.stats.bind_misses.fetch_add(1, Ordering::Relaxed);
                        let mut entries = Vec::with_capacity(p.slots.len() * 2 + 1);
                        for (i, r) in res.iter().enumerate() {
                            match *r {
                                Res::None => {}
                                Res::Buffer(h, len) => {
                                    let b = &buffers.get(h).expect("validated above").buffer;
                                    let binding = if i == MAX_UNIFORMS {
                                        p.push_binding()
                                    } else {
                                        spv::buffer_binding(i as u32)
                                    };
                                    entries.push(wgpu::BindGroupEntry {
                                        binding,
                                        resource: wgpu::BindingResource::Buffer(
                                            wgpu::BufferBinding {
                                                buffer: b,
                                                offset: 0,
                                                size: std::num::NonZeroU64::new(len),
                                            },
                                        ),
                                    });
                                }
                                Res::Texture(vh, sh, nearest) => {
                                    let v = views.get(vh).expect("validated above");
                                    let s = samplers.get(sh).expect("validated above");
                                    let view = subst.get(&vh).unwrap_or(&v.view);
                                    entries.push(wgpu::BindGroupEntry {
                                        binding: spv::texture_binding(i as u32),
                                        resource: wgpu::BindingResource::TextureView(view),
                                    });
                                    entries.push(wgpu::BindGroupEntry {
                                        binding: spv::sampler_binding(i as u32),
                                        resource: wgpu::BindingResource::Sampler(if nearest {
                                            &s.nearest
                                        } else {
                                            &s.sampler
                                        }),
                                    });
                                }
                            }
                        }
                        let g = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("java bind group"),
                            layout: &layout.bind_group,
                            entries: &entries,
                        });
                        if !substituted {
                            if binds.map.len() >= MAX_BIND_CACHE {
                                binds.map.clear();
                            }
                            binds.map.insert(bind_key.clone(), g.clone());
                        }
                        g
                    }
                };
                pass.set_bind_group(0, &group, &offsets[..n_offsets]);
                bound_group = Some((bind_key, offsets, n_offsets));
                Ok(p)
            })();
            result
        }};
    }

    // Vertex range checks keep wgpu from invalidating the whole encoder on a bad draw.
    let check_vertices = |p: &Pipeline,
                          vertex: &[Option<(Handle, u64)>; MAX_VERTEX_SLOTS],
                          first: u64,
                          count: u64,
                          first_instance: u64,
                          instances: u64,
                          indexed: bool|
     -> OpResult<()> {
        for (slot, layout) in p.vertex_layouts().iter().enumerate() {
            if layout.attributes_end == 0 {
                continue;
            }
            let (h, off) = vertex.get(slot).copied().flatten().ok_or_else(|| {
                OpError::Invalid(format!("vertex slot {slot} of {} is unbound", p.name))
            })?;
            let size = buffers
                .get(h)
                .ok_or(OpError::Handle(Kind::Buffer, h))?
                .buffer
                .size();
            let avail = size.saturating_sub(off);
            let (first, count) = match layout.step {
                wgpu::VertexStepMode::Instance => (first_instance, instances),
                wgpu::VertexStepMode::Vertex if !indexed => (first, count),
                wgpu::VertexStepMode::Vertex => continue,
            };
            if count > 0 && (first + count - 1) * layout.stride + layout.attributes_end > avail {
                return Err(OpError::Invalid(format!(
                    "draw reads past vertex buffer {slot} of {}",
                    p.name
                )));
            }
        }
        Ok(())
    };

    let index_size = |f: wgpu::IndexFormat| if f == wgpu::IndexFormat::Uint16 { 2 } else { 4 };

    macro_rules! bind_user_index {
        ($first:expr, $count:expr) => {{
            let result: OpResult<()> = (|| {
                let (h, f) = index
                    .ok_or_else(|| OpError::Invalid("indexed draw without index buffer".into()))?;
                let b = &buffers
                    .get(h)
                    .ok_or(OpError::Handle(Kind::Buffer, h))?
                    .buffer;
                if !b.usage().contains(wgpu::BufferUsages::INDEX)
                    || (u64::from($first) + u64::from($count)) * index_size(f) > b.size()
                {
                    return Err(OpError::Invalid(
                        "indexed draw reads past its index buffer".into(),
                    ));
                }
                if bound_index != Bound::User {
                    pass.set_index_buffer(b.slice(..), f);
                    bound_index = Bound::User;
                }
                Ok(())
            })();
            result
        }};
    }

    macro_rules! draw {
        ($vc:expr, $ic:expr, $fv:expr, $fi:expr) => {{
            let (vc, ic, fv, fi): (u32, u32, u32, u32) = ($vc, $ic, $fv, $fi);
            if vc != 0 && ic != 0 {
                let result: OpResult<()> = (|| {
                    let p = prepare!()?;
                    check_vertices(
                        &p,
                        &vertex,
                        u64::from(fv),
                        u64::from(vc),
                        u64::from(fi),
                        u64::from(ic),
                        false,
                    )?;
                    let end_instance = fi
                        .checked_add(ic)
                        .ok_or_else(|| OpError::Invalid("instance range overflows".into()))?;
                    if p.fan {
                        let fan = super::backend::ensure_fan(&mut fan_slot, &gpu.device, vc)
                            .ok_or_else(|| OpError::Invalid(format!("fan of {vc} vertices")))?;
                        if bound_index != Bound::Fan {
                            pass.set_index_buffer(fan.slice(..), wgpu::IndexFormat::Uint32);
                            bound_index = Bound::Fan;
                        }
                        let base = i32::try_from(fv)
                            .map_err(|_| OpError::Invalid("first vertex".into()))?;
                        pass.draw_indexed(0..(vc - 2) * 3, base, fi..end_instance);
                    } else {
                        let end = fv
                            .checked_add(vc)
                            .ok_or_else(|| OpError::Invalid("vertex range overflows".into()))?;
                        pass.draw(fv..end, fi..end_instance);
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => report.draws += 1,
                    Err(e) => fail(&mut report, e),
                }
            }
        }};
    }

    macro_rules! draw_indexed {
        ($count:expr, $ic:expr, $first:expr, $base:expr, $fi:expr) => {{
            let (count, ic, first, base, fi): (u32, u32, u32, i32, u32) =
                ($count, $ic, $first, $base, $fi);
            if count != 0 && ic != 0 {
                let result: OpResult<()> = (|| {
                    let p = prepare!()?;
                    if p.fan {
                        return Err(OpError::Invalid(
                            "indexed triangle fans are not supported".into(),
                        ));
                    }
                    check_vertices(&p, &vertex, 0, 0, u64::from(fi), u64::from(ic), true)?;
                    bind_user_index!(first, count)?;
                    let end_instance = fi
                        .checked_add(ic)
                        .ok_or_else(|| OpError::Invalid("instance range overflows".into()))?;
                    pass.draw_indexed(first..first + count, base, fi..end_instance);
                    Ok(())
                })();
                match result {
                    Ok(()) => report.draws += 1,
                    Err(e) => fail(&mut report, e),
                }
            }
        }};
    }

    let result: OpResult<()> = (|| {
        while r.at < r.words.len() {
            let op = r.u32()?;
            report.ops += 1;
            match op {
                P_PIPELINE => {
                    let h = r.u64()?;
                    let found = if pipeline.is_some() && h == pipeline_handle {
                        pipeline.clone()
                    } else {
                        backend.pipeline(h)
                    };
                    match found {
                        Some(p) => {
                            if p.slots.len() > MAX_UNIFORMS {
                                return Err(OpError::Invalid(
                                    "pipeline has too many uniforms".into(),
                                ));
                            }
                            pipeline = Some(p);
                            pipeline_handle = h;
                        }
                        None => {
                            pipeline = None;
                            pipeline_handle = 0;
                            fail(&mut report, OpError::Handle(Kind::Pipeline, h));
                        }
                    }
                    uniforms = [Uniform::None; MAX_UNIFORMS];
                    push = None;
                }
                P_UNIFORM_BUFFER => {
                    let (i, h, off, len) = (r.u32()? as usize, r.u64()?, r.u64()?, r.u64()?);
                    if let Some(u) = uniforms.get_mut(i) {
                        *u = Uniform::Buffer(h, off, len);
                    }
                }
                P_UNIFORM_TEXTURE => {
                    let (i, v, s) = (r.u32()? as usize, r.u64()?, r.u64()?);
                    if let Some(u) = uniforms.get_mut(i) {
                        *u = Uniform::Texture(v, s);
                    }
                }
                P_UNIFORM_NONE => {
                    let i = r.u32()? as usize;
                    if let Some(u) = uniforms.get_mut(i) {
                        *u = Uniform::None;
                    }
                }
                P_PUSH => push = Some((r.u64()?, r.u64()?, r.u64()?)),
                P_SCISSOR => {
                    let s = r.take(4)?;
                    let s = [s[0] as i32, s[1] as i32, s[2] as i32, s[3] as i32];
                    if scissor == Some(s) {
                        continue;
                    }
                    let (x, y, w, h) = clamp_rect(
                        i64::from(s[0]),
                        i64::from(s[1]),
                        i64::from(s[2]),
                        i64::from(s[3]),
                    );
                    pass.set_scissor_rect(x, y, w, h);
                    scissor = Some(s);
                }
                P_NO_SCISSOR => {
                    if scissor.is_some() {
                        pass.set_scissor_rect(area.0, area.1, area.2, area.3);
                        scissor = None;
                    }
                }
                P_VERTEX => {
                    let (slot, h, off) = (r.u32()? as usize, r.u64()?, r.u64()?);
                    if slot >= MAX_VERTEX_SLOTS || slot >= limits.max_vertex_buffers as usize {
                        fail(&mut report, OpError::Invalid(format!("vertex slot {slot}")));
                        continue;
                    }
                    if h == 0 {
                        vertex[slot] = None;
                        continue;
                    }
                    if vertex[slot] == Some((h, off)) {
                        continue;
                    }
                    match buffers.get(h) {
                        Some(b)
                            if off.is_multiple_of(4)
                                && off <= b.buffer.size()
                                && b.buffer.usage().contains(wgpu::BufferUsages::VERTEX) =>
                        {
                            pass.set_vertex_buffer(slot as u32, b.buffer.slice(off..));
                            vertex[slot] = Some((h, off));
                        }
                        _ => {
                            vertex[slot] = None;
                            fail(
                                &mut report,
                                OpError::Invalid(format!("vertex buffer {h:#x}+{off}")),
                            );
                        }
                    }
                }
                P_INDEX => {
                    let (h, t) = (r.u64()?, r.u32()?);
                    let f = if t == 0 {
                        wgpu::IndexFormat::Uint16
                    } else {
                        wgpu::IndexFormat::Uint32
                    };
                    if index != Some((h, f)) {
                        index = Some((h, f));
                        if bound_index == Bound::User {
                            bound_index = Bound::None;
                        }
                    }
                }
                P_DRAW => {
                    let a = r.take(4)?;
                    draw!(a[0], a[1], a[2], a[3]);
                }
                P_DRAW_INDEXED => {
                    let a = r.take(5)?;
                    draw_indexed!(a[0], a[1], a[2], a[3] as i32, a[4]);
                }
                P_MULTI_DRAW => {
                    let (count, ic, fi) = (r.u32()? as usize, r.u32()?, r.u32()?);
                    let list = r.take(
                        count
                            .checked_mul(2)
                            .ok_or_else(|| OpError::Invalid("count".into()))?,
                    )?;
                    for d in list.as_chunks::<2>().0 {
                        draw!(d[1], ic, d[0], fi);
                    }
                }
                P_MULTI_DRAW_INDEXED => {
                    let (count, ic, fi) = (r.u32()? as usize, r.u32()?, r.u32()?);
                    let list = r.take(
                        count
                            .checked_mul(3)
                            .ok_or_else(|| OpError::Invalid("count".into()))?,
                    )?;
                    for d in list.as_chunks::<3>().0 {
                        draw_indexed!(d[1], ic, d[0], d[2] as i32, fi);
                    }
                }
                P_DRAW_INDIRECT | P_DRAW_INDEXED_INDIRECT => {
                    let (h, off, count) = (r.u64()?, r.u64()?, r.u32()?);
                    let indexed = op == P_DRAW_INDEXED_INDIRECT;
                    let stride: u64 = if indexed { 20 } else { 16 };
                    let result: OpResult<()> = (|| {
                        let p = prepare!()?;
                        if p.fan {
                            return Err(OpError::Invalid(
                                "indirect triangle fans are not supported".into(),
                            ));
                        }
                        let b = buffers
                            .get(h)
                            .ok_or(OpError::Handle(Kind::Buffer, h))?
                            .buffer
                            .clone();
                        if !b.usage().contains(wgpu::BufferUsages::INDIRECT)
                            || !off.is_multiple_of(4)
                            || off
                                .checked_add(u64::from(count) * stride)
                                .is_none_or(|e| e > b.size())
                        {
                            return Err(OpError::Invalid("indirect buffer range".into()));
                        }
                        if indexed {
                            let (ih, _) = index.ok_or_else(|| {
                                OpError::Invalid("indexed draw without index buffer".into())
                            })?;
                            let ib = buffers.get(ih).ok_or(OpError::Handle(Kind::Buffer, ih))?;
                            if bound_index != Bound::User {
                                pass.set_index_buffer(
                                    ib.buffer.slice(..),
                                    index.expect("checked").1,
                                );
                                bound_index = Bound::User;
                            }
                        }
                        for k in 0..u64::from(count) {
                            if indexed {
                                pass.draw_indexed_indirect(&b, off + k * stride);
                            } else {
                                pass.draw_indirect(&b, off + k * stride);
                            }
                        }
                        Ok(())
                    })();
                    match result {
                        Ok(()) => report.draws += u64::from(count),
                        Err(e) => fail(&mut report, e),
                    }
                }
                P_DEBUG_PUSH => {
                    let len = r.u32()? as usize;
                    let words = r.take(len.div_ceil(4))?;
                    if debug_depth < MAX_DEBUG_DEPTH {
                        let bytes: Vec<u8> = words
                            .iter()
                            .flat_map(|w| w.to_ne_bytes())
                            .take(len.min(MAX_DEBUG_LABEL))
                            .collect();
                        pass.push_debug_group(&String::from_utf8_lossy(&bytes));
                        debug_depth += 1;
                    }
                }
                P_DEBUG_POP => {
                    if debug_depth > 0 {
                        pass.pop_debug_group();
                        debug_depth -= 1;
                    }
                }
                other => return Err(OpError::Invalid(format!("unknown pass op {other}"))),
            }
        }
        Ok(())
    })();
    for _ in 0..debug_depth {
        pass.pop_debug_group();
    }
    drop(pass);
    if desc.flags & PASS_CONTINUES != 0 {
        enc.resume = Some(PassState {
            pipeline: pipeline_handle,
            uniforms,
            push,
            vertex,
            index,
            scissor,
        });
    }
    state.fan = fan_slot;
    let stats = &backend.stats;
    stats.passes.fetch_add(1, Ordering::Relaxed);
    stats.ops.fetch_add(report.ops, Ordering::Relaxed);
    stats.draws.fetch_add(report.draws, Ordering::Relaxed);
    stats
        .skipped_draws
        .fetch_add(report.skipped, Ordering::Relaxed);
    stats
        .encode_ns
        .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    result.map(|()| report)
}

/// The textures the pass samples, resolved like its attachments, when clears are pending; `None`
/// when the stream cannot be read, so every pending clear must run first.
fn sampled_textures(
    stream: &[u32],
    views: &super::slab::Slab<super::resources::View>,
    textures: &super::slab::Slab<super::resources::Texture>,
    world: Option<&super::slots::WorldTargets>,
) -> Option<Vec<wgpu::Texture>> {
    let mut out = Vec::new();
    sampled_views(stream, |vh| {
        let Some((v, t)) = views
            .get(vh)
            .and_then(|v| textures.get(v.texture).map(|t| (v, t)))
        else {
            return;
        };
        out.push(match world.filter(|_| t.virtual_main) {
            Some(w) if v.format.has_color_aspect() => w.color_texture.clone(),
            Some(w) => w.depth.texture().clone(),
            None => t.texture.clone(),
        });
    })
    .ok()?;
    Some(out)
}

type ResolvedColor = Option<(wgpu::TextureView, Option<[f32; 4]>)>;

/// Moves pending clears of this pass's attachments into their load ops and returns the ones that
/// must run before the pass: of textures it samples, or of its attachments when it resumes. Others
/// stay pending.
fn take_clears(
    pending: &mut Vec<PendingClear>,
    resumed: bool,
    sampled: Option<&[wgpu::Texture]>,
    targets: &[Option<wgpu::Texture>; MAX_COLOR_ATTACHMENTS + 1],
    colors: &mut [ResolvedColor],
    mut depth: Option<&mut Option<f32>>,
) -> Vec<PendingClear> {
    let mut before = Vec::new();
    let mut i = 0;
    while i < pending.len() {
        let c = &pending[i];
        let is = |t: &Option<wgpu::Texture>| t.as_ref() == Some(&c.texture);
        if sampled.is_none_or(|s| s.contains(&c.texture)) {
            before.push(pending.remove(i));
            continue;
        }
        // A clear the pass itself asks for comes later and wins.
        let target: Option<&mut Option<_>> = match c.value {
            ClearValue::Color(_) => colors
                .iter_mut()
                .zip(targets)
                .find(|(slot, t)| slot.is_some() && is(t))
                .and_then(|(slot, _)| slot.as_mut())
                .map(|(_, clear)| clear),
            ClearValue::Depth(..) => None,
        };
        let depth_target = match c.value {
            ClearValue::Depth(..) if is(&targets[MAX_COLOR_ATTACHMENTS]) => depth.as_deref_mut(),
            _ => None,
        };
        match (c.value, target, depth_target) {
            (_, None, None) => i += 1,
            _ if resumed => before.push(pending.remove(i)),
            (ClearValue::Color(rgba), Some(clear), _) => {
                clear.get_or_insert(rgba);
                pending.remove(i);
            }
            (ClearValue::Depth(d, _), _, Some(clear)) => {
                clear.get_or_insert(d);
                pending.remove(i);
            }
            _ => i += 1,
        }
    }
    before
}

/// A Java scissor or render area clamped to the target; Java counts rows from the bottom, so a
/// top-down target gets the rectangle mirrored.
pub fn target_rect(
    [x, y, w, h]: [i64; 4],
    (width, height): (u32, u32),
    top_down: bool,
) -> (u32, u32, u32, u32) {
    let (w, h) = (w.max(0), h.max(0));
    let y = if top_down {
        i64::from(height) - y - h
    } else {
        y
    };
    let x0 = x.clamp(0, i64::from(width));
    let y0 = y.clamp(0, i64::from(height));
    let x1 = (x + w).clamp(x0, i64::from(width));
    let y1 = (y + h).clamp(y0, i64::from(height));
    (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
}

struct Sources<'a> {
    textures: &'a super::slab::Slab<super::resources::Texture>,
    views: &'a super::slab::Slab<super::resources::View>,
    world: Option<&'a super::slots::WorldTargets>,
    /// Textures the pass renders into that it may also sample.
    targets: &'a [Handle],
}

/// Substitute views for what the pass samples: in world slots a virtual main target reads mcr's
/// scene, and a texture the pass also renders into reads a copy taken before the pass (wgpu
/// forbids a texture as both attachment and binding).
fn feedback_copies(
    gpu: &super::context::Gpu,
    cmd: &mut wgpu::CommandEncoder,
    src: &Sources<'_>,
    scratch: &mut Option<wgpu::Texture>,
    sampled: &mut Vec<Handle>,
    stream: &[u32],
) -> FxHashMap<Handle, wgpu::TextureView> {
    if sampled_views(stream, |v| sampled.push(v)).is_err() {
        return FxHashMap::default();
    }
    let mut subst = FxHashMap::default();
    let mut copied: Option<Handle> = None;
    for &vh in sampled.iter() {
        let Some(v) = src.views.get(vh) else { continue };
        if subst.contains_key(&vh) {
            continue;
        }
        let Some(t) = src.textures.get(v.texture) else {
            continue;
        };
        let rendered = src.targets.contains(&v.texture);
        let world = src.world.filter(|_| t.virtual_main);
        if let Some(w) = world
            && !rendered
        {
            let view = if v.format.has_color_aspect() {
                w.color.clone()
            } else {
                w.depth.clone()
            };
            subst.insert(vh, view);
            continue;
        }
        if !rendered || copied.is_some_and(|c| c != v.texture) {
            continue;
        }
        let source = match world {
            Some(w) if v.format.has_color_aspect() => w.color_texture.clone(),
            Some(w) => w.depth.texture().clone(),
            None => t.texture.clone(),
        };
        if !source.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            continue;
        }
        let (w, h, mips) = (source.width(), source.height(), source.mip_level_count());
        let fits = scratch.as_ref().is_some_and(|s| {
            (s.width(), s.height(), s.format(), s.mip_level_count())
                == (w, h, source.format(), mips)
        });
        if !fits {
            *scratch = Some(gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("java feedback copy"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: source.format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
        let copy = scratch.as_ref().expect("created above");
        if copied.replace(v.texture).is_none() {
            cmd.copy_texture_to_texture(
                source.as_image_copy(),
                copy.as_image_copy(),
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        subst.insert(
            vh,
            copy.create_view(&wgpu::TextureViewDescriptor {
                label: Some("java feedback view"),
                base_mip_level: v.base_mip,
                mip_level_count: Some(1),
                ..Default::default()
            }),
        );
    }
    subst
}

/// Calls `f` with every texture view the stream binds.
pub fn sampled_views(stream: &[u32], mut f: impl FnMut(Handle)) -> OpResult<()> {
    let mut r = Reader {
        words: stream,
        at: 0,
    };
    let count = |n: u32, per: usize| {
        (n as usize)
            .checked_mul(per)
            .ok_or_else(|| OpError::Invalid("count".into()))
    };
    while r.at < r.words.len() {
        match r.u32()? {
            P_UNIFORM_TEXTURE => {
                r.u32()?;
                f(r.u64()?);
                r.u64()?;
            }
            P_PIPELINE => {
                r.take(2)?;
            }
            P_UNIFORM_BUFFER => {
                r.take(7)?;
            }
            P_UNIFORM_NONE => {
                r.take(1)?;
            }
            P_PUSH => {
                r.take(6)?;
            }
            P_SCISSOR | P_DRAW => {
                r.take(4)?;
            }
            P_NO_SCISSOR | P_DEBUG_POP => {}
            P_VERTEX | P_DRAW_INDEXED | P_DRAW_INDIRECT | P_DRAW_INDEXED_INDIRECT => {
                r.take(5)?;
            }
            P_INDEX => {
                r.take(3)?;
            }
            P_MULTI_DRAW => {
                let n = r.u32()?;
                r.take(2)?;
                r.take(count(n, 2)?)?;
            }
            P_MULTI_DRAW_INDEXED => {
                let n = r.u32()?;
                r.take(2)?;
                r.take(count(n, 3)?)?;
            }
            P_DEBUG_PUSH => {
                let len = r.u32()? as usize;
                r.take(len.div_ceil(4))?;
            }
            other => return Err(OpError::Invalid(format!("unknown pass op {other}"))),
        }
    }
    Ok(())
}
