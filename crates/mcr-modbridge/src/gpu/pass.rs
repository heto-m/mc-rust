//! Render pass replay: Java's off-heap command stream becomes one `wgpu::RenderPass`.
//!
//! The stream is native-endian `u32` words: `[op][payload]`, 64-bit values as two words (low first).
//! The descriptor is a list of `u64` slots (see [`PassDesc::parse`]).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use rustc_hash::FxHashMap;

use super::backend::{Backend, OpError, OpResult, State};
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
}

impl PassDesc {
    /// `[count, count × (view, has_clear, r, g, b, a), depth_view, depth_has_clear, depth, x, y, w, h]`,
    /// floats as `f32` bits.
    pub fn parse(slots: &[u64]) -> OpResult<Self> {
        let bad = || OpError::Invalid("malformed pass descriptor".into());
        let count = *slots.first().ok_or_else(bad)? as usize;
        if count > MAX_COLOR_ATTACHMENTS || slots.len() != 1 + count * 6 + 7 {
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

#[derive(Clone, Copy)]
enum Uniform {
    None,
    Buffer(Handle, u64, u64),
    Texture(Handle, Handle),
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
        ..
    } = state;
    let enc = encoders
        .get_mut(encoder)
        .ok_or(OpError::Handle(Kind::Encoder, encoder))?;

    let attachment = |h: Handle| -> OpResult<(wgpu::TextureView, wgpu::TextureFormat, u32, u32)> {
        let v = views.get(h).ok_or(OpError::Handle(Kind::View, h))?;
        textures
            .get(v.texture)
            .ok_or(OpError::Handle(Kind::Texture, v.texture))?;
        let view = v
            .attachment
            .clone()
            .ok_or_else(|| OpError::Invalid("view cannot be a render attachment".into()))?;
        Ok((view, v.format, v.width, v.height))
    };
    let mut colors = Vec::with_capacity(desc.colors.len());
    let mut key = VariantKey {
        colors: [None; MAX_COLOR_ATTACHMENTS],
        color_count: desc.colors.len() as u8,
        depth: None,
        unfilterable: 0,
    };
    let mut size: Option<(u32, u32)> = None;
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
        let (view, format, w, h) = attachment(c.view)?;
        if !format.has_color_aspect() {
            return Err(OpError::Invalid(
                "depth view used as a color attachment".into(),
            ));
        }
        same_size(w, h)?;
        key.colors[i] = Some(format);
        colors.push(Some((view, c.clear)));
    }
    let depth = match desc.depth {
        Some((h, clear)) => {
            let (view, format, w, ht) = attachment(h)?;
            if !format.has_depth_aspect() {
                return Err(OpError::Invalid(
                    "color view used as a depth attachment".into(),
                ));
            }
            same_size(w, ht)?;
            key.depth = Some(format);
            Some((view, clear, format.has_stencil_aspect()))
        }
        None => None,
    };
    let (width, height) =
        size.ok_or_else(|| OpError::Invalid("pass without attachments".into()))?;

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

    let clamp_rect = |x: i64, y: i64, w: i64, h: i64| -> (u32, u32, u32, u32) {
        let x0 = x.clamp(0, i64::from(width));
        let y0 = y.clamp(0, i64::from(height));
        let x1 = (x + w.max(0)).clamp(x0, i64::from(width));
        let y1 = (y + h.max(0)).clamp(y0, i64::from(height));
        (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
    };
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
    let mut bound_index = Bound::None;
    let mut bound_group: Option<(BindKey, [u32; MAX_UNIFORMS + 1], usize)> = None;
    let mut debug_depth = 0u32;

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
                if bound_group
                    .as_ref()
                    .is_some_and(|(k, o, n)| *k == bind_key && *o == offsets && *n == n_offsets)
                {
                    return Ok(p);
                }
                let group = match binds.map.get(&bind_key) {
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
                                    entries.push(wgpu::BindGroupEntry {
                                        binding: spv::texture_binding(i as u32),
                                        resource: wgpu::BindingResource::TextureView(&v.view),
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
                        if binds.map.len() >= MAX_BIND_CACHE {
                            binds.map.clear();
                        }
                        binds.map.insert(bind_key.clone(), g.clone());
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
                    match backend.pipeline(h) {
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
                    let (x, y, w, h) = clamp_rect(
                        i64::from(s[0] as i32),
                        i64::from(s[1] as i32),
                        i64::from(s[2] as i32),
                        i64::from(s[3] as i32),
                    );
                    pass.set_scissor_rect(x, y, w, h);
                }
                P_NO_SCISSOR => pass.set_scissor_rect(area.0, area.1, area.2, area.3),
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
                    index = Some((h, f));
                    bound_index = Bound::None;
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
