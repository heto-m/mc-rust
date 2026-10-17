//! Process-wide backend state behind Java's handles, and every device and encoder operation.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use rustc_hash::FxHashMap;

use super::context::{self, Gpu, GpuError};
use super::info::PipelineInfo;
use super::pass::BindCache;
use super::pipeline::{Pipeline, PipelineError};
use super::resources::{self, Buffer, Sampler, Texture, View};
use super::slab::{Handle, Kind, Slab};
use super::slots::{
    MAP_FAILED, MAP_OK, MAP_PENDING, Slots, Submission, SubmitTrack, WorldTargets, pooled_copy,
};

/// Largest single upload or readback Java may request.
pub const MAX_TRANSFER: u64 = 512 << 20;
pub const MAX_BUFFER: u64 = 2 << 30;
const RING_BLOCK: usize = 4 << 20;
const KEPT_RING_BLOCKS: usize = 2;
const FRAMES_IN_FLIGHT: u64 = 2;
const SUBMIT_WAIT: Duration = Duration::from_secs(5);
const MAX_PENDING_READBACKS: usize = 256;
const MAX_PENDING_CLEARS: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("{0}")]
    Gpu(#[from] GpuError),
    #[error("invalid {0:?} handle {1:#x}")]
    Handle(Kind, Handle),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
    #[error(transparent)]
    Resource(#[from] resources::ResourceError),
    #[error("wgpu: {0}")]
    Wgpu(String),
    #[error("the handle table is full")]
    Full,
}

pub type OpResult<T> = Result<T, OpError>;

fn invalid(msg: impl Into<String>) -> OpError {
    OpError::Invalid(msg.into())
}

#[derive(Debug, Default)]
pub struct Stats {
    pub passes: AtomicU64,
    pub ops: AtomicU64,
    pub draws: AtomicU64,
    pub skipped_draws: AtomicU64,
    pub bind_hits: AtomicU64,
    pub bind_misses: AtomicU64,
    pub encode_ns: AtomicU64,
    pub submits: AtomicU64,
    pub uploaded: AtomicU64,
    pub errors: AtomicU64,
    /// Time and count of Java's GPU downcalls, while [`PROFILE`] is on.
    pub ffi_ns: AtomicU64,
    pub ffi_calls: AtomicU64,
    /// Time and count per [`cost`] bucket, while [`PROFILE`] is on.
    pub cost_ns: [AtomicU64; cost::COUNT],
    pub cost_calls: [AtomicU64; cost::COUNT],
}

/// Where the Rust time of Java's GPU work goes: downcalls by kind on Java's thread, then the
/// render thread's replay of the queued work.
pub mod cost {
    /// Every `gpu_device` call.
    pub const DEVICE: usize = 0;
    // 1..=15 are the encoder ops (`E_*`).
    pub const PASS: usize = 16;
    /// `CommandEncoder::finish` inside a split or submit.
    pub const FINISH: usize = 17;
    /// Copying dirty transient ranges and staged bytes out for a submit.
    pub const STAGE: usize = 18;
    /// Render thread: `queue.write_buffer` of queued work.
    pub const QUEUE_WRITE: usize = 19;
    /// Render thread: `queue.submit` and completion callbacks of queued work.
    pub const QUEUE_SUBMIT: usize = 20;
    /// Render thread: error scopes and queue bookkeeping around queued work.
    pub const QUEUE_OTHER: usize = 21;
    pub const COUNT: usize = 22;

    pub const NAMES: [&str; COUNT] = [
        "device",
        "clear",
        "region-clear",
        "write",
        "copy",
        "write-tex",
        "buf-to-tex",
        "readback",
        "tex-copy",
        "flush",
        "blit",
        "dirty",
        "submit",
        "poll",
        "wait",
        "split",
        "pass",
        "finish",
        "stage",
        "q-write",
        "q-submit",
        "q-other",
    ];
}

/// Adds the time since `start` to `bucket`; `start` is `None` while profiling is off.
pub fn add_cost(bucket: usize, start: Option<std::time::Instant>) {
    let (Some(start), Some(b)) = (start, BACKEND.get()) else {
        return;
    };
    if bucket < cost::COUNT {
        let ns = start.elapsed().as_nanos() as u64;
        b.stats.cost_ns[bucket].fetch_add(ns, Ordering::Relaxed);
        b.stats.cost_calls[bucket].fetch_add(1, Ordering::Relaxed);
    }
}

/// A start time for [`add_cost`] while profiling.
pub fn cost_start() -> Option<std::time::Instant> {
    PROFILE
        .load(Ordering::Relaxed)
        .then(std::time::Instant::now)
}

/// Times every GPU downcall into [`Stats::ffi_ns`]; off unless the frame profile is.
pub static PROFILE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub passes: u64,
    pub ops: u64,
    pub draws: u64,
    pub skipped_draws: u64,
    pub bind_hits: u64,
    pub bind_misses: u64,
    pub encode_ns: u64,
    pub submits: u64,
    pub uploaded: u64,
    pub errors: u64,
    pub ffi_ns: u64,
    pub ffi_calls: u64,
    pub cost_ns: [u64; cost::COUNT],
    pub cost_calls: [u64; cost::COUNT],
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        let l = |a: &AtomicU64| a.load(Ordering::Relaxed);
        StatsSnapshot {
            passes: l(&self.passes),
            ops: l(&self.ops),
            draws: l(&self.draws),
            skipped_draws: l(&self.skipped_draws),
            bind_hits: l(&self.bind_hits),
            bind_misses: l(&self.bind_misses),
            encode_ns: l(&self.encode_ns),
            submits: l(&self.submits),
            uploaded: l(&self.uploaded),
            errors: l(&self.errors),
            ffi_ns: l(&self.ffi_ns),
            ffi_calls: l(&self.ffi_calls),
            cost_ns: self.cost_ns.each_ref().map(l),
            cost_calls: self.cost_calls.each_ref().map(l),
        }
    }
}

/// An RGBA8 image, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Default)]
struct Capture {
    requested: bool,
    image: Option<Image>,
}

static CAPTURE: Mutex<Capture> = Mutex::new(Capture {
    requested: false,
    image: None,
});

/// The next offscreen `present` stores its image for [`take_capture`].
pub fn capture_next_present() {
    let mut c = CAPTURE.lock().unwrap_or_else(PoisonError::into_inner);
    c.requested = true;
    c.image = None;
}

pub fn take_capture() -> Option<Image> {
    CAPTURE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .image
        .take()
}

/// Host-shadowed upload blocks: bytes are staged here, copied in-stream, and written at submit.
#[derive(Debug)]
struct RingBlock {
    buffer: wgpu::Buffer,
    data: Vec<u8>,
    used: usize,
    /// Bytes already handed to the queue in this submit.
    flushed: usize,
    used_last: bool,
}

#[derive(Debug, Default)]
pub struct UploadRing {
    blocks: Vec<RingBlock>,
    current: usize,
}

/// An idle block stays only if it is one of the first few and of the standard size.
fn keep_ring_block(position: usize, len: usize, used_last: bool) -> bool {
    used_last || (position <= KEPT_RING_BLOCKS && len <= RING_BLOCK)
}

pub fn align(n: u64, a: u64) -> u64 {
    n.div_ceil(a) * a
}

impl UploadRing {
    /// A slice of staging memory that the GPU sees at `offset` of the returned buffer.
    fn alloc(
        &mut self,
        device: &wgpu::Device,
        len: usize,
        alignment: usize,
    ) -> (wgpu::Buffer, u64, &mut [u8]) {
        let fits =
            |b: &RingBlock| align(b.used as u64, alignment as u64) as usize + len <= b.data.len();
        let index = match (self.current..self.blocks.len()).find(|&i| fits(&self.blocks[i])) {
            Some(i) => i,
            None => {
                let size = align(len.max(RING_BLOCK) as u64, 4) as usize;
                self.blocks.push(RingBlock {
                    buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("mcr-modbridge upload"),
                        size: size as u64,
                        usage: wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST
                            | wgpu::BufferUsages::VERTEX
                            | wgpu::BufferUsages::UNIFORM,
                        mapped_at_creation: false,
                    }),
                    data: vec![0; size],
                    used: 0,
                    flushed: 0,
                    used_last: false,
                });
                self.blocks.len() - 1
            }
        };
        self.current = index;
        let b = &mut self.blocks[index];
        let offset = align(b.used as u64, alignment as u64) as usize;
        b.used = offset + len;
        (
            b.buffer.clone(),
            offset as u64,
            &mut b.data[offset..offset + len],
        )
    }

    /// Writes the bytes staged since the last upload now, or hands copies to `deferred` to write with
    /// the submit. Blocks are reused only from the next submit on (`end_submit`), so splits of one
    /// submit never overwrite each other's staging.
    fn upload(
        &mut self,
        queue: &wgpu::Queue,
        mut deferred: Option<&mut Vec<(wgpu::Buffer, u64, Vec<u8>)>>,
        end_submit: bool,
    ) -> u64 {
        let mut bytes = 0;
        for b in &mut self.blocks {
            let end = align(b.used as u64, 4) as usize;
            if end > b.flushed {
                let range = b.flushed..end;
                match deferred.as_deref_mut() {
                    Some(out) => out.push((
                        b.buffer.clone(),
                        b.flushed as u64,
                        pooled_copy(&b.data[range]),
                    )),
                    None => queue.write_buffer(&b.buffer, b.flushed as u64, &b.data[range]),
                }
                bytes += (end - b.flushed) as u64;
                b.flushed = end;
            }
        }
        if end_submit {
            for b in &mut self.blocks {
                b.used_last = b.used > 0;
                b.used = 0;
                b.flushed = 0;
            }
            let mut i = 0;
            self.blocks.retain(|b| {
                i += 1;
                keep_ring_block(i, b.data.len(), b.used_last)
            });
            self.current = 0;
        }
        bytes
    }
}

#[derive(Debug)]
enum ReadbackTarget {
    /// Rows land in the destination's host shadow when the map completes.
    Shadow {
        staging: wgpu::Buffer,
        mapped: Arc<AtomicU8>,
        dest: Handle,
        dest_offset: u64,
        row_bytes: u32,
        padded_row: u32,
        rows: u32,
    },
    /// Copied on the GPU; done when its submission completes.
    Gpu,
}

#[derive(Debug)]
struct Readback {
    id: u64,
    submit: u64,
    target: ReadbackTarget,
}

#[derive(Debug)]
pub struct Encoder {
    pub cmd: Option<wgpu::CommandEncoder>,
    pub ring: UploadRing,
    recorded: Vec<Readback>,
    pending: Vec<Readback>,
    /// Submits so far; the next submit gets `submits + 1`.
    pub submits: u64,
    track: Arc<SubmitTrack>,
    /// State of a pass cut at a slot boundary, for its continuation.
    pub resume: Option<super::pass::PassState>,
    /// Full clears not recorded yet: the next pass that renders to them clears as it loads.
    pub clears: Vec<PendingClear>,
}

/// A full clear of mip 0 of a single-layer texture. It is recorded only once something uses the
/// texture, so a clear nobody observes before the next one costs nothing.
#[derive(Debug)]
pub struct PendingClear {
    pub texture: wgpu::Texture,
    /// The view to clear through; mip 0, layer 0 of `texture` when `None`.
    pub view: Option<wgpu::TextureView>,
    pub value: ClearValue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearValue {
    Color([f32; 4]),
    /// Depth, and whether the format has a stencil aspect (cleared to 0).
    Depth(f32, bool),
}

impl ClearValue {
    pub fn is_color(self) -> bool {
        matches!(self, Self::Color(_))
    }
}

/// Records `clears` as clear passes, a color clear and a depth clear of the same size sharing one.
pub fn record_clears(
    cmd: &mut wgpu::CommandEncoder,
    clears: impl IntoIterator<Item = PendingClear>,
) {
    let view = |c: &PendingClear| {
        c.view.clone().unwrap_or_else(|| {
            c.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("java clear"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                mip_level_count: Some(1),
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
    };
    let mut clears = clears.into_iter().peekable();
    while let Some(c) = clears.next() {
        let (color, depth) = match c.value {
            ClearValue::Color(rgba) => {
                let depth = clears.next_if(|d| {
                    matches!(d.value, ClearValue::Depth(..)) && d.texture.size() == c.texture.size()
                });
                (Some((view(&c), rgba)), depth.map(|d| (view(&d), d.value)))
            }
            ClearValue::Depth(..) => (None, Some((view(&c), c.value))),
        };
        let color_att = color
            .as_ref()
            .map(|(v, c)| wgpu::RenderPassColorAttachment {
                view: v,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(c[0]),
                        g: f64::from(c[1]),
                        b: f64::from(c[2]),
                        a: f64::from(c[3]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            });
        let depth_att = depth.as_ref().map(|(v, value)| {
            let (d, stencil) = match *value {
                ClearValue::Depth(d, stencil) => (d, stencil),
                ClearValue::Color(_) => (0.0, false),
            };
            wgpu::RenderPassDepthStencilAttachment {
                view: v,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(d.clamp(0.0, 1.0)),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: stencil.then_some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }
        });
        drop(cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("java clear"),
            color_attachments: &[color_att],
            depth_stencil_attachment: depth_att,
            ..Default::default()
        }));
    }
}

impl Encoder {
    fn new() -> Self {
        Self {
            resume: None,
            clears: Vec::new(),
            cmd: None,
            ring: UploadRing::default(),
            recorded: Vec::new(),
            pending: Vec::new(),
            submits: 0,
            track: Arc::default(),
        }
    }

    /// Holds back a full clear; an earlier pending clear of the same aspect is overwritten by it.
    fn defer_clear(&mut self, device: &wgpu::Device, clear: PendingClear) {
        self.clears
            .retain(|c| c.texture != clear.texture || c.value.is_color() != clear.value.is_color());
        self.clears.push(clear);
        if self.clears.len() > MAX_PENDING_CLEARS {
            let clears = std::mem::take(&mut self.clears);
            record_clears(self.cmd(device), clears);
        }
    }

    pub fn cmd(&mut self, device: &wgpu::Device) -> &mut wgpu::CommandEncoder {
        self.cmd.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mcr-modbridge java"),
            })
        })
    }
}

#[derive(Debug, Default)]
pub struct Surface {
    texture: Option<wgpu::Texture>,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClearKey {
    color: Option<wgpu::TextureFormat>,
    depth: Option<wgpu::TextureFormat>,
}

#[derive(Debug)]
pub struct State {
    pub buffers: Slab<Buffer>,
    pub textures: Slab<Texture>,
    pub views: Slab<View>,
    pub samplers: Slab<Sampler>,
    pub encoders: Slab<Encoder>,
    pub surfaces: Slab<Surface>,
    dirty_blocks: Vec<Handle>,
    destroyed: Vec<Handle>,
    pub binds: BindCache,
    pub fan: Option<(wgpu::Buffer, u32)>,
    clears: FxHashMap<ClearKey, wgpu::RenderPipeline>,
    clear_shader: Option<wgpu::ShaderModule>,
    /// Copy of a main target a pass samples while rendering into it.
    pub feedback: Option<wgpu::Texture>,
    flipper: Option<super::flip::Flipper>,
}

#[derive(Debug)]
pub struct Backend {
    pub gpu: &'static Gpu,
    state: Mutex<State>,
    pipelines: Mutex<Slab<Arc<Pipeline>>>,
    pub stats: Stats,
    pub slots: Slots,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();
static INIT: Mutex<()> = Mutex::new(());

pub fn backend() -> OpResult<&'static Backend> {
    if let Some(b) = BACKEND.get() {
        return Ok(b);
    }
    let _guard = INIT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(b) = BACKEND.get() {
        return Ok(b);
    }
    let gpu = context::gpu()?;
    Ok(BACKEND.get_or_init(|| Backend {
        gpu,
        state: Mutex::new(State {
            buffers: Slab::new(Kind::Buffer),
            textures: Slab::new(Kind::Texture),
            views: Slab::new(Kind::View),
            samplers: Slab::new(Kind::Sampler),
            encoders: Slab::new(Kind::Encoder),
            surfaces: Slab::new(Kind::Surface),
            dirty_blocks: Vec::new(),
            destroyed: Vec::new(),
            binds: BindCache::default(),
            fan: None,
            clears: FxHashMap::default(),
            clear_shader: None,
            feedback: None,
            flipper: None,
        }),
        pipelines: Mutex::new(Slab::new(Kind::Pipeline)),
        stats: Stats::default(),
        slots: Slots::default(),
    }))
}

fn insert<T>(slab: &mut Slab<T>, value: T) -> OpResult<Handle> {
    slab.insert(value).ok_or(OpError::Full)
}

/// Rows and offsets of a texture region copy in wgpu terms.
fn region(tex: &Texture, mip: u32, layer: u32, x: u32, y: u32, w: u32, h: u32) -> OpResult<()> {
    if mip >= tex.mips || layer >= tex.layers {
        return Err(invalid(format!("mip {mip} / layer {layer} out of range")));
    }
    let (mw, mh) = tex.mip_size(mip);
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|e| e > mw)
        || y.checked_add(h).is_none_or(|e| e > mh)
    {
        return Err(invalid(format!(
            "region {x},{y} {w}x{h} outside mip {mip} ({mw}x{mh})"
        )));
    }
    Ok(())
}

impl Backend {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn pipeline(&self, handle: Handle) -> Option<Arc<Pipeline>> {
        self.pipelines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(handle)
            .cloned()
    }

    pub fn create_buffer(&self, usage: u32, size: u64, init: Option<&[u8]>) -> OpResult<Handle> {
        let max = self.gpu.limits.max_buffer_size.min(MAX_BUFFER);
        if size == 0 || size > max {
            return Err(invalid(format!("buffer size {size} (max {max})")));
        }
        let init = init.map(|d| &d[..d.len().min(size as usize)]);
        let buffer = Buffer::new(&self.gpu.device, usage, size, init);
        insert(&mut self.lock().buffers, buffer)
    }

    pub fn create_transient_block(&self, size: u64) -> OpResult<Handle> {
        let max = self.gpu.limits.max_buffer_size.min(MAX_TRANSFER);
        if size == 0 || size > max {
            return Err(invalid(format!("transient block size {size}")));
        }
        let buffer = Buffer::transient(&self.gpu.device, size);
        insert(&mut self.lock().buffers, buffer)
    }

    /// The host shadow of a mappable buffer or transient block; 0 if it has none.
    pub fn buffer_host(&self, handle: Handle) -> OpResult<(i64, u64)> {
        let state = self.lock();
        let b = state
            .buffers
            .get(handle)
            .ok_or(OpError::Handle(Kind::Buffer, handle))?;
        Ok(b.shadow
            .as_ref()
            .map_or((0, 0), |s| (s.addr(), s.len() as u64)))
    }

    pub fn create_texture(
        &self,
        usage: u32,
        format: u32,
        width: u32,
        height: u32,
        layers: u32,
        mips: u32,
    ) -> OpResult<Handle> {
        let tex = resources::create_texture(
            &self.gpu.device,
            self.gpu.features,
            usage,
            format,
            width,
            height,
            layers,
            mips,
        )?;
        let main_color = tex.virtual_main && tex.format.has_color_aspect();
        let texture = tex.texture.clone();
        let handle = insert(&mut self.lock().textures, tex)?;
        if main_color {
            self.slots.set_main_color(handle, texture);
        }
        Ok(handle)
    }

    pub fn create_view(&self, texture: Handle, base_mip: u32, mips: u32) -> OpResult<Handle> {
        let mut state = self.lock();
        let tex = state
            .textures
            .get(texture)
            .ok_or(OpError::Handle(Kind::Texture, texture))?;
        let view = resources::create_view(tex, texture, base_mip, mips, self.gpu.features)
            .ok_or_else(|| invalid(format!("view mips {base_mip}+{mips} of {}", tex.mips)))?;
        insert(&mut state.views, view)
    }

    pub fn create_sampler(
        &self,
        address_u: u32,
        address_v: u32,
        min: u32,
        mag: u32,
        anisotropy: u32,
        max_lod: Option<f64>,
    ) -> OpResult<Handle> {
        let s = resources::create_sampler(
            &self.gpu.device,
            address_u,
            address_v,
            min,
            mag,
            anisotropy,
            max_lod,
        );
        insert(&mut self.lock().samplers, s)
    }

    pub fn compile_pipeline(&self, bytes: &[u8]) -> OpResult<Handle> {
        let info = PipelineInfo::parse(bytes).map_err(|e| invalid(e.to_string()))?;
        let polygon_line = self
            .gpu
            .features
            .contains(wgpu::Features::POLYGON_MODE_LINE);
        let pipeline = Pipeline::create(&self.gpu.device, &info, polygon_line)?;
        // Building the declared variant up front surfaces wgpu errors at compile time, not mid-frame.
        let depth = info.depth.map(|_| wgpu::TextureFormat::Depth32Float);
        pipeline.variant(&self.gpu.device, &pipeline.declared_key(depth))?;
        let mut slab = self
            .pipelines
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slab.insert(Arc::new(pipeline)).ok_or(OpError::Full)
    }

    pub fn destroy(&self, handle: Handle) -> OpResult<()> {
        let kind = Kind::of(handle).ok_or(OpError::Handle(Kind::Buffer, handle))?;
        if kind == Kind::Pipeline {
            return self
                .pipelines
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(handle)
                .map(drop)
                .ok_or(OpError::Handle(kind, handle));
        }
        let mut state = self.lock();
        let removed = match kind {
            Kind::Buffer => state.buffers.remove(handle).map(drop),
            Kind::Texture => state
                .textures
                .remove(handle)
                .map(|_| self.slots.forget(handle)),
            Kind::View => state.views.remove(handle).map(drop),
            Kind::Sampler => state.samplers.remove(handle).map(drop),
            Kind::Encoder => state.encoders.remove(handle).map(drop),
            Kind::Surface => state.surfaces.remove(handle).map(drop),
            Kind::Pipeline => unreachable!("handled above"),
        };
        removed.ok_or(OpError::Handle(kind, handle))?;
        if matches!(kind, Kind::Buffer | Kind::View | Kind::Sampler) {
            state.destroyed.push(handle);
        }
        Ok(())
    }

    pub fn create_encoder(&self) -> OpResult<Handle> {
        insert(&mut self.lock().encoders, Encoder::new())
    }

    pub fn create_surface(&self) -> OpResult<Handle> {
        insert(&mut self.lock().surfaces, Surface::default())
    }

    pub fn configure_surface(&self, surface: Handle, width: u32, height: u32) -> OpResult<()> {
        let max = self.gpu.limits.max_texture_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(invalid(format!("surface size {width}x{height}")));
        }
        let mut state = self.lock();
        let s = state
            .surfaces
            .get_mut(surface)
            .ok_or(OpError::Handle(Kind::Surface, surface))?;
        if s.texture.is_some() && s.width == width && s.height == height {
            return Ok(());
        }
        s.texture = Some(self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mcr-modbridge surface"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
        s.width = width;
        s.height = height;
        Ok(())
    }

    /// The surface image Rust composes later; also stored for tests when a capture was requested.
    pub fn present(&self, surface: Handle) -> OpResult<()> {
        let texture = {
            let state = self.lock();
            let s = state
                .surfaces
                .get(surface)
                .ok_or(OpError::Handle(Kind::Surface, surface))?;
            s.texture.clone()
        };
        let requested = CAPTURE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .requested;
        if let (true, Some(texture)) = (requested, texture) {
            let image = read_texture(self.gpu, &texture)?;
            let mut c = CAPTURE.lock().unwrap_or_else(PoisonError::into_inner);
            c.requested = false;
            c.image = Some(image);
        }
        Ok(())
    }

    /// Copies out a mappable buffer's host bytes (tests, readback checks).
    pub fn read_host(&self, handle: Handle) -> OpResult<Vec<u8>> {
        let state = self.lock();
        let b = state
            .buffers
            .get(handle)
            .ok_or(OpError::Handle(Kind::Buffer, handle))?;
        b.shadow
            .as_ref()
            .map(|s| s.bytes().to_vec())
            .ok_or_else(|| invalid("buffer has no host copy"))
    }

    pub fn write_host(&self, handle: Handle, offset: u64, data: &[u8]) -> OpResult<()> {
        let state = self.lock();
        let b = state
            .buffers
            .get(handle)
            .ok_or(OpError::Handle(Kind::Buffer, handle))?;
        let s = b
            .shadow
            .as_ref()
            .ok_or_else(|| invalid("buffer has no host copy"))?;
        if offset
            .checked_add(data.len() as u64)
            .is_none_or(|e| e > s.len() as u64)
        {
            return Err(invalid("host write out of range"));
        }
        s.write(offset as usize, data);
        Ok(())
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    pub fn live_counts(&self) -> [usize; 6] {
        let s = self.lock();
        [
            s.buffers.len(),
            s.textures.len(),
            s.views.len(),
            s.samplers.len(),
            self.pipelines
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len(),
            s.binds.len(),
        ]
    }
}

/// Synchronously reads an `Rgba8Unorm` texture, flipping rows: renderpearl targets are bottom-up.
fn read_texture(gpu: &Gpu, texture: &wgpu::Texture) -> OpResult<Image> {
    let (width, height) = (texture.width(), texture.height());
    let row = width * 4;
    let padded = align(u64::from(row), 256) as u32;
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mcr-modbridge capture"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    let index = gpu.queue.submit([enc.finish()]);
    let ok = Arc::new(AtomicU8::new(MAP_PENDING));
    let flag = ok.clone();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        flag.store(
            if r.is_ok() { MAP_OK } else { MAP_FAILED },
            Ordering::Release,
        );
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: Some(SUBMIT_WAIT),
        })
        .map_err(|e| OpError::Wgpu(e.to_string()))?;
    if ok.load(Ordering::Acquire) != MAP_OK {
        return Err(OpError::Wgpu("capture map failed".into()));
    }
    let mut rgba = Vec::with_capacity((row * height) as usize);
    {
        let view = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|e| OpError::Wgpu(e.to_string()))?;
        for y in (0..height).rev() {
            let start = (y * padded) as usize;
            rgba.extend_from_slice(&view[start..start + row as usize]);
        }
    }
    staging.unmap();
    Ok(Image {
        width,
        height,
        rgba,
    })
}

/// Encoder operations; Java issues them in vanilla's order and Rust records them in that order.
impl State {
    fn encoder(&mut self, handle: Handle) -> OpResult<&mut Encoder> {
        self.encoders
            .get_mut(handle)
            .ok_or(OpError::Handle(Kind::Encoder, handle))
    }

    fn texture(&self, handle: Handle) -> OpResult<&Texture> {
        self.textures
            .get(handle)
            .ok_or(OpError::Handle(Kind::Texture, handle))
    }

    fn buffer(&self, handle: Handle) -> OpResult<&Buffer> {
        self.buffers
            .get(handle)
            .ok_or(OpError::Handle(Kind::Buffer, handle))
    }

    /// Records `data` into `dest` at this point of the stream.
    fn stage_copy(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        dest: Handle,
        offset: u64,
        data: &[u8],
    ) -> OpResult<()> {
        let dst = self.buffer(dest)?;
        let dst_buffer = dst.buffer.clone();
        let wgpu_size = dst_buffer.size();
        if !offset.is_multiple_of(4) {
            return Err(invalid(format!(
                "buffer write offset {offset} is not 4-aligned"
            )));
        }
        let len = align(data.len() as u64, 4);
        if offset.checked_add(len).is_none_or(|e| e > wgpu_size) || data.len() as u64 > MAX_TRANSFER
        {
            return Err(invalid(format!(
                "write of {} bytes at {offset} overruns a {}-byte buffer",
                data.len(),
                dst.size
            )));
        }
        let tail: Option<Vec<u8>> = (len != data.len() as u64).then(|| {
            // Unaligned tails keep the bytes after them from the host copy when there is one.
            let shadow = dst.shadow.as_ref().map(|s| s.bytes());
            (data.len() as u64..len)
                .map(|i| {
                    shadow
                        .and_then(|s| s.get((offset + i) as usize).copied())
                        .unwrap_or(0)
                })
                .collect()
        });
        if let Some(s) = &dst.shadow {
            s.write(offset as usize, data);
        }
        let enc = self.encoder(encoder)?;
        let (src, src_offset, staging) = enc.ring.alloc(&gpu.device, len as usize, 4);
        staging[..data.len()].copy_from_slice(data);
        if let Some(t) = tail {
            staging[data.len()..].copy_from_slice(&t);
        }
        enc.cmd(&gpu.device)
            .copy_buffer_to_buffer(&src, src_offset, &dst_buffer, offset, len);
        Ok(())
    }

    pub fn write_buffer(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        dest: Handle,
        offset: u64,
        data: &[u8],
    ) -> OpResult<()> {
        self.stage_copy(gpu, encoder, dest, offset, data)
    }

    /// A closed write mapping: the host range goes to the GPU copy at this point of the stream.
    pub fn flush_mapped(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        buffer: Handle,
        offset: u64,
        len: u64,
    ) -> OpResult<()> {
        let b = self
            .buffers
            .get(buffer)
            .ok_or(OpError::Handle(Kind::Buffer, buffer))?;
        let shadow = b
            .shadow
            .as_ref()
            .ok_or_else(|| invalid("flush of a buffer that cannot be mapped"))?;
        if offset.checked_add(len).is_none_or(|e| e > b.size) {
            return Err(invalid(format!(
                "flush {offset}+{len} of a {}-byte buffer",
                b.size
            )));
        }
        let start = offset & !3;
        let end = align(offset + len, 4).min(b.buffer.size());
        if end <= start {
            return Ok(());
        }
        let dst = b.buffer.clone();
        let src = shadow.bytes();
        let avail = ((src.len() as u64).min(end).saturating_sub(start)) as usize;
        let enc = self
            .encoders
            .get_mut(encoder)
            .ok_or(OpError::Handle(Kind::Encoder, encoder))?;
        let size = (end - start) as usize;
        let (ring, ring_offset, staging) = enc.ring.alloc(&gpu.device, size, 4);
        staging[..avail].copy_from_slice(&src[start as usize..start as usize + avail]);
        staging[avail..].fill(0);
        enc.cmd(&gpu.device)
            .copy_buffer_to_buffer(&ring, ring_offset, &dst, start, size as u64);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        src: Handle,
        src_offset: u64,
        dst: Handle,
        dst_offset: u64,
        len: u64,
    ) -> OpResult<()> {
        let s = self.buffer(src)?.buffer.clone();
        let d = self.buffer(dst)?.buffer.clone();
        let rounded = align(len, 4);
        if !src_offset.is_multiple_of(4) || !dst_offset.is_multiple_of(4) {
            return Err(invalid("buffer copy offsets must be 4-aligned"));
        }
        if src_offset.checked_add(rounded).is_none_or(|e| e > s.size())
            || dst_offset.checked_add(rounded).is_none_or(|e| e > d.size())
            || (src == dst
                && src_offset < dst_offset + rounded
                && dst_offset < src_offset + rounded)
        {
            return Err(invalid(format!(
                "buffer copy of {len} bytes is out of range"
            )));
        }
        if len == 0 {
            return Ok(());
        }
        self.encoder(encoder)?
            .cmd(&gpu.device)
            .copy_buffer_to_buffer(&s, src_offset, &d, dst_offset, rounded);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_texture(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        dest: Handle,
        data: &[u8],
        mip: u32,
        layer: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let tex = self.texture(dest)?;
        region(tex, mip, layer, x, y, w, h)?;
        if tex.format.is_depth_stencil_format() {
            return Err(invalid("writes into depth textures are not supported"));
        }
        let bs = tex.block_size();
        let row = u64::from(w) * u64::from(bs);
        let need = row * u64::from(h);
        if (data.len() as u64) < need || need > MAX_TRANSFER {
            return Err(invalid(format!(
                "texture write needs {need} bytes, got {}",
                data.len()
            )));
        }
        let padded = align(row, 256);
        let texture = tex.texture.clone();
        let aspect = tex.aspect();
        let enc = self.encoder(encoder)?;
        let (src, offset, staging) =
            enc.ring
                .alloc(&gpu.device, (padded * u64::from(h)) as usize, 256);
        for r in 0..h as usize {
            let from = r * row as usize;
            let to = r * padded as usize;
            staging[to..to + row as usize].copy_from_slice(&data[from..from + row as usize]);
        }
        enc.cmd(&gpu.device).copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &src,
                layout: wgpu::TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(h),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d { x, y, z: layer },
                aspect,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// `src` rows are `src_w` texels wide; the copy starts at texel (`src_x`, `src_y`) of them.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        src: Handle,
        src_offset: u64,
        src_x: u32,
        src_y: u32,
        src_w: u32,
        src_h: u32,
        dest: Handle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        mip: u32,
        layer: u32,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let tex = self.texture(dest)?;
        region(tex, mip, layer, x, y, w, h)?;
        let bs = u64::from(tex.block_size());
        let buffer = self.buffer(src)?.buffer.clone();
        if src_x.checked_add(w).is_none_or(|e| e > src_w)
            || src_y.checked_add(h).is_none_or(|e| e > src_h)
        {
            return Err(invalid(
                "buffer-to-texture source rectangle is out of range",
            ));
        }
        let row = u64::from(src_w) * bs;
        let start = src_offset + (u64::from(src_x) + u64::from(src_y) * u64::from(src_w)) * bs;
        let end = start + row * u64::from(h - 1) + u64::from(w) * bs;
        if end > buffer.size()
            || !start.is_multiple_of(bs)
            || (tex.format.is_depth_stencil_format() && !start.is_multiple_of(4))
        {
            return Err(invalid(
                "buffer-to-texture source is out of range or misaligned",
            ));
        }
        let texture = tex.texture.clone();
        let aspect = tex.aspect();
        let cmd = self.encoder(encoder)?.cmd(&gpu.device);
        let copy = |cmd: &mut wgpu::CommandEncoder,
                    offset: u64,
                    bytes_per_row: Option<u32>,
                    oy: u32,
                    rows: u32| {
            cmd.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row,
                        rows_per_image: bytes_per_row.map(|_| rows),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d { x, y: oy, z: layer },
                    aspect,
                },
                wgpu::Extent3d {
                    width: w,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
        };
        if row.is_multiple_of(256) {
            copy(cmd, start, Some(row as u32), y, h);
        } else {
            for r in 0..h {
                copy(cmd, start + u64::from(r) * row, None, y + r, 1);
            }
        }
        Ok(())
    }

    /// Before Java copies out of its main color while composing, the scene is flipped into it: Java
    /// draws into the scene and never into its own main color then.
    pub fn refresh_main(
        &mut self,
        gpu: &Gpu,
        slots: &super::slots::Slots,
        encoder: Handle,
        src: Handle,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let t = self.texture(src)?;
        if !t.virtual_main || !t.format.has_color_aspect() {
            return Ok(());
        }
        let main = t.texture.clone();
        let Some(scene) = slots.scene_for_main((main.width(), main.height())) else {
            return Ok(());
        };
        if !super::flip::Flipper::fits(&scene, &main) {
            return Ok(());
        }
        let enc = self
            .encoders
            .get_mut(encoder)
            .ok_or(OpError::Handle(Kind::Encoder, encoder))?;
        let flipper = self
            .flipper
            .get_or_insert_with(|| super::flip::Flipper::new(&gpu.device));
        flipper.record(&gpu.device, enc.cmd(&gpu.device), &scene, &main);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_buffer(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        src: Handle,
        dest: Handle,
        dest_offset: u64,
        id: u64,
        mip: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let tex = self.texture(src)?;
        region(tex, mip, 0, x, y, w, h)?;
        let bs = tex.block_size();
        let row = w * bs;
        let padded = align(u64::from(row), 256) as u32;
        let size = u64::from(padded) * u64::from(h);
        let dst = self.buffer(dest)?;
        if dest_offset
            .checked_add(u64::from(row) * u64::from(h))
            .is_none_or(|e| e > dst.size)
            || size > MAX_TRANSFER
        {
            return Err(invalid("texture readback overruns its buffer"));
        }
        let shadowed = dst.shadow.is_some();
        let dst_buffer = dst.buffer.clone();
        if !shadowed && (!dest_offset.is_multiple_of(4) || !row.is_multiple_of(4)) {
            return Err(invalid("GPU readback rows must be 4-aligned"));
        }
        let texture = tex.texture.clone();
        let aspect = tex.aspect();
        let usage = if shadowed {
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ
        } else {
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC
        };
        let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mcr-modbridge readback"),
            size,
            usage,
            mapped_at_creation: false,
        });
        let enc = self.encoder(encoder)?;
        if enc.recorded.len() + enc.pending.len() >= MAX_PENDING_READBACKS {
            return Err(invalid("too many readbacks in flight"));
        }
        let submit = enc.submits + 1;
        let cmd = enc.cmd(&gpu.device);
        cmd.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let target = if shadowed {
            ReadbackTarget::Shadow {
                staging,
                mapped: Arc::new(AtomicU8::new(MAP_PENDING)),
                dest,
                dest_offset,
                row_bytes: row,
                padded_row: padded,
                rows: h,
            }
        } else {
            for r in 0..u64::from(h) {
                cmd.copy_buffer_to_buffer(
                    &staging,
                    r * u64::from(padded),
                    &dst_buffer,
                    dest_offset + r * u64::from(row),
                    u64::from(row),
                );
            }
            ReadbackTarget::Gpu
        };
        enc.recorded.push(Readback { id, submit, target });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_texture(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        src: Handle,
        dst: Handle,
        mip: u32,
        dst_x: u32,
        dst_y: u32,
        src_x: u32,
        src_y: u32,
        w: u32,
        h: u32,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let s = self.texture(src)?;
        let d = self.texture(dst)?;
        region(s, mip, 0, src_x, src_y, w, h)?;
        region(d, mip, 0, dst_x, dst_y, w, h)?;
        if s.format.remove_srgb_suffix() != d.format.remove_srgb_suffix() {
            return Err(invalid(format!(
                "copy from {:?} to {:?}",
                s.format, d.format
            )));
        }
        if s.format.is_depth_stencil_format()
            && (s.mip_size(mip) != (w, h) || d.mip_size(mip) != (w, h))
        {
            return Err(invalid("depth copies must cover the whole mip"));
        }
        if src == dst {
            return Err(invalid("copy within one texture"));
        }
        let (st, dt) = (s.texture.clone(), d.texture.clone());
        let (sa, da) = (s.aspect(), d.aspect());
        self.encoder(encoder)?
            .cmd(&gpu.device)
            .copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &st,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: src_x,
                        y: src_y,
                        z: 0,
                    },
                    aspect: sa,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &dt,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: dst_x,
                        y: dst_y,
                        z: 0,
                    },
                    aspect: da,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        Ok(())
    }

    /// Full clears of mip 0 (every layer); `None` leaves that attachment out.
    pub fn clear(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        color: Option<(Handle, [f32; 4])>,
        depth: Option<(Handle, f32)>,
        world: Option<&WorldTargets>,
    ) -> OpResult<()> {
        let color_tex = color
            .map(|(h, c)| self.texture(h).map(|t| (t, c)))
            .transpose()?;
        let depth_tex = depth
            .map(|(h, d)| self.texture(h).map(|t| (t, d)))
            .transpose()?;
        if let (Some((t, d)), Some(w)) = (depth_tex, world)
            && t.virtual_main
        {
            if color_tex.is_some_and(|(t, _)| !t.virtual_main) {
                self.clear(gpu, encoder, color, None, None)?;
            }
            if !w.scene_depth {
                let depth_has_stencil = w.depth.texture().format().has_stencil_aspect();
                self.encoder(encoder)?.defer_clear(
                    &gpu.device,
                    PendingClear {
                        texture: w.depth.texture().clone(),
                        view: Some(w.depth.clone()),
                        value: ClearValue::Depth(d, depth_has_stencil),
                    },
                );
            }
            return Ok(());
        }
        if let Some((t, _)) = color_tex
            && !t.format.has_color_aspect()
        {
            return Err(invalid("color clear of a depth texture"));
        }
        let color_tex = color_tex.filter(|(t, _)| !t.virtual_main);
        if color_tex.is_none() && depth_tex.is_none() {
            return Ok(());
        }
        if let Some((t, _)) = depth_tex
            && !t.format.has_depth_aspect()
        {
            return Err(invalid("depth clear of a color texture"));
        }
        if let (Some((c, _)), Some((d, _))) = (color_tex, depth_tex)
            && (c.width, c.height) != (d.width, d.height)
        {
            return Err(invalid("color and depth clear sizes differ"));
        }
        let layers = color_tex
            .map(|(t, _)| t.layers)
            .or(depth_tex.map(|(t, _)| t.layers))
            .unwrap_or(0);
        if color_tex.is_none_or(|(t, _)| t.layers == 1)
            && depth_tex.is_none_or(|(t, _)| t.layers == 1)
        {
            let mut pending = Vec::with_capacity(2);
            if let Some((t, c)) = color_tex {
                pending.push(PendingClear {
                    texture: t.texture.clone(),
                    view: None,
                    value: ClearValue::Color(c),
                });
            }
            if let Some((t, d)) = depth_tex {
                pending.push(PendingClear {
                    texture: t.texture.clone(),
                    view: None,
                    value: ClearValue::Depth(d, t.format.has_stencil_aspect()),
                });
            }
            let enc = self.encoder(encoder)?;
            for c in pending {
                enc.defer_clear(&gpu.device, c);
            }
            return Ok(());
        }
        let mut passes = Vec::with_capacity(layers as usize);
        for layer in 0..layers {
            let cv = color_tex
                .and_then(|(t, c)| (layer < t.layers).then(|| (t.layer_view(0, layer), c)));
            let dv = depth_tex.and_then(|(t, d)| {
                (layer < t.layers)
                    .then(|| (t.layer_view(0, layer), d, t.format.has_stencil_aspect()))
            });
            passes.push((cv, dv));
        }
        self.flush_clears(gpu, encoder)?;
        let cmd = self.encoder(encoder)?.cmd(&gpu.device);
        for (cv, dv) in &passes {
            let color_att = cv.as_ref().map(|(v, c)| wgpu::RenderPassColorAttachment {
                view: v,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(c[0]),
                        g: f64::from(c[1]),
                        b: f64::from(c[2]),
                        a: f64::from(c[3]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            });
            let depth_att =
                dv.as_ref()
                    .map(|(v, d, stencil)| wgpu::RenderPassDepthStencilAttachment {
                        view: v,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(d.clamp(0.0, 1.0)),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: stencil.then_some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Store,
                        }),
                    });
            drop(cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("java clear"),
                color_attachments: &[color_att],
                depth_stencil_attachment: depth_att,
                ..Default::default()
            }));
        }
        Ok(())
    }

    /// Records the clears no pass took over, in order.
    pub fn flush_clears(&mut self, gpu: &Gpu, encoder: Handle) -> OpResult<()> {
        let enc = self.encoder(encoder)?;
        if enc.clears.is_empty() {
            return Ok(());
        }
        let clears = std::mem::take(&mut enc.clears);
        record_clears(enc.cmd(&gpu.device), clears);
        Ok(())
    }

    /// A scissored clear of one mip: a full-screen triangle carrying the clear values.
    #[allow(clippy::too_many_arguments)]
    pub fn clear_region(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        color: (Handle, [f32; 4]),
        depth: (Handle, f32),
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        mip: u32,
        world: bool,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let ct = self.texture(color.0)?;
        let dt = self.texture(depth.0)?;
        if world && (ct.virtual_main || dt.virtual_main) {
            return Ok(());
        }
        region(ct, mip, 0, x, y, w, h)?;
        region(dt, mip, 0, x, y, w, h)?;
        if ct.mip_size(mip) != dt.mip_size(mip)
            || !ct.format.has_color_aspect()
            || !dt.format.has_depth_aspect()
        {
            return Err(invalid("region clear attachments do not match"));
        }
        let keep_color = !ct.virtual_main;
        let key = ClearKey {
            color: keep_color.then_some(ct.format),
            depth: Some(dt.format),
        };
        let has_stencil = dt.format.has_stencil_aspect();
        let cv = ct.layer_view(mip, 0);
        let dv = dt.layer_view(mip, 0);
        let pipeline = self.clear_pipeline(gpu, key);
        let d = depth.1.clamp(0.0, 1.0);
        let [r, g, b, a] = color.1;
        let mut vertices = [0f32; 24];
        for (i, (vx, vy)) in [(-1.0f32, -1.0f32), (3.0, -1.0), (-1.0, 3.0)]
            .into_iter()
            .enumerate()
        {
            vertices[i * 8..i * 8 + 8].copy_from_slice(&[vx, vy, d, 1.0, r, g, b, a]);
        }
        let enc = self.encoder(encoder)?;
        let (vb, offset, staging) = enc.ring.alloc(&gpu.device, 96, 16);
        for (dst, v) in staging.as_chunks_mut::<4>().0.iter_mut().zip(vertices) {
            dst.copy_from_slice(&v.to_ne_bytes());
        }
        let cmd = enc.cmd(&gpu.device);
        let mut pass = cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("java region clear"),
            color_attachments: &[keep_color.then_some(wgpu::RenderPassColorAttachment {
                view: &cv,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &dv,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: has_stencil.then_some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
            }),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_scissor_rect(x, y, w, h);
        pass.set_vertex_buffer(0, vb.slice(offset..offset + 96));
        pass.draw(0..3, 0..1);
        Ok(())
    }

    fn clear_pipeline(&mut self, gpu: &Gpu, key: ClearKey) -> wgpu::RenderPipeline {
        if let Some(p) = self.clears.get(&key) {
            return p.clone();
        }
        let shader = self
            .clear_shader
            .get_or_insert_with(|| {
                gpu.device
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("mcr-modbridge clear"),
                        source: wgpu::ShaderSource::Wgsl(CLEAR_WGSL.into()),
                    })
            })
            .clone();
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mcr-modbridge clear"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: Default::default(),
                depth_stencil: key.depth.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[key.color.map(|format| wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        self.clears.insert(key, pipeline.clone());
        pipeline
    }

    /// Records a copy of `view` into the surface image (the swapchain blit of a real backend).
    pub fn blit(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        surface: Handle,
        view: Handle,
    ) -> OpResult<()> {
        self.flush_clears(gpu, encoder)?;
        let v = self
            .views
            .get(view)
            .ok_or(OpError::Handle(Kind::View, view))?;
        let tex = self.texture(v.texture)?;
        let (base_mip, src) = (v.base_mip, tex.texture.clone());
        let src_format = tex.format;
        let (vw, vh) = tex.mip_size(base_mip);
        let s = self
            .surfaces
            .get(surface)
            .ok_or(OpError::Handle(Kind::Surface, surface))?;
        let dst = s
            .texture
            .clone()
            .ok_or_else(|| invalid("blit to an unconfigured surface"))?;
        if src_format.remove_srgb_suffix() != wgpu::TextureFormat::Rgba8Unorm {
            return Err(invalid(format!("blit from {src_format:?}")));
        }
        let (w, h) = (vw.min(dst.width()), vh.min(dst.height()));
        self.encoder(encoder)?
            .cmd(&gpu.device)
            .copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &src,
                    mip_level: base_mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                dst.as_image_copy(),
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        Ok(())
    }

    /// Java filled `[offset, offset + len)` of a transient block for this submit.
    pub fn transient_dirty(&mut self, block: Handle, offset: u64, len: u64) -> OpResult<()> {
        let b = self
            .buffers
            .get_mut(block)
            .ok_or(OpError::Handle(Kind::Buffer, block))?;
        if !b.transient || offset.checked_add(len).is_none_or(|e| e > b.size) {
            return Err(invalid("transient range is out of its block"));
        }
        if len == 0 {
            return Ok(());
        }
        let was_clean = b.dirty.is_none();
        b.dirty = Some(match b.dirty {
            Some((s, e)) => (s.min(offset), e.max(offset + len)),
            None => (offset, offset + len),
        });
        if was_clean {
            self.dirty_blocks.push(block);
        }
        Ok(())
    }

    /// Uploads staged bytes and submits (or queues the lot for the render thread), maps readbacks
    /// and, when submitting directly, keeps at most two submits in flight.
    pub fn submit(
        &mut self,
        gpu: &Gpu,
        stats: &Stats,
        slots: &Slots,
        encoder: Handle,
    ) -> OpResult<u64> {
        let defer = slots.is_active();
        let (writes, uploaded) = self.stage(gpu, encoder, defer, true)?;
        let enc = self.encoder(encoder)?;
        let (cmd, failed) = finish_valid(gpu, enc.cmd.take());
        enc.submits += 1;
        let n = enc.submits;
        let mut maps = Vec::new();
        for mut rb in std::mem::take(&mut enc.recorded) {
            rb.submit = n;
            if let ReadbackTarget::Shadow {
                staging, mapped, ..
            } = &rb.target
            {
                maps.push((staging.clone(), mapped.clone()));
            }
            enc.pending.push(rb);
        }
        let (slot, frame) = slots.current();
        let sub = Submission {
            slot,
            frame,
            writes,
            cmd,
            maps,
            track: enc.track.clone(),
            n,
            epoch: n,
        };
        if defer {
            slots.defer(gpu, sub);
        } else {
            sub.execute(gpu);
        }
        stats.submits.fetch_add(1, Ordering::Relaxed);
        stats.uploaded.fetch_add(uploaded, Ordering::Relaxed);
        if !defer && n > FRAMES_IN_FLIGHT {
            self.wait(gpu, encoder, n - FRAMES_IN_FLIGHT, Some(SUBMIT_WAIT))?;
        }
        let destroyed = std::mem::take(&mut self.destroyed);
        self.binds.purge(&destroyed);
        failed.map_or(Ok(n), Err)
    }

    /// While composing, queues what was recorded so far under the current slot, so a slot change
    /// can cut the frame; readbacks, fences and Java's transient memory still follow the next submit.
    pub fn split(
        &mut self,
        gpu: &Gpu,
        stats: &Stats,
        slots: &Slots,
        encoder: Handle,
    ) -> OpResult<()> {
        if !slots.is_active() {
            self.encoder(encoder)?;
            return Ok(());
        }
        let (writes, uploaded) = self.stage(gpu, encoder, true, false)?;
        let enc = self.encoder(encoder)?;
        let (cmd, failed) = finish_valid(gpu, enc.cmd.take());
        if cmd.is_none() && writes.is_empty() {
            return failed.map_or(Ok(()), Err);
        }
        let (slot, frame) = slots.current();
        slots.defer(
            gpu,
            Submission {
                slot,
                frame,
                writes,
                cmd,
                maps: Vec::new(),
                track: enc.track.clone(),
                n: enc.submits,
                epoch: enc.submits + 1,
            },
        );
        stats.uploaded.fetch_add(uploaded, Ordering::Relaxed);
        failed.map_or(Ok(()), Err)
    }

    /// Collects the dirty host ranges and staged ring bytes for the next queue write; with `defer`
    /// they are copied out to travel with a queued submission.
    #[allow(clippy::type_complexity)]
    fn stage(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        defer: bool,
        end_submit: bool,
    ) -> OpResult<(Vec<(wgpu::Buffer, u64, Vec<u8>)>, u64)> {
        let start = cost_start();
        let mut writes = Vec::new();
        for block in std::mem::take(&mut self.dirty_blocks) {
            let Some(b) = self.buffers.get_mut(block) else {
                continue;
            };
            let Some((s, e)) = b.dirty.take() else {
                continue;
            };
            let (s, e) = (s & !3, align(e, 4).min(b.buffer.size()));
            if let Some(shadow) = &b.shadow {
                let bytes = shadow.bytes();
                let host_end = (bytes.len() as u64).min(e);
                if host_end > s {
                    let mut data = pooled_copy(&bytes[s as usize..host_end as usize]);
                    data.resize((e - s) as usize, 0);
                    writes.push((b.buffer.clone(), s, data));
                }
            }
        }
        let dirty: u64 = writes.iter().map(|(_, _, d)| d.len() as u64).sum();
        let enc = self.encoder(encoder)?;
        let ring = enc
            .ring
            .upload(&gpu.queue, defer.then_some(&mut writes), end_submit);
        add_cost(cost::STAGE, start);
        Ok((writes, dirty + ring))
    }

    /// Waits until submit `n` completed; `false` on timeout or when `n` is not on the queue yet
    /// (queued work waits for the render thread, which may be waiting for this very frame).
    pub fn wait(
        &mut self,
        gpu: &Gpu,
        encoder: Handle,
        n: u64,
        timeout: Option<Duration>,
    ) -> OpResult<bool> {
        let enc = self.encoder(encoder)?;
        let track = enc.track.clone();
        let done = track.done.load(Ordering::Acquire);
        if done >= n {
            return Ok(true);
        }
        if n > enc.submits || n > track.queued.load(Ordering::Acquire) {
            return Ok(false);
        }
        // Host writes reach the GPU as queue-ordered copies, so only a readback can observe completion.
        if !enc.pending.iter().any(|rb| rb.submit <= n) {
            let mut inflight = track.inflight();
            inflight.retain(|(k, _)| *k > done);
            return Ok(true);
        }
        let Some(index) = track
            .inflight()
            .iter()
            .find(|(k, _)| *k >= n)
            .map(|(_, i)| i.clone())
        else {
            return Ok(true);
        };
        let result = gpu.device.poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout,
        });
        let done = track.done.load(Ordering::Acquire);
        track.inflight().retain(|(k, _)| *k > done);
        match result {
            Ok(_) => Ok(true),
            Err(wgpu::PollError::Timeout) => Ok(false),
            Err(e) => Err(OpError::Wgpu(e.to_string())),
        }
    }

    pub fn fence_pending(&mut self, encoder: Handle) -> OpResult<u64> {
        Ok(self.encoder(encoder)?.submits + 1)
    }

    /// Readback ids whose data is in place, in completion order.
    pub fn poll(&mut self, gpu: &Gpu, encoder: Handle) -> OpResult<Vec<u64>> {
        if self.encoder(encoder)?.pending.is_empty() {
            return Ok(Vec::new());
        }
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        let enc = self.encoder(encoder)?;
        let done = enc.track.done.load(Ordering::Acquire);
        let mut finished = Vec::new();
        let mut i = 0;
        while i < enc.pending.len() {
            let ready = match &enc.pending[i].target {
                ReadbackTarget::Shadow { mapped, .. } => {
                    mapped.load(Ordering::Acquire) != MAP_PENDING
                }
                ReadbackTarget::Gpu => done >= enc.pending[i].submit,
            };
            if ready {
                finished.push(enc.pending.swap_remove(i));
            } else {
                i += 1;
            }
        }
        finished.sort_by_key(|r| r.id);
        let mut ids = Vec::with_capacity(finished.len());
        for rb in finished {
            if let ReadbackTarget::Shadow {
                staging,
                mapped,
                dest,
                dest_offset,
                row_bytes,
                padded_row,
                rows,
            } = &rb.target
                && mapped.load(Ordering::Acquire) == MAP_OK
            {
                if let Some(shadow) = self.buffers.get(*dest).and_then(|b| b.shadow.as_ref())
                    && let Ok(view) = staging.slice(..).get_mapped_range()
                {
                    for r in 0..*rows as usize {
                        let from = r * *padded_row as usize;
                        shadow.write(
                            *dest_offset as usize + r * *row_bytes as usize,
                            &view[from..from + *row_bytes as usize],
                        );
                    }
                }
                staging.unmap();
            }
            ids.push(rb.id);
        }
        Ok(ids)
    }
}

/// Finishes `cmd`, or drops it when its recording failed: an invalid buffer would sink mcr's whole
/// frame batch.
fn finish_valid(
    gpu: &Gpu,
    cmd: Option<wgpu::CommandEncoder>,
) -> (Option<wgpu::CommandBuffer>, Option<OpError>) {
    let Some(cmd) = cmd else {
        return (None, None);
    };
    let start = cost_start();
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let buffer = cmd.finish();
    let error = pollster::block_on(scope.pop());
    add_cost(cost::FINISH, start);
    match error {
        None => (Some(buffer), None),
        Some(e) => (None, Some(OpError::Wgpu(e.to_string()))),
    }
}

/// Shared `u32` indices turning a triangle fan of up to `vertices` vertices into a list.
pub fn ensure_fan(
    slot: &mut Option<(wgpu::Buffer, u32)>,
    device: &wgpu::Device,
    vertices: u32,
) -> Option<wgpu::Buffer> {
    const MAX_FAN: u32 = 1 << 16;
    if !(3..=MAX_FAN).contains(&vertices) {
        return None;
    }
    if let Some((b, n)) = slot.as_ref()
        && *n >= vertices
    {
        return Some(b.clone());
    }
    let n = vertices.next_power_of_two().clamp(64, MAX_FAN);
    let mut data = Vec::with_capacity((n as usize - 2) * 12);
    for i in 1..n - 1 {
        for v in [0u32, i, i + 1] {
            data.extend_from_slice(&v.to_ne_bytes());
        }
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mcr-modbridge fan indices"),
        size: data.len() as u64,
        usage: wgpu::BufferUsages::INDEX,
        mapped_at_creation: true,
    });
    if let Ok(mut view) = buffer.slice(..).get_mapped_range_mut() {
        view.copy_from_slice(&data);
    }
    buffer.unmap();
    *slot = Some((buffer.clone(), n));
    Some(buffer)
}

const CLEAR_WGSL: &str = r"
struct V { @builtin(position) pos: vec4f, @location(0) color: vec4f }
@vertex fn vs(@location(0) pos: vec4f, @location(1) color: vec4f) -> V { return V(pos, color); }
@fragment fn fs(v: V) -> @location(0) vec4f { return v.color; }
";

#[cfg(test)]
mod ring_tests {
    use super::*;

    #[test]
    fn splits_of_one_submit_upload_disjoint_ranges() {
        let Ok(gpu) = context::gpu() else {
            eprintln!("skipped: no gpu");
            return;
        };
        let mut ring = UploadRing::default();
        let mut writes = Vec::new();
        ring.alloc(&gpu.device, 8, 4).2.fill(1);
        assert_eq!(ring.upload(&gpu.queue, Some(&mut writes), false), 8);
        let (_, offset, _) = ring.alloc(&gpu.device, 8, 4);
        assert_eq!(offset, 8, "a split keeps earlier staging");
        assert_eq!(ring.upload(&gpu.queue, Some(&mut writes), false), 8);
        let ranges: Vec<_> = writes.iter().map(|(_, o, d)| (*o, d.len())).collect();
        assert_eq!(ranges, [(0, 8), (8, 8)]);
        assert_eq!(ring.upload(&gpu.queue, Some(&mut writes), true), 0);
        let (_, offset, _) = ring.alloc(&gpu.device, 8, 4);
        assert_eq!(offset, 0, "the next submit reuses the block");
    }

    #[test]
    fn idle_oversized_ring_blocks_are_released() {
        assert!(keep_ring_block(1, RING_BLOCK, false));
        assert!(keep_ring_block(KEPT_RING_BLOCKS, RING_BLOCK, false));
        assert!(!keep_ring_block(KEPT_RING_BLOCKS + 1, RING_BLOCK, false));
        assert!(!keep_ring_block(1, 512 << 20, false));
        assert!(keep_ring_block(1, 512 << 20, true));
    }
}
