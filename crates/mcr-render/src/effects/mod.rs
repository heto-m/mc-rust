//! Sky, clouds, precipitation and block-destruction decals drawn inside the world pass.
//!
//! App integration:
//! - `Renderer::set_effect_assets(&EffectAssets::load(..))` once after the resources load.
//! - Every frame: `set_environment` (with [`crate::Environment::overworld`], `nether` or `end`),
//!   `set_effect_time(game_time, partial)`, `update_weather(..)` while raining, `set_destroy_decals(..)`.
//! - `set_cloud_settings` when the options change.

mod assets;
mod border;
mod clouds;
mod destroy;
mod random;
mod sky;
mod weather;

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

pub use assets::{EffectAssets, EffectImage};
pub use border::WorldBorderVisual;
pub use clouds::{CloudMode, CloudSettings};
pub use destroy::{DestroyDecal, MAX_BOXES_PER_DECAL, MAX_DECALS};
pub use weather::{MAX_WEATHER_RADIUS, MIN_WEATHER_RADIUS, Precipitation, WeatherSource};

use crate::environment::{Environment, Skybox};
use clouds::{CloudVertex, Clouds};
use sky::{FxVertex, SkyLayout};

const SLOT: u64 = 256;
const SLOTS: u64 = 12;
const MAX_WEATHER_QUADS: usize = 2 * 21 * 21;
const MAX_DESTROY_QUADS: usize = MAX_DECALS * MAX_BOXES_PER_DECAL * 6;

mod slot {
    pub const SUNRISE: usize = 0;
    pub const SUN: usize = 1;
    pub const MOON: usize = 2;
    pub const STARS: usize = 3;
    pub const END: usize = 4;
    pub const CLOUDS: usize = 5;
    pub const RAIN: usize = 6;
    pub const SNOW: usize = 7;
    pub const DESTROY: usize = 8;
    pub const BORDER: usize = 9;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FxUniform {
    model: [f32; 16],
    color: [f32; 4],
    extra: [f32; 4],
    misc: [f32; 4],
}

impl FxUniform {
    fn new(model: Mat4, color: [f32; 4]) -> Self {
        Self {
            model: model.to_cols_array(),
            color,
            extra: [0.0; 4],
            misc: [0.0; 4],
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Plan {
    sunrise: bool,
    sun: bool,
    moon: bool,
    stars: bool,
    end: bool,
    clouds: bool,
    rain: u32,
    snow: u32,
    destroy: u32,
    border: u32,
}

struct Textures {
    celestial: wgpu::TextureView,
    end_sky: wgpu::TextureView,
    rain: wgpu::TextureView,
    snow: wgpu::TextureView,
    destroy: wgpu::TextureView,
    forcefield: wgpu::TextureView,
}

struct Groups {
    sky: wgpu::BindGroup,
    end: wgpu::BindGroup,
    clouds: wgpu::BindGroup,
    rain: wgpu::BindGroup,
    snow: wgpu::BindGroup,
    destroy: wgpu::BindGroup,
    border: wgpu::BindGroup,
}

struct Pipelines {
    sunrise: wgpu::RenderPipeline,
    celestial: wgpu::RenderPipeline,
    stars: wgpu::RenderPipeline,
    end: wgpu::RenderPipeline,
    clouds_fancy: wgpu::RenderPipeline,
    clouds_flat: wgpu::RenderPipeline,
    weather: wgpu::RenderPipeline,
    destroy: wgpu::RenderPipeline,
    border: wgpu::RenderPipeline,
}

pub(crate) struct Effects {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bgl: wgpu::BindGroupLayout,
    pipelines: Pipelines,
    uniforms: wgpu::Buffer,
    sky_buffer: wgpu::Buffer,
    layout: SkyLayout,
    cloud_buffer: wgpu::Buffer,
    weather_buffer: wgpu::Buffer,
    destroy_buffer: wgpu::Buffer,
    border_buffer: wgpu::Buffer,
    dummy_storage: wgpu::Buffer,
    sampler_clamp: wgpu::Sampler,
    sampler_repeat: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
    dummy_texture: wgpu::TextureView,
    textures: Option<Textures>,
    groups: Option<Groups>,
    clouds: Clouds,
    cloud_settings: CloudSettings,
    cloud_uploaded: bool,
    weather: weather::Columns,
    decals: Vec<DestroyDecal>,
    border: Option<(WorldBorderVisual, f64)>,
    plan: Plan,
    time: (u64, f32),
}

fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sane_position(p: [f64; 3]) -> [f64; 3] {
    p.map(|c| {
        if c.is_finite() {
            c.clamp(-3.0e7, 3.0e7)
        } else {
            0.0
        }
    })
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn texture_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    images: &[&EffectImage],
) -> wgpu::TextureView {
    let (w, h) = (images[0].width, images[0].height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: images.len() as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in images.iter().enumerate() {
        let (iw, ih) = (image.width.min(w), image.height.min(h));
        let row = image.width as usize * 4;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..ih as usize {
            let n = iw as usize * 4;
            data[y * w as usize * 4..y * w as usize * 4 + n]
                .copy_from_slice(&image.rgba[y * row..y * row + n]);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &data,
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
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

impl Effects {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        globals_bgl: &wgpu::BindGroupLayout,
        globals_src: &str,
    ) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
                storage_entry(3),
                storage_entry(4),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("effects"),
            source: wgpu::ShaderSource::Wgsl(
                [globals_src, include_str!("../shaders/effects.wgsl")]
                    .concat()
                    .into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effects"),
            bind_group_layouts: &[Some(globals_bgl), Some(&bgl)],
            immediate_size: 0,
        });
        let overlay = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let crumble = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let make = |label: &str,
                    vs: &str,
                    fs: &str,
                    blend: wgpu::BlendState,
                    depth_test: bool,
                    cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(if depth_test {
                        wgpu::CompareFunction::GreaterEqual
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let alpha = wgpu::BlendState::ALPHA_BLENDING;
        let pipelines = Pipelines {
            sunrise: make("sunrise", "vs_tri", "fs_color", alpha, false, None),
            celestial: make("celestial", "vs_quad", "fs_textured", overlay, false, None),
            stars: make("stars", "vs_quad", "fs_color", overlay, false, None),
            end: make("end sky", "vs_quad", "fs_textured", alpha, false, None),
            clouds_fancy: make(
                "clouds",
                "vs_cloud",
                "fs_cloud",
                alpha,
                true,
                Some(wgpu::Face::Back),
            ),
            clouds_flat: make("flat clouds", "vs_cloud", "fs_cloud", alpha, true, None),
            weather: make("weather", "vs_quad", "fs_weather", alpha, true, None),
            destroy: make(
                "destroy",
                "vs_quad",
                "fs_destroy",
                crumble,
                true,
                Some(wgpu::Face::Back),
            ),
            border: make("world border", "vs_quad", "fs_border", overlay, true, None),
        };

        let (sky_vertices, sky_layout) = sky::build_static();
        let sky_buffer = storage_buffer(
            device,
            "sky vertices",
            (sky_vertices.len() * size_of::<FxVertex>()) as u64,
        );
        queue.write_buffer(&sky_buffer, 0, bytemuck::cast_slice(&sky_vertices));
        let sampler = |filter: wgpu::FilterMode, address: wgpu::AddressMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: address,
                address_mode_v: address,
                address_mode_w: address,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let dummy_image = EffectImage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        Self {
            queue: queue.clone(),
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("effect uniforms"),
                size: SLOT * SLOTS,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sky_buffer,
            layout: sky_layout,
            cloud_buffer: storage_buffer(
                device,
                "cloud vertices",
                (clouds::MAX_CLOUD_QUADS * 4 * size_of::<CloudVertex>()) as u64,
            ),
            weather_buffer: storage_buffer(
                device,
                "weather vertices",
                (MAX_WEATHER_QUADS * 4 * size_of::<FxVertex>()) as u64,
            ),
            destroy_buffer: storage_buffer(
                device,
                "destroy vertices",
                (MAX_DESTROY_QUADS * 4 * size_of::<FxVertex>()) as u64,
            ),
            border_buffer: storage_buffer(
                device,
                "border vertices",
                (border::MAX_BORDER_QUADS * 4 * size_of::<FxVertex>()) as u64,
            ),
            dummy_storage: storage_buffer(device, "effect dummy", 64),
            sampler_clamp: sampler(wgpu::FilterMode::Nearest, wgpu::AddressMode::ClampToEdge),
            sampler_repeat: sampler(wgpu::FilterMode::Nearest, wgpu::AddressMode::Repeat),
            sampler_linear: sampler(wgpu::FilterMode::Linear, wgpu::AddressMode::Repeat),
            dummy_texture: texture_array(device, queue, "effect dummy", &[&dummy_image]),
            device: device.clone(),
            bgl,
            pipelines,
            textures: None,
            groups: None,
            clouds: Clouds::default(),
            cloud_settings: CloudSettings::default(),
            cloud_uploaded: false,
            weather: weather::Columns {
                quads: Vec::new(),
                rain_quads: 0,
                snow_quads: 0,
            },
            decals: Vec::new(),
            border: None,
            plan: Plan::default(),
            time: (0, 0.0),
        }
    }

    pub(crate) fn set_assets(&mut self, assets: &EffectAssets) {
        let mut celestial: Vec<&EffectImage> = vec![&assets.sun];
        celestial.extend(assets.moons.iter());
        let destroy: Vec<&EffectImage> = assets.destroy.iter().collect();
        let (d, q) = (&self.device, &self.queue);
        let textures = Textures {
            celestial: texture_array(d, q, "celestial", &celestial),
            end_sky: texture_array(d, q, "end sky", &[&assets.end_sky]),
            rain: texture_array(d, q, "rain", &[&assets.rain]),
            snow: texture_array(d, q, "snow", &[&assets.snow]),
            destroy: texture_array(d, q, "destroy", &destroy),
            forcefield: texture_array(d, q, "forcefield", &[&assets.forcefield]),
        };
        let group = |tex: &wgpu::TextureView,
                     sampler: &wgpu::Sampler,
                     verts: &wgpu::Buffer,
                     cloud: &wgpu::Buffer| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("effects"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniforms,
                            offset: 0,
                            size: wgpu::BufferSize::new(size_of::<FxUniform>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(tex),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: verts.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: cloud.as_entire_binding(),
                    },
                ],
            })
        };
        let dummy = &self.dummy_storage;
        self.groups = Some(Groups {
            sky: group(
                &textures.celestial,
                &self.sampler_clamp,
                &self.sky_buffer,
                dummy,
            ),
            end: group(
                &textures.end_sky,
                &self.sampler_linear,
                &self.sky_buffer,
                dummy,
            ),
            clouds: group(
                &self.dummy_texture,
                &self.sampler_clamp,
                dummy,
                &self.cloud_buffer,
            ),
            rain: group(
                &textures.rain,
                &self.sampler_repeat,
                &self.weather_buffer,
                dummy,
            ),
            snow: group(
                &textures.snow,
                &self.sampler_repeat,
                &self.weather_buffer,
                dummy,
            ),
            destroy: group(
                &textures.destroy,
                &self.sampler_clamp,
                &self.destroy_buffer,
                dummy,
            ),
            border: group(
                &textures.forcefield,
                &self.sampler_repeat,
                &self.border_buffer,
                dummy,
            ),
        });
        self.textures = Some(textures);
        self.clouds.set_texture(&assets.clouds);
        self.cloud_uploaded = false;
    }

    pub(crate) fn set_cloud_settings(&mut self, settings: CloudSettings) {
        self.cloud_settings = settings;
    }

    pub(crate) fn set_time(&mut self, game_time: u64, partial: f32) {
        self.time = (
            game_time,
            if partial.is_finite() {
                partial.clamp(0.0, 1.0)
            } else {
                0.0
            },
        );
    }

    pub(crate) fn set_decals(&mut self, decals: &[DestroyDecal]) {
        self.decals.clear();
        self.decals.extend(
            decals
                .iter()
                .take(MAX_DECALS)
                .filter(|d| d.stage < 10)
                .cloned(),
        );
    }

    /// The border walls to draw within `distance` blocks of the camera, or `None` to hide them.
    pub(crate) fn set_border(&mut self, border: Option<WorldBorderVisual>, distance: f64) {
        self.border = border.map(|b| (b, distance));
    }

    pub(crate) fn update_weather(
        &mut self,
        env: &Environment,
        camera: [f64; 3],
        radius: u32,
        src: &dyn WeatherSource,
    ) {
        let rain = unit(env.rain_level);
        if rain <= 0.0 {
            self.weather.rain_quads = 0;
            self.weather.snow_quads = 0;
            return;
        }
        let camera = sane_position(camera);
        let lightmap = env.lightmap(1.0);
        let mut columns = weather::build(
            &weather::WeatherInput {
                camera,
                game_time: self.time.0,
                partial: self.time.1,
                radius,
                intensity: rain,
                lightmap: &lightmap,
            },
            src,
        );
        columns.quads.truncate(MAX_WEATHER_QUADS * 4);
        let fit = (columns.quads.len() / 4) as u32;
        columns.rain_quads = columns.rain_quads.min(fit);
        columns.snow_quads = columns.snow_quads.min(fit - columns.rain_quads);
        self.queue.write_buffer(
            &self.weather_buffer,
            0,
            bytemuck::cast_slice(&columns.quads),
        );
        self.weather = columns;
    }

    pub(crate) fn prepare(&mut self, env: &Environment, camera: [f64; 3]) {
        self.plan = Plan::default();
        let camera = sane_position(camera);
        let rain_level = unit(env.rain_level);
        if self.groups.is_none() {
            return;
        }
        let mut uniforms = [FxUniform::zeroed(); SLOTS as usize];
        let base = Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        match env.skybox {
            Skybox::Overworld
                if [env.sun_angle, env.moon_angle, env.star_angle]
                    .iter()
                    .all(|a| a.is_finite()) =>
            {
                let rain_brightness = 1.0 - rain_level;
                let sunrise_alpha = env.sunrise_color[3];
                if sunrise_alpha > 0.001 {
                    let flip = if env.sun_angle.sin() < 0.0 {
                        180.0f32
                    } else {
                        0.0
                    };
                    let model = Mat4::from_rotation_x(90f32.to_radians())
                        * Mat4::from_rotation_z((flip + 90.0).to_radians())
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, sunrise_alpha));
                    uniforms[slot::SUNRISE] = FxUniform::new(model, env.sunrise_color);
                    self.plan.sunrise = true;
                }
                let body = |angle: f32, scale: f32| {
                    base * Mat4::from_rotation_x(angle)
                        * Mat4::from_translation(Vec3::new(0.0, 100.0, 0.0))
                        * Mat4::from_scale(Vec3::new(scale, 1.0, scale))
                };
                let white = [1.0, 1.0, 1.0, rain_brightness];
                uniforms[slot::SUN] = FxUniform::new(body(env.sun_angle, 30.0), white);
                uniforms[slot::MOON] = FxUniform::new(body(env.moon_angle, 20.0), white);
                uniforms[slot::MOON].misc[0] = f32::from(env.moon_phase % 8) + 1.0;
                self.plan.sun = true;
                self.plan.moon = true;
                if env.star_brightness > 0.0 {
                    let b = env.star_brightness;
                    uniforms[slot::STARS] =
                        FxUniform::new(base * Mat4::from_rotation_x(env.star_angle), [b; 4]);
                    self.plan.stars = true;
                }
            }
            Skybox::End => {
                uniforms[slot::END] = FxUniform::new(Mat4::IDENTITY, [1.0; 4]);
                self.plan.end = true;
            }
            _ => {}
        }

        if let Some(height) = env.cloud_height.filter(|h| h.is_finite()) {
            let moved = self.clouds.update(
                self.cloud_settings,
                height,
                camera,
                self.time.0,
                self.time.1,
            );
            if moved || !self.cloud_uploaded {
                let n = self.clouds.vertices.len().min(clouds::MAX_CLOUD_QUADS * 4);
                self.queue.write_buffer(
                    &self.cloud_buffer,
                    0,
                    bytemuck::cast_slice(&self.clouds.vertices[..n]),
                );
                self.cloud_uploaded = true;
            }
            if self.clouds.quads > 0 && env.cloud_color[3] > 0.0 {
                let mut u = FxUniform::new(Mat4::IDENTITY, env.cloud_color);
                let fog_end = (self.cloud_settings.range_chunks.clamp(2, 128) * 16) as f32;
                u.extra = [
                    self.clouds.offset[0],
                    self.clouds.offset[1],
                    self.clouds.offset[2],
                    fog_end,
                ];
                uniforms[slot::CLOUDS] = u;
                self.plan.clouds = true;
            }
        }

        if rain_level > 0.0 {
            self.plan.rain = self.weather.rain_quads;
            self.plan.snow = self.weather.snow_quads;
            uniforms[slot::RAIN] = FxUniform::new(Mat4::IDENTITY, [1.0; 4]);
            uniforms[slot::SNOW] = FxUniform::new(Mat4::IDENTITY, [1.0; 4]);
        }

        if !self.decals.is_empty() {
            let mut verts = destroy::build(&self.decals, camera);
            verts.truncate(MAX_DESTROY_QUADS * 4);
            self.queue
                .write_buffer(&self.destroy_buffer, 0, bytemuck::cast_slice(&verts));
            self.plan.destroy = (verts.len() / 4) as u32;
            uniforms[slot::DESTROY] = FxUniform::new(Mat4::IDENTITY, [1.0; 4]);
        }
        if let Some((b, distance)) = self.border.filter(|(b, d)| b.visible_from(camera, *d)) {
            let verts = border::build(&b, camera, distance);
            self.queue
                .write_buffer(&self.border_buffer, 0, bytemuck::cast_slice(&verts));
            self.plan.border = (verts.len() / 4) as u32;
            uniforms[slot::BORDER] = FxUniform::new(Mat4::IDENTITY, [1.0; 4]);
        }
        let mut bytes = vec![0u8; (SLOT * SLOTS) as usize];
        for (i, u) in uniforms.iter().enumerate() {
            let at = i * SLOT as usize;
            bytes[at..at + size_of::<FxUniform>()].copy_from_slice(bytemuck::bytes_of(u));
        }
        self.queue.write_buffer(&self.uniforms, 0, &bytes);
    }

    pub(crate) fn draw_sky(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (Some(g), p) = (&self.groups, &self.plan) else {
            return;
        };
        let at = |s: usize| [(s as u64 * SLOT) as u32];
        let l = &self.layout;
        if p.end {
            pass.set_pipeline(&self.pipelines.end);
            pass.set_bind_group(1, &g.end, &at(slot::END));
            pass.draw(l.end_quad * 6..(l.end_quad + 6) * 6, 0..1);
        }
        if p.sunrise {
            pass.set_pipeline(&self.pipelines.sunrise);
            pass.set_bind_group(1, &g.sky, &at(slot::SUNRISE));
            pass.draw(0..l.sunrise_vertices, 0..1);
        }
        if p.sun || p.moon {
            pass.set_pipeline(&self.pipelines.celestial);
        }
        if p.sun {
            pass.set_bind_group(1, &g.sky, &at(slot::SUN));
            pass.draw(l.sun_quad * 6..(l.sun_quad + 1) * 6, 0..1);
        }
        if p.moon {
            pass.set_bind_group(1, &g.sky, &at(slot::MOON));
            pass.draw(l.moon_quad * 6..(l.moon_quad + 1) * 6, 0..1);
        }
        if p.stars {
            pass.set_pipeline(&self.pipelines.stars);
            pass.set_bind_group(1, &g.sky, &at(slot::STARS));
            pass.draw(l.star_quad * 6..(l.star_quad + l.star_quads) * 6, 0..1);
        }
    }

    pub(crate) fn draw_world(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (Some(g), p) = (&self.groups, &self.plan) else {
            return;
        };
        let at = |s: usize| [(s as u64 * SLOT) as u32];
        if p.destroy > 0 {
            pass.set_pipeline(&self.pipelines.destroy);
            pass.set_bind_group(1, &g.destroy, &at(slot::DESTROY));
            pass.draw(0..p.destroy * 6, 0..1);
        }
        if p.border > 0 {
            pass.set_pipeline(&self.pipelines.border);
            pass.set_bind_group(1, &g.border, &at(slot::BORDER));
            pass.draw(0..p.border * 6, 0..1);
        }
        if p.rain + p.snow > 0 {
            pass.set_pipeline(&self.pipelines.weather);
            if p.rain > 0 {
                pass.set_bind_group(1, &g.rain, &at(slot::RAIN));
                pass.draw(0..p.rain * 6, 0..1);
            }
            if p.snow > 0 {
                pass.set_bind_group(1, &g.snow, &at(slot::SNOW));
                pass.draw(p.rain * 6..(p.rain + p.snow) * 6, 0..1);
            }
        }
        if p.clouds {
            pass.set_pipeline(if self.cloud_settings.mode == CloudMode::Fancy {
                &self.pipelines.clouds_fancy
            } else {
                &self.pipelines.clouds_flat
            });
            pass.set_bind_group(1, &g.clouds, &at(slot::CLOUDS));
            pass.draw(0..self.clouds.quads * 6, 0..1);
        }
    }
}
