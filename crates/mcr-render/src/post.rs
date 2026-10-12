use std::num::NonZeroU64;

/// Most chains run in one frame; the server list plus the spectated chain is cut here.
pub const MAX_POST_CHAINS: usize = 8;

/// The vanilla post chains mcr implements; every other id is ignored, like a chain vanilla fails to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostChain {
    Blur,
    Creeper,
    Invert,
    Spider,
    /// Vanilla's `entity_outline`: runs on the glow silhouettes, never on the world image, so
    /// [`PostChain::from_id`] does not return it.
    EntityOutline,
}

impl PostChain {
    pub const ALL: [PostChain; 4] = [Self::Blur, Self::Creeper, Self::Invert, Self::Spider];

    /// Parses `minecraft:<name>` or a bare `<name>`.
    pub fn from_id(id: &str) -> Option<Self> {
        let name = id.strip_prefix("minecraft:").unwrap_or(id);
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Blur => "blur",
            Self::Creeper => "creeper",
            Self::Invert => "invert",
            Self::Spider => "spider",
            Self::EntityOutline => "entity_outline",
        }
    }

    pub(crate) fn passes(self) -> &'static [Pass] {
        match self {
            Self::Blur => &BLUR,
            Self::Creeper => &CREEPER,
            Self::Invert => &INVERT,
            Self::Spider => &SPIDER,
            Self::EntityOutline => &ENTITY_OUTLINE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Main,
    Swap,
    Temp,
    LargeBlur,
    SmallBlur,
}

const POOL: usize = 4;

impl Slot {
    fn pool_index(self) -> Option<usize> {
        match self {
            Slot::Main => None,
            Slot::Swap => Some(0),
            Slot::Temp => Some(1),
            Slot::LargeBlur => Some(2),
            Slot::SmallBlur => Some(3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Program {
    /// `radius` 0 takes the menu background blurriness.
    BoxBlur {
        dir: [f32; 2],
        radius: f32,
    },
    Convolve {
        rows: [[f32; 3]; 3],
    },
    Bits {
        levels: f32,
        mosaic: f32,
    },
    Invert {
        amount: f32,
    },
    Blit {
        tint: [f32; 4],
    },
    SpiderClip(&'static SpiderEye),
    Sobel,
    OutlineBlur {
        dir: [f32; 2],
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpiderEye {
    scale: [f32; 2],
    offset: [f32; 2],
    degrees: f32,
    clip: [f32; 4],
    fade: [f32; 4],
}

const PROGRAMS: usize = 8;

impl Program {
    fn kind(self) -> usize {
        match self {
            Program::BoxBlur { .. } => 0,
            Program::Convolve { .. } => 1,
            Program::Bits { .. } => 2,
            Program::Invert { .. } => 3,
            Program::Blit { .. } => 4,
            Program::SpiderClip(_) => 5,
            Program::Sobel => 6,
            Program::OutlineBlur { .. } => 7,
        }
    }

    fn entry_points(kind: usize) -> (&'static str, &'static str) {
        match kind {
            0 => ("vs_quad", "fs_box_blur"),
            1 => ("vs_quad", "fs_convolve"),
            2 => ("vs_quad", "fs_bits"),
            3 => ("vs_quad", "fs_invert"),
            4 => ("vs_quad", "fs_blit"),
            5 => ("vs_rotscale", "fs_spider"),
            6 => ("vs_quad", "fs_sobel"),
            _ => ("vs_quad", "fs_outline_blur"),
        }
    }

    fn params(self) -> [[f32; 4]; 4] {
        let mut out = [[0.0; 4]; 4];
        match self {
            Program::BoxBlur { dir, radius } => out[0] = [dir[0], dir[1], radius, 0.0],
            Program::OutlineBlur { dir } => out[0] = [dir[0], dir[1], 0.0, 0.0],
            Program::Sobel => {}
            Program::Convolve { rows } => {
                for (o, r) in out.iter_mut().zip(rows) {
                    *o = [r[0], r[1], r[2], 0.0];
                }
            }
            Program::Bits { levels, mosaic } => out[0] = [levels, mosaic, 0.0, 0.0],
            Program::Invert { amount } => out[0][0] = amount,
            Program::Blit { tint } => out[0] = tint,
            Program::SpiderClip(e) => {
                out[0] = [e.scale[0], e.scale[1], e.offset[0], e.offset[1]];
                out[1][0] = e.degrees;
                out[2] = e.clip;
                out[3] = e.fade;
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pass {
    pub program: Program,
    pub input: Slot,
    pub linear: bool,
    /// Second input (the spider's blurred backdrop), always nearest.
    pub aux: Option<Slot>,
    pub output: Slot,
}

const fn pass(program: Program, input: Slot, linear: bool, output: Slot) -> Pass {
    Pass {
        program,
        input,
        linear,
        aux: None,
        output,
    }
}

const fn blur(dir: [f32; 2], radius: f32, input: Slot, output: Slot) -> Pass {
    pass(Program::BoxBlur { dir, radius }, input, true, output)
}

const fn eye(input: Slot, aux: Slot, output: Slot, e: &'static SpiderEye) -> Pass {
    Pass {
        program: Program::SpiderClip(e),
        input,
        linear: false,
        aux: Some(aux),
        output,
    }
}

const H: [f32; 2] = [1.0, 0.0];
const V: [f32; 2] = [0.0, 1.0];
const WHITE: [f32; 4] = [1.0; 4];

static BLUR: [Pass; 6] = [
    blur(H, 0.0, Slot::Main, Slot::Swap),
    blur(V, 0.0, Slot::Swap, Slot::Main),
    blur(H, 0.0, Slot::Main, Slot::Swap),
    blur(V, 0.0, Slot::Swap, Slot::Main),
    blur(H, 0.0, Slot::Main, Slot::Swap),
    blur(V, 0.0, Slot::Swap, Slot::Main),
];

static CREEPER: [Pass; 2] = [
    pass(
        Program::Convolve {
            rows: [[0.0; 3], [0.3, 0.59, 0.11], [0.0; 3]],
        },
        Slot::Main,
        false,
        Slot::Swap,
    ),
    pass(
        Program::Bits {
            levels: 16.0,
            mosaic: 4.0,
        },
        Slot::Swap,
        false,
        Slot::Main,
    ),
];

static INVERT: [Pass; 2] = [
    pass(
        Program::Invert { amount: 0.8 },
        Slot::Main,
        false,
        Slot::Swap,
    ),
    pass(Program::Blit { tint: WHITE }, Slot::Swap, false, Slot::Main),
];

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const WIDE_FADE: [f32; 4] = [0.1, 0.1, 0.9, 0.9];
const NARROW_CLIP: [f32; 4] = [0.21, 0.0, 0.79, 1.0];
const NARROW_FADE: [f32; 4] = [0.31, 0.1, 0.69, 0.9];

static EYES: [SpiderEye; 5] = [
    SpiderEye {
        scale: [1.25, 2.0],
        offset: [-0.125, -0.1],
        degrees: 0.0,
        clip: FULL,
        fade: WIDE_FADE,
    },
    SpiderEye {
        scale: [2.35, 4.2],
        offset: [-1.1, -1.5],
        degrees: -45.0,
        clip: NARROW_CLIP,
        fade: NARROW_FADE,
    },
    SpiderEye {
        scale: [2.35, 4.2],
        offset: [0.45, -4.45],
        degrees: 45.0,
        clip: NARROW_CLIP,
        fade: NARROW_FADE,
    },
    SpiderEye {
        scale: [2.35, 2.35],
        offset: [-0.385, -1.29],
        degrees: 0.0,
        clip: FULL,
        fade: NARROW_FADE,
    },
    SpiderEye {
        scale: [2.35, 2.35],
        offset: [-0.965, -1.29],
        degrees: 0.0,
        clip: FULL,
        fade: NARROW_FADE,
    },
];

static SPIDER: [Pass; 10] = [
    blur(H, 15.0, Slot::Main, Slot::Temp),
    blur(V, 15.0, Slot::Temp, Slot::LargeBlur),
    blur(H, 7.0, Slot::Main, Slot::Temp),
    blur(V, 7.0, Slot::Temp, Slot::SmallBlur),
    eye(Slot::Main, Slot::LargeBlur, Slot::Temp, &EYES[0]),
    eye(Slot::SmallBlur, Slot::Temp, Slot::Swap, &EYES[1]),
    eye(Slot::SmallBlur, Slot::Swap, Slot::Temp, &EYES[2]),
    eye(Slot::SmallBlur, Slot::Temp, Slot::Swap, &EYES[3]),
    eye(Slot::SmallBlur, Slot::Swap, Slot::Temp, &EYES[4]),
    pass(
        Program::Blit {
            tint: [1.0, 0.8, 0.8, 1.0],
        },
        Slot::Temp,
        false,
        Slot::Main,
    ),
];

static ENTITY_OUTLINE: [Pass; 4] = [
    pass(Program::Sobel, Slot::Main, false, Slot::Swap),
    pass(
        Program::OutlineBlur { dir: H },
        Slot::Swap,
        true,
        Slot::Main,
    ),
    pass(
        Program::OutlineBlur { dir: V },
        Slot::Main,
        true,
        Slot::Swap,
    ),
    pass(Program::Blit { tint: WHITE }, Slot::Swap, false, Slot::Main),
];

/// Why a pass graph cannot run in place on the main image.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GraphError {
    ReadsOwnOutput(usize),
    ReadsUnwritten(usize, Slot),
    LastPassNotMain,
    Empty,
}

#[cfg(test)]
pub(crate) fn validate(passes: &[Pass]) -> Result<(), GraphError> {
    let last = passes.last().ok_or(GraphError::Empty)?;
    if last.output != Slot::Main {
        return Err(GraphError::LastPassNotMain);
    }
    let mut written = [false; POOL];
    for (i, p) in passes.iter().enumerate() {
        for read in std::iter::once(p.input).chain(p.aux) {
            if read == p.output {
                return Err(GraphError::ReadsOwnOutput(i));
            }
            if let Some(k) = read.pool_index()
                && !written[k]
            {
                return Err(GraphError::ReadsUnwritten(i, read));
            }
        }
        if let Some(k) = p.output.pool_index() {
            written[k] = true;
        }
    }
    Ok(())
}

const PARAMS_SIZE: u64 = 80;
const MAX_PASSES: usize = MAX_POST_CHAINS * 10;
const MAX_MENU_BLUR: u32 = 10;

#[derive(Clone, PartialEq)]
struct CacheKey {
    main: wgpu::Texture,
    chains: Vec<PostChain>,
    menu_radius: u32,
}

struct Prepared {
    key: CacheKey,
    groups: Vec<wgpu::BindGroup>,
    main_view: wgpu::TextureView,
}

/// Runs post chains in place on a sampleable, render-attachable image.
pub struct PostStack {
    format: wgpu::TextureFormat,
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: [Option<wgpu::RenderPipeline>; PROGRAMS],
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    params: wgpu::Buffer,
    stride: u64,
    staging: Vec<u8>,
    pool: [Option<(wgpu::Texture, wgpu::TextureView)>; POOL],
    pool_size: (u32, u32),
    prepared: Option<Prepared>,
}

impl std::fmt::Debug for PostStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostStack")
            .field("format", &self.format)
            .field("pool_size", &self.pool_size)
            .finish_non_exhaustive()
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn sampler(device: &wgpu::Device, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("post"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    })
}

impl PostStack {
    /// `format` is the main image's format; pooled targets share it.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                sampler_entry(2),
                sampler_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(PARAMS_SIZE),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let stride = PARAMS_SIZE.next_multiple_of(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        ));
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post params"),
            size: stride * MAX_PASSES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            format,
            shader,
            layout,
            pipeline_layout,
            pipelines: Default::default(),
            nearest: sampler(device, wgpu::FilterMode::Nearest),
            linear: sampler(device, wgpu::FilterMode::Linear),
            params,
            stride,
            staging: Vec::new(),
            pool: Default::default(),
            pool_size: (0, 0),
            prepared: None,
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Drops pooled targets and cached bindings; the next `record` recreates what it needs.
    pub fn release(&mut self) {
        self.pool = Default::default();
        self.pool_size = (0, 0);
        self.prepared = None;
    }

    fn pipeline(&mut self, device: &wgpu::Device, kind: usize) -> &wgpu::RenderPipeline {
        let (vs, fs) = Program::entry_points(kind);
        self.pipelines[kind].get_or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    /// Creates the pooled targets `chains` write, at `size`; other slots stay unallocated.
    fn ensure_pool(&mut self, device: &wgpu::Device, size: (u32, u32), chains: &[PostChain]) {
        if self.pool_size != size {
            self.pool = Default::default();
            self.pool_size = size;
            self.prepared = None;
        }
        let outputs = chains.iter().flat_map(|c| c.passes()).map(|p| p.output);
        for k in outputs.filter_map(Slot::pool_index) {
            if self.pool[k].is_some() {
                continue;
            }
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("post target"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.pool[k] = Some((t, v));
            self.prepared = None;
        }
    }

    fn view<'a>(
        pool: &'a [Option<(wgpu::Texture, wgpu::TextureView)>; POOL],
        main: &'a wgpu::TextureView,
        slot: Slot,
    ) -> &'a wgpu::TextureView {
        match slot.pool_index().and_then(|i| pool[i].as_ref()) {
            Some((_, v)) => v,
            None => main,
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        main: &wgpu::Texture,
        chains: &[PostChain],
        menu_radius: u32,
    ) {
        let fresh = self.prepared.as_ref().is_some_and(|p| {
            p.key.main == *main && p.key.chains == chains && p.key.menu_radius == menu_radius
        });
        if fresh {
            return;
        }
        let main_view = main.create_view(&wgpu::TextureViewDescriptor::default());
        let size = [main.width() as f32, main.height() as f32];
        self.staging.clear();
        let mut groups = Vec::new();
        for p in chains.iter().flat_map(|c| c.passes()) {
            let mut block = [[0.0f32; 4]; 5];
            block[0] = [size[0], size[1], menu_radius as f32, 0.0];
            block[1..].copy_from_slice(&p.program.params());
            let start = self.staging.len();
            self.staging.extend_from_slice(bytemuck::cast_slice(&block));
            self.staging.resize(start + self.stride as usize, 0);
            let input = Self::view(&self.pool, &main_view, p.input);
            let aux = Self::view(&self.pool, &main_view, p.aux.unwrap_or(p.input));
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post pass"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(aux),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(if p.linear {
                            &self.linear
                        } else {
                            &self.nearest
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.nearest),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.params,
                            offset: 0,
                            size: NonZeroU64::new(PARAMS_SIZE),
                        }),
                    },
                ],
            }));
        }
        if !self.staging.is_empty() {
            queue.write_buffer(&self.params, 0, &self.staging);
        }
        self.prepared = Some(Prepared {
            key: CacheKey {
                main: main.clone(),
                chains: chains.to_vec(),
                menu_radius,
            },
            groups,
            main_view,
        });
    }

    /// Records `chains` (at most [`MAX_POST_CHAINS`]) over `main`, which must be in this stack's
    /// format with `TEXTURE_BINDING` and `RENDER_ATTACHMENT` usage. `menu_blur` is the menu
    /// background blurriness that `blur` uses.
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        main: &wgpu::Texture,
        chains: &[PostChain],
        menu_blur: u32,
    ) {
        let chains = &chains[..chains.len().min(MAX_POST_CHAINS)];
        if chains.is_empty() || main.format() != self.format {
            return;
        }
        self.ensure_pool(device, (main.width(), main.height()), chains);
        self.prepare(device, queue, main, chains, menu_blur.min(MAX_MENU_BLUR));
        for kind in 0..PROGRAMS {
            if chains
                .iter()
                .flat_map(|c| c.passes())
                .any(|p| p.program.kind() == kind)
            {
                self.pipeline(device, kind);
            }
        }
        let prepared = self.prepared.as_ref().expect("prepared above");
        let passes = chains.iter().flat_map(|c| c.passes());
        for (i, (p, group)) in passes.zip(&prepared.groups).enumerate() {
            let Some(pipeline) = &self.pipelines[p.program.kind()] else {
                continue;
            };
            let target = Self::view(&self.pool, &prepared.main_view, p.output);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[(i as u64 * self.stride) as u32]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chain_runs_in_place() {
        for chain in PostChain::ALL.into_iter().chain([PostChain::EntityOutline]) {
            assert_eq!(validate(chain.passes()), Ok(()), "{chain:?}");
        }
    }

    #[test]
    fn graph_rejects_feedback_and_unwritten_reads() {
        let blit = Program::Blit { tint: WHITE };
        assert_eq!(
            validate(&[pass(blit, Slot::Main, false, Slot::Main)]),
            Err(GraphError::ReadsOwnOutput(0))
        );
        assert_eq!(
            validate(&[pass(blit, Slot::Temp, false, Slot::Main)]),
            Err(GraphError::ReadsUnwritten(0, Slot::Temp))
        );
        assert_eq!(
            validate(&[pass(blit, Slot::Main, false, Slot::Swap)]),
            Err(GraphError::LastPassNotMain)
        );
        assert_eq!(validate(&[]), Err(GraphError::Empty));
    }

    #[test]
    fn worst_case_fits_the_parameter_buffer() {
        let longest = PostChain::ALL.iter().map(|c| c.passes().len()).max();
        assert!(longest.unwrap() * MAX_POST_CHAINS <= MAX_PASSES);
    }

    #[test]
    fn ids_parse_with_or_without_namespace() {
        assert_eq!(
            PostChain::from_id("minecraft:spider"),
            Some(PostChain::Spider)
        );
        assert_eq!(PostChain::from_id("invert"), Some(PostChain::Invert));
        assert_eq!(PostChain::from_id("minecraft:entity_outline"), None);
        assert_eq!(PostChain::from_id("foo:creeper"), None);
        assert_eq!(PostChain::from_id("minecraft:end_of_frame"), None);
    }

    #[test]
    fn program_parameters_land_in_their_slots() {
        let p = Program::SpiderClip(&EYES[1]).params();
        assert_eq!(p[0], [2.35, 4.2, -1.1, -1.5]);
        assert_eq!(p[1][0], -45.0);
        assert_eq!(p[2], NARROW_CLIP);
        assert_eq!(p[3], NARROW_FADE);
    }
}
