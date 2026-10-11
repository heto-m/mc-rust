use bytemuck::{Pod, Zeroable};
use mcr_render::wgpu;
use mcr_render::{Environment, OverlayCtx};

use crate::atlas::{DYNAMIC_SIZE, ParticleAtlas, SLOT, Uv};
use crate::sim::{self, Kind, Light, P, POOL_CAP, Pool};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Instance {
    pub pos_size: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub roll: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    right: [f32; 4],
    up: [f32; 4],
}

const INSTANCE_BYTES: u64 = std::mem::size_of::<Instance>() as u64;

pub(crate) struct Gpu {
    format: wgpu::TextureFormat,
    uniforms: wgpu::Buffer,
    particle_bg: wgpu::BindGroup,
    dynamic_bg: wgpu::BindGroup,
    dynamic_tex: wgpu::Texture,
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    instances: [wgpu::Buffer; 3],
    scratch: Vec<Instance>,
    lightmap: Vec<[f32; 4]>,
    lightmap_env: Option<Environment>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu").field("format", &self.format).finish()
    }
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    w: u32,
    h: u32,
    data: Option<&[u8]>,
) -> wgpu::Texture {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if let Some(data) = data {
        queue.write_texture(
            t.as_image_copy(),
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
    t
}

impl Gpu {
    pub(crate) fn new(ctx: &OverlayCtx<'_>, atlas: &ParticleAtlas) -> Self {
        let device = ctx.device;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas_tex = texture(
            device,
            ctx.queue,
            "particle atlas",
            atlas.width,
            atlas.height,
            Some(&atlas.rgba),
        );
        let dynamic_tex = texture(
            device,
            ctx.queue,
            "particle dynamic atlas",
            DYNAMIC_SIZE,
            DYNAMIC_SIZE,
            Some(&vec![0u8; (DYNAMIC_SIZE * DYNAMIC_SIZE * 4) as usize]),
        );
        let group = |tex: &wgpu::Texture| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("particles"),
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &tex.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let particle_bg = group(&atlas_tex);
        let dynamic_bg = group(&dynamic_tex);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particle.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let attributes: Vec<wgpu::VertexAttribute> = (0..4)
            .map(|i| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: i * 16,
                shader_location: i as u32,
            })
            .collect();
        let make = |fs: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: INSTANCE_BYTES,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(blend.is_none()),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: ctx.format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let instances = std::array::from_fn(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(["opaque", "translucent", "terrain"][i]),
                size: POOL_CAP as u64 * INSTANCE_BYTES,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self {
            format: ctx.format,
            uniforms,
            particle_bg,
            dynamic_bg,
            dynamic_tex,
            opaque: make("fs_opaque", None),
            blend: make("fs_blend", Some(wgpu::BlendState::ALPHA_BLENDING)),
            instances,
            scratch: Vec::with_capacity(POOL_CAP),
            lightmap: Vec::new(),
            lightmap_env: None,
        }
    }

    pub(crate) fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub(crate) fn upload_slot(&self, queue: &wgpu::Queue, slot: u32, rgba: &[u8]) {
        let (x, y) = crate::atlas::DynamicSlots::origin(slot);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.dynamic_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SLOT * 4),
                rows_per_image: Some(SLOT),
            },
            wgpu::Extent3d {
                width: SLOT,
                height: SLOT,
                depth_or_array_layers: 1,
            },
        );
    }
}

pub(crate) fn build_instances(
    pools: &[Vec<P>; 3],
    atlas: &ParticleAtlas,
    camera: mcr_render::glam::DVec3,
    partial: f32,
    lightmap: &[[f32; 4]],
    out: &mut [Vec<Instance>; 3],
) {
    for (pool, list) in pools.iter().enumerate() {
        let out = &mut out[pool];
        out.clear();
        for p in list.iter().take(POOL_CAP) {
            let size = sim::quad_size(p, partial);
            if size <= 0.0 || p.kind == Kind::Seed {
                continue;
            }
            let pos = p.prev + (p.pos - p.prev) * f64::from(partial) - camera;
            let (mut block, mut sky) = (f32::from(p.light.0), f32::from(p.light.1));
            match p.light_mode {
                Light::World => {}
                Light::Full => (block, sky) = (15.0, 15.0),
                Light::BlockMax => block = 15.0,
                Light::Emission => block += (15.0 - block) * sim::emission(p, partial),
            }
            let lm = lightmap
                .get((sky as usize).min(15) * 16 + (block as usize).min(15))
                .copied()
                .unwrap_or([1.0; 4]);
            let uv = if pool == Pool::Terrain as usize {
                Uv {
                    u0: p.rect.u0 + (p.rect.u1 - p.rect.u0) * p.sub[0] / 4.0,
                    v0: p.rect.v0 + (p.rect.v1 - p.rect.v0) * p.sub[1] / 4.0,
                    u1: p.rect.u0 + (p.rect.u1 - p.rect.u0) * (p.sub[0] + 1.0) / 4.0,
                    v1: p.rect.v0 + (p.rect.v1 - p.rect.v0) * (p.sub[1] + 1.0) / 4.0,
                }
            } else {
                atlas.uv(p.sprite)
            };
            out.push(Instance {
                pos_size: [pos.x as f32, pos.y as f32, pos.z as f32, size],
                uv: [uv.u0, uv.v0, uv.u1, uv.v1],
                color: [
                    p.color[0] * lm[0],
                    p.color[1] * lm[1],
                    p.color[2] * lm[2],
                    sim::alpha(p, partial),
                ],
                roll: [
                    p.prev_roll + (p.roll - p.prev_roll) * partial,
                    0.0,
                    0.0,
                    0.0,
                ],
            });
        }
    }
}

impl Gpu {
    pub(crate) fn record(
        &mut self,
        ctx: &mut OverlayCtx<'_>,
        pools: &[Vec<P>; 3],
        atlas: &ParticleAtlas,
        partial: f32,
        env: &Environment,
    ) {
        if self.lightmap_env.as_ref() != Some(env) {
            self.lightmap = env.lightmap(1.0);
            self.lightmap_env = Some(env.clone());
        }
        let cam = ctx.camera;
        let aspect = ctx.size.0 as f32 / ctx.size.1.max(1) as f32;
        let forward = cam.forward();
        let right = cam.right();
        let up = right.cross(forward);
        let uniforms = Uniforms {
            view_proj: cam.view_proj(aspect).to_cols_array(),
            right: [right.x, right.y, right.z, 0.0],
            up: [up.x, up.y, up.z, 0.0],
        };
        ctx.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut per_pool: [Vec<Instance>; 3] =
            [std::mem::take(&mut self.scratch), Vec::new(), Vec::new()];
        build_instances(
            pools,
            atlas,
            cam.position,
            partial,
            &self.lightmap,
            &mut per_pool,
        );
        let counts: [u32; 3] = std::array::from_fn(|i| per_pool[i].len() as u32);
        for (i, list) in per_pool.iter().enumerate() {
            if !list.is_empty() {
                ctx.queue
                    .write_buffer(&self.instances[i], 0, bytemuck::cast_slice(list));
            }
        }
        self.scratch = std::mem::take(&mut per_pool[0]);
        if counts.iter().all(|&c| c == 0) {
            return;
        }
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particles"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let draws = [
            (Pool::Opaque, &self.opaque, &self.particle_bg),
            (Pool::Translucent, &self.blend, &self.particle_bg),
            (Pool::Terrain, &self.opaque, &self.dynamic_bg),
        ];
        for (pool, pipeline, bg) in draws {
            let n = counts[pool as usize];
            if n == 0 {
                continue;
            }
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.set_vertex_buffer(0, self.instances[pool as usize].slice(..));
            pass.draw(0..4, 0..n);
        }
    }
}
