//! `improvedTransparency`: vanilla's wavelet order-independent transparency. Translucent surfaces are
//! drawn three times (depth bounds, transmittance coefficients, weighted colour) into targets that exist
//! only while the option is on, then composited over the world.

const OIT_SRC: &str = include_str!("shaders/oit.wgsl");
const RESOLVE_SRC: &str = include_str!("shaders/oit_resolve.wgsl");
pub(crate) const COEFF_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OitStage {
    DepthBounds = 0,
    Transmittance = 1,
    Accumulate = 2,
}

impl OitStage {
    pub const ALL: [Self; 3] = [Self::DepthBounds, Self::Transmittance, Self::Accumulate];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The fragment entry suffix [`OitPipelineDesc`] appends: `bounds`, `coefficients` or `accumulate`.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::DepthBounds => "bounds",
            Self::Transmittance => "coefficients",
            Self::Accumulate => "accumulate",
        }
    }

    /// The colour targets of this stage's pass, with their blending.
    pub fn targets(
        self,
        bounds_format: wgpu::TextureFormat,
    ) -> Vec<Option<wgpu::ColorTargetState>> {
        let (format, op, count) = match self {
            Self::DepthBounds => (bounds_format, wgpu::BlendOperation::Max, 1),
            Self::Transmittance => (COEFF_FORMAT, wgpu::BlendOperation::Add, 2),
            Self::Accumulate => (COEFF_FORMAT, wgpu::BlendOperation::Add, 1),
        };
        let component = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: op,
        };
        vec![
            Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: component,
                    alpha: component,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            });
            count
        ]
    }

    /// Depth tested against the opaque world, never written.
    pub fn depth_state() -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }
    }

    /// Fragment entry points: `fs_oit_bounds`, `fs_oit_coefficients` and `fs_oit_accumulate`.
    pub fn entry(self) -> &'static str {
        match self {
            Self::DepthBounds => "fs_oit_bounds",
            Self::Transmittance => "fs_oit_coefficients",
            Self::Accumulate => "fs_oit_accumulate",
        }
    }
}

/// The three stage pipelines over a shader whose fragment entries are `{fragment}_{suffix}`.
#[derive(Debug)]
pub struct OitPipelineDesc<'a> {
    pub label: &'a str,
    pub layout: &'a wgpu::PipelineLayout,
    pub module: &'a wgpu::ShaderModule,
    pub vertex: &'a str,
    pub buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    pub primitive: wgpu::PrimitiveState,
    pub bias: wgpu::DepthBiasState,
    pub fragment: &'a str,
    pub bounds_format: wgpu::TextureFormat,
}

impl OitPipelineDesc<'_> {
    pub fn build(&self, device: &wgpu::Device) -> [wgpu::RenderPipeline; 3] {
        OitStage::ALL.map(|stage| {
            let entry = format!("{}_{}", self.fragment, stage.suffix());
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("{} {}", self.label, stage.suffix())),
                layout: Some(self.layout),
                vertex: wgpu::VertexState {
                    module: self.module,
                    entry_point: Some(self.vertex),
                    compilation_options: Default::default(),
                    buffers: self.buffers,
                },
                primitive: self.primitive,
                depth_stencil: Some(wgpu::DepthStencilState {
                    bias: self.bias,
                    ..OitStage::depth_state()
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: self.module,
                    entry_point: Some(&entry),
                    compilation_options: Default::default(),
                    targets: &stage.targets(self.bounds_format),
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

/// WGSL helpers for a pipeline's improved transparency stages, binding their textures at `group`.
pub fn oit_wgsl(group: u32) -> String {
    OIT_SRC.replace("OIT_GROUP", &group.to_string())
}

/// What an overlay needs to build its improved transparency pipelines.
#[derive(Debug, Clone, Copy)]
pub struct OitInfo<'a> {
    pub layout: &'a wgpu::BindGroupLayout,
    pub bounds_format: wgpu::TextureFormat,
}

pub(crate) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oit"),
        entries: &[
            texture(0),
            texture(1),
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture(4),
        ],
    })
}

fn target(
    device: &wgpu::Device,
    label: &str,
    (width, height): (u32, u32),
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

/// The screen-sized targets and the per-stage bind groups.
pub(crate) struct OitTargets {
    pub bounds: wgpu::TextureView,
    pub culled: wgpu::TextureView,
    pub coeffs: [wgpu::TextureView; 2],
    pub accum: wgpu::TextureView,
    /// Bound while the stage of the same index draws; a target is never bound while it is written.
    pub stage_groups: [wgpu::BindGroup; 3],
    pub cull_group: wgpu::BindGroup,
    pub composite_group: wgpu::BindGroup,
}

/// Pipelines that do not depend on what draws into the stages.
pub(crate) struct OitShared {
    pub layout: wgpu::BindGroupLayout,
    pub bounds_format: wgpu::TextureFormat,
    pub params: wgpu::Buffer,
    dummy: wgpu::TextureView,
    pub copy: wgpu::RenderPipeline,
    pub cull: wgpu::RenderPipeline,
    pub composite: wgpu::RenderPipeline,
}

impl OitShared {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let bounds_format = if device
            .features()
            .contains(wgpu::Features::FLOAT32_BLENDABLE)
        {
            wgpu::TextureFormat::Rgba32Float
        } else {
            wgpu::TextureFormat::Rgba16Float
        };
        let layout = bind_group_layout(device);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oit params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oit resolve"),
            source: wgpu::ShaderSource::Wgsl([oit_wgsl(0).as_str(), RESOLVE_SRC].concat().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oit resolve"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |fs: &str,
                        format: wgpu::TextureFormat,
                        blend: Option<wgpu::BlendState>,
                        depth: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let depth = |write: bool, compare| {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        Self {
            copy: pipeline(
                "fs_bounds_copy",
                bounds_format,
                None,
                depth(false, wgpu::CompareFunction::Always),
            ),
            cull: pipeline(
                "fs_bounds_cull",
                bounds_format,
                None,
                depth(true, wgpu::CompareFunction::GreaterEqual),
            ),
            composite: pipeline(
                "fs_composite",
                color_format,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                depth(true, wgpu::CompareFunction::GreaterEqual),
            ),
            dummy: target(device, "oit dummy", (1, 1), COEFF_FORMAT),
            layout,
            bounds_format,
            params,
        }
    }

    pub fn info(&self) -> OitInfo<'_> {
        OitInfo {
            layout: &self.layout,
            bounds_format: self.bounds_format,
        }
    }

    pub fn targets(&self, device: &wgpu::Device, size: (u32, u32)) -> OitTargets {
        let bounds = target(device, "oit depth bounds", size, self.bounds_format);
        let culled = target(device, "oit culled bounds", size, self.bounds_format);
        let coeffs = [0, 1].map(|_| target(device, "oit transmittance", size, COEFF_FORMAT));
        let accum = target(device, "oit accumulate", size, COEFF_FORMAT);
        let d = &self.dummy;
        let group = |views: [&wgpu::TextureView; 4]| {
            let tex = |binding, view| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            };
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oit"),
                layout: &self.layout,
                entries: &[
                    tex(0, views[0]),
                    tex(1, views[1]),
                    tex(2, views[2]),
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.params.as_entire_binding(),
                    },
                    tex(4, views[3]),
                ],
            })
        };
        OitTargets {
            stage_groups: [
                group([d, d, d, d]),
                group([&culled, d, d, d]),
                group([&culled, &coeffs[0], &coeffs[1], d]),
            ],
            cull_group: group([&bounds, d, d, d]),
            composite_group: group([&bounds, &coeffs[0], &coeffs[1], &accum]),
            bounds,
            culled,
            coeffs,
            accum,
        }
    }
}

impl OitTargets {
    /// The colour attachments a stage writes, cleared first.
    pub fn attachments(&self, stage: OitStage) -> Vec<Option<wgpu::RenderPassColorAttachment<'_>>> {
        let clear = |view, color| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        match stage {
            OitStage::DepthBounds => vec![clear(
                &self.bounds,
                wgpu::Color {
                    r: f64::from(-f32::MAX),
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
            )],
            OitStage::Transmittance => self
                .coeffs
                .iter()
                .map(|v| clear(v, wgpu::Color::TRANSPARENT))
                .collect(),
            OitStage::Accumulate => vec![clear(&self.accum, wgpu::Color::TRANSPARENT)],
        }
    }
}
