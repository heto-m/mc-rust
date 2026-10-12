//! Render pipelines from vanilla's `CreateInfo`, with per-attachment-format and per-binding variants.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use wgpu::naga;

use super::enums;
use super::info::{ColorTarget, Depth, PipelineInfo, Stage};
use super::shader::{self, ShaderError};
use super::spv::{self, UniformKind};

static LAYOUT_IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Shader(#[from] ShaderError),
    #[error("pipeline needs a {0:?} shader")]
    MissingStage(Stage),
    #[error("vertex attribute {location} ({format}) does not fit the {stride}-byte stride")]
    Attribute {
        location: u32,
        format: &'static str,
        stride: u32,
    },
    #[error("unsupported vertex format {0}")]
    VertexFormat(&'static str),
    #[error("vertex step rate {0} needs instance divisors")]
    StepRate(u32),
    #[error("binding {0} is not declared by the pipeline")]
    Binding(u32),
    #[error("wgpu rejected the pipeline: {0}")]
    Wgpu(String),
    #[error("pipeline exceeds the {0} limit")]
    Limit(&'static str),
}

const MAX_VERTEX_SLOTS: u32 = 8;
const MAX_STRIDE: u32 = 2048;

/// What each flattened uniform index holds in the pipeline layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Unused,
    Buffer,
    Texture {
        sample: TextureSample,
        dimension: wgpu::TextureViewDimension,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSample {
    Float,
    Uint,
    Sint,
    Depth,
}

#[derive(Debug)]
pub struct Layout {
    pub id: u64,
    pub bind_group: wgpu::BindGroupLayout,
    pub pipeline: wgpu::PipelineLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariantKey {
    pub colors: [Option<wgpu::TextureFormat>; 8],
    pub color_count: u8,
    pub depth: Option<wgpu::TextureFormat>,
    /// Bit `i`: the texture at uniform `i` cannot be filtered (depth or 32-bit float).
    pub unfilterable: u64,
    /// Drawn with mcr's top-down projection (world slots): front faces are CCW again.
    pub flip: bool,
}

#[derive(Debug)]
pub struct VertexBuffer {
    pub stride: u64,
    pub step: wgpu::VertexStepMode,
    /// End of the furthest attribute; 0 when the slot has none.
    pub attributes_end: u64,
    attributes: Vec<wgpu::VertexAttribute>,
}

#[derive(Debug)]
pub struct Pipeline {
    pub name: String,
    vertex: wgpu::ShaderModule,
    fragment: wgpu::ShaderModule,
    buffers: Vec<VertexBuffer>,
    primitive: wgpu::PrimitiveState,
    depth: Option<Depth>,
    targets: Vec<Option<ColorTarget>>,
    pub slots: Vec<Slot>,
    pub push_constants: bool,
    /// Std140 size of each buffer block, the push block last; bindings are never smaller.
    pub block_sizes: Vec<u64>,
    pub fan: bool,
    /// Reads vanilla's `Projection` block, so a flipped projection flips its winding.
    pub uses_projection: bool,
    layouts: Mutex<FxHashMap<u64, Arc<Layout>>>,
    variants: Mutex<FxHashMap<VariantKey, wgpu::RenderPipeline>>,
}

fn texture_slot(class: naga::ImageClass, dim: naga::ImageDimension) -> Slot {
    let sample = match class {
        naga::ImageClass::Sampled {
            kind: naga::ScalarKind::Uint,
            ..
        } => TextureSample::Uint,
        naga::ImageClass::Sampled {
            kind: naga::ScalarKind::Sint,
            ..
        } => TextureSample::Sint,
        naga::ImageClass::Depth { .. } => TextureSample::Depth,
        _ => TextureSample::Float,
    };
    let dimension = match dim {
        naga::ImageDimension::Cube => wgpu::TextureViewDimension::Cube,
        naga::ImageDimension::D3 => wgpu::TextureViewDimension::D3,
        naga::ImageDimension::D1 => wgpu::TextureViewDimension::D1,
        naga::ImageDimension::D2 => wgpu::TextureViewDimension::D2,
    };
    Slot::Texture { sample, dimension }
}

/// Uniform slots and push-constant use, read back from the translated modules.
/// Slots, push-constant use, and the std140 size of every buffer block (index `n` is the push block).
#[allow(clippy::type_complexity)]
fn reflect(
    modules: &[shader::Translated],
    uniforms: &[UniformKind],
) -> Result<(Vec<Slot>, bool, Vec<u64>), PipelineError> {
    let mut slots = vec![Slot::Unused; uniforms.len()];
    let mut sizes = vec![0u64; uniforms.len() + 1];
    let mut push = false;
    let push_binding = spv::push_constant_binding(uniforms.len() as u32);
    for t in modules {
        for (_, var) in t.module.global_variables.iter() {
            let Some(binding) = &var.binding else {
                continue;
            };
            let b = binding.binding;
            let size = u64::from(t.module.types[var.ty].inner.size(t.module.to_ctx()));
            if b == push_binding {
                push = true;
                sizes[uniforms.len()] = sizes[uniforms.len()].max(size);
                continue;
            }
            let index = (b / 2) as usize;
            let slot = slots.get_mut(index).ok_or(PipelineError::Binding(b))?;
            match t.module.types[var.ty].inner {
                naga::TypeInner::Image { dim, class, .. } => *slot = texture_slot(class, dim),
                naga::TypeInner::Sampler { .. } => {
                    if *slot == Slot::Unused {
                        *slot = texture_slot(
                            naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Float,
                                multi: false,
                            },
                            naga::ImageDimension::D2,
                        );
                    }
                }
                _ => {
                    *slot = Slot::Buffer;
                    sizes[index] = sizes[index].max(size);
                }
            }
        }
    }
    Ok((slots, push, sizes))
}

fn blend(target: &ColorTarget) -> Option<wgpu::BlendState> {
    let b = target.blend?;
    let component = |e: [u32; 3]| {
        let operation = enums::blend_op(e[2]).unwrap_or(wgpu::BlendOperation::Add);
        // wgpu requires factor One for min/max, which is what min/max ignore anyway.
        let (src, dst) = if matches!(
            operation,
            wgpu::BlendOperation::Min | wgpu::BlendOperation::Max
        ) {
            (wgpu::BlendFactor::One, wgpu::BlendFactor::One)
        } else {
            (
                enums::blend_factor(e[0]).unwrap_or(wgpu::BlendFactor::One),
                enums::blend_factor(e[1]).unwrap_or(wgpu::BlendFactor::Zero),
            )
        };
        wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation,
        }
    };
    Some(wgpu::BlendState {
        color: component(b.color),
        alpha: component(b.alpha),
    })
}

/// Waits for the validation errors of what `f` created on this thread.
pub fn checked<T>(device: &wgpu::Device, f: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = f();
    match pollster::block_on(scope.pop()) {
        None => Ok(value),
        Some(e) => Err(e.to_string()),
    }
}

impl Pipeline {
    pub fn create(
        device: &wgpu::Device,
        info: &PipelineInfo,
        polygon_line: bool,
    ) -> Result<Self, PipelineError> {
        if info.uniforms.len() > super::pass::MAX_UNIFORMS {
            return Err(PipelineError::Limit("uniform"));
        }
        if info
            .vertex_buffers
            .iter()
            .any(|b| b.slot >= MAX_VERTEX_SLOTS || b.stride > MAX_STRIDE)
        {
            return Err(PipelineError::Limit("vertex buffer"));
        }
        let translated = shader::translate(info)?;
        let (slots, push_constants, block_sizes) = reflect(&translated, &info.uniform_kinds())?;
        let mut vertex = None;
        let mut fragment = None;
        for t in translated {
            let module = checked(device, || {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&info.name),
                    source: wgpu::ShaderSource::Naga(Cow::Owned(t.module)),
                })
            })
            .map_err(PipelineError::Wgpu)?;
            match t.stage {
                Stage::Vertex => vertex = Some(module),
                Stage::Fragment => fragment = Some(module),
            }
        }
        let vertex = vertex.ok_or(PipelineError::MissingStage(Stage::Vertex))?;
        let fragment = fragment.ok_or(PipelineError::MissingStage(Stage::Fragment))?;

        let slot_count = info
            .vertex_buffers
            .iter()
            .map(|b| b.slot + 1)
            .max()
            .unwrap_or(0);
        let mut buffers: Vec<VertexBuffer> = (0..slot_count)
            .map(|_| VertexBuffer {
                stride: 0,
                step: wgpu::VertexStepMode::Vertex,
                attributes_end: 0,
                attributes: Vec::new(),
            })
            .collect();
        for vb in &info.vertex_buffers {
            let buffer = &mut buffers[vb.slot as usize];
            buffer.stride = u64::from(vb.stride);
            buffer.step = match vb.step_rate {
                0 => wgpu::VertexStepMode::Vertex,
                1 => wgpu::VertexStepMode::Instance,
                n => return Err(PipelineError::StepRate(n)),
            };
        }
        for a in &info.attributes {
            let name = enums::GPU_FORMATS[a.format as usize];
            let (format, _) =
                enums::vertex_format(a.format).ok_or(PipelineError::VertexFormat(name))?;
            let Some(buffer) = buffers.get_mut(a.slot as usize) else {
                return Err(PipelineError::VertexFormat(name));
            };
            if buffer.stride != 0 && u64::from(a.offset) + format.size() > buffer.stride {
                return Err(PipelineError::Attribute {
                    location: a.location,
                    format: name,
                    stride: buffer.stride as u32,
                });
            }
            buffer.attributes_end = buffer
                .attributes_end
                .max(u64::from(a.offset) + format.size());
            buffer.attributes.push(wgpu::VertexAttribute {
                format,
                offset: u64::from(a.offset),
                shader_location: a.location,
            });
        }

        let topology =
            enums::topology(info.topology).unwrap_or(wgpu::PrimitiveTopology::TriangleList);
        let primitive = wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            // naga flips clip-space Y, which mirrors winding: vanilla's CCW front faces arrive as CW.
            front_face: wgpu::FrontFace::Cw,
            cull_mode: info.cull.then_some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: if info.wireframe && polygon_line {
                wgpu::PolygonMode::Line
            } else {
                wgpu::PolygonMode::Fill
            },
            conservative: false,
        };
        let uses_projection = info
            .uniforms
            .iter()
            .zip(&slots)
            .any(|(u, slot)| u.name == "Projection" && *slot != Slot::Unused);
        Ok(Self {
            name: info.name.clone(),
            vertex,
            fragment,
            buffers,
            primitive,
            depth: info.depth,
            targets: info.color_targets.clone(),
            slots,
            push_constants,
            block_sizes,
            fan: info.topology == enums::TOPOLOGY_TRIANGLE_FAN,
            uses_projection,
            layouts: Mutex::new(FxHashMap::default()),
            variants: Mutex::new(FxHashMap::default()),
        })
    }

    /// The key for the formats the pipeline declares, with or without a `Depth32Float` attachment.
    pub fn declared_key(&self, depth: Option<wgpu::TextureFormat>) -> VariantKey {
        let mut colors = [None; 8];
        for (c, t) in colors.iter_mut().zip(&self.targets) {
            *c = t.and_then(|t| enums::texture_format(t.format));
        }
        VariantKey {
            colors,
            color_count: self.targets.len().min(8) as u8,
            depth,
            unfilterable: 0,
            flip: false,
        }
    }

    pub fn vertex_layouts(&self) -> &[VertexBuffer] {
        &self.buffers
    }

    pub fn push_binding(&self) -> u32 {
        spv::push_constant_binding(self.slots.len() as u32)
    }

    pub fn layout(&self, device: &wgpu::Device, unfilterable: u64) -> Arc<Layout> {
        let mut layouts = self.layouts.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(l) = layouts.get(&unfilterable) {
            return l.clone();
        }
        let visibility = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries = Vec::with_capacity(self.slots.len() * 2 + 1);
        for (i, slot) in self.slots.iter().enumerate() {
            let i = i as u32;
            match *slot {
                Slot::Unused => {}
                Slot::Buffer => entries.push(uniform(spv::buffer_binding(i))),
                Slot::Texture { sample, dimension } => {
                    let unfilterable = unfilterable & (1 << i) != 0;
                    let (sample_type, sampler) = match sample {
                        TextureSample::Float if !unfilterable => (
                            wgpu::TextureSampleType::Float { filterable: true },
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        TextureSample::Float => (
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::SamplerBindingType::NonFiltering,
                        ),
                        TextureSample::Uint => (
                            wgpu::TextureSampleType::Uint,
                            wgpu::SamplerBindingType::NonFiltering,
                        ),
                        TextureSample::Sint => (
                            wgpu::TextureSampleType::Sint,
                            wgpu::SamplerBindingType::NonFiltering,
                        ),
                        TextureSample::Depth => (
                            wgpu::TextureSampleType::Depth,
                            wgpu::SamplerBindingType::Comparison,
                        ),
                    };
                    entries.push(wgpu::BindGroupLayoutEntry {
                        binding: spv::texture_binding(i),
                        visibility,
                        ty: wgpu::BindingType::Texture {
                            sample_type,
                            view_dimension: dimension,
                            multisampled: false,
                        },
                        count: None,
                    });
                    entries.push(wgpu::BindGroupLayoutEntry {
                        binding: spv::sampler_binding(i),
                        visibility,
                        ty: wgpu::BindingType::Sampler(sampler),
                        count: None,
                    });
                }
            }
        }
        if self.push_constants {
            entries.push(uniform(self.push_binding()));
        }
        let bind_group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&self.name),
            entries: &entries,
        });
        let pipeline = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&self.name),
            bind_group_layouts: &[Some(&bind_group)],
            immediate_size: 0,
        });
        let layout = Arc::new(Layout {
            id: LAYOUT_IDS.fetch_add(1, Ordering::Relaxed),
            bind_group,
            pipeline,
        });
        layouts.insert(unfilterable, layout.clone());
        layout
    }

    /// The compiled pipeline for these attachments, creating it on first use.
    pub fn variant(
        &self,
        device: &wgpu::Device,
        key: &VariantKey,
    ) -> Result<(wgpu::RenderPipeline, Arc<Layout>), PipelineError> {
        let layout = self.layout(device, key.unfilterable);
        if let Some(p) = self
            .variants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
        {
            return Ok((p.clone(), layout));
        }
        let buffers: Vec<Option<wgpu::VertexBufferLayout>> = self
            .buffers
            .iter()
            .map(|b| {
                Some(wgpu::VertexBufferLayout {
                    array_stride: b.stride,
                    step_mode: b.step,
                    attributes: &b.attributes,
                })
            })
            .collect();
        let targets: Vec<Option<wgpu::ColorTargetState>> = (0..key.color_count as usize)
            .map(|i| {
                let format = key.colors[i]?;
                let declared = self.targets.get(i).copied().flatten();
                Some(wgpu::ColorTargetState {
                    format,
                    blend: declared.as_ref().and_then(blend),
                    write_mask: wgpu::ColorWrites::from_bits_truncate(
                        declared.map_or(0, |t| t.write_mask),
                    ),
                })
            })
            .collect();
        let depth_stencil = key.depth.map(|format| {
            let (write, compare, bias) = match self.depth {
                Some(d) => (
                    d.write,
                    enums::compare(d.compare).unwrap_or(wgpu::CompareFunction::Always),
                    wgpu::DepthBiasState {
                        constant: d.bias_constant.round() as i32,
                        slope_scale: d.bias_slope,
                        clamp: 0.0,
                    },
                ),
                None => (false, wgpu::CompareFunction::Always, Default::default()),
            };
            wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias,
            }
        });
        let pipeline = checked(device, || {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&self.name),
                layout: Some(&layout.pipeline),
                vertex: wgpu::VertexState {
                    module: &self.vertex,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    front_face: if key.flip {
                        wgpu::FrontFace::Ccw
                    } else {
                        self.primitive.front_face
                    },
                    ..self.primitive
                },
                depth_stencil,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.fragment,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        })
        .map_err(PipelineError::Wgpu)?;
        self.variants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), pipeline.clone());
        Ok((pipeline, layout))
    }

    pub fn variant_count(&self) -> usize {
        self.variants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}
