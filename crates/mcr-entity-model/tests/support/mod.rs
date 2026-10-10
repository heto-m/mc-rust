#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_entity_model::glam::{DVec3, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityScene, FrameParams, LightInfo, Registries, SceneAssets, TargetFormats,
};
use rustc_hash::FxHashMap;

pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub fn cache() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/versions/26.3")
}

pub fn assets() -> Option<Arc<SceneAssets>> {
    let dir = cache();
    let jar = dir.join("client.jar");
    let extracted = dir.join("extracted");
    if !jar.exists() || !extracted.join("entity_models.json").exists() {
        eprintln!("skipping: client jar or extracted data missing");
        return None;
    }
    let resources = Arc::new(ResourceManager::vanilla(&jar).expect("jar"));
    Some(Arc::new(
        SceneAssets::load(resources, &extracted).expect("assets"),
    ))
}

pub fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Names of the synced registries the tests pretend a server sent.
#[derive(Default)]
pub struct TestRegistries {
    pub entries: FxHashMap<(String, i32), String>,
}

impl TestRegistries {
    pub fn set(&mut self, registry: &str, id: i32, name: &str) -> &mut Self {
        self.entries
            .insert((registry.to_owned(), id), name.to_owned());
        self
    }

    /// The vanilla variant registries in datagen (alphabetical) order.
    pub fn vanilla() -> Self {
        let mut r = TestRegistries::default();
        let lists: &[(&str, &[&str])] = &[
            (
                "minecraft:cat_variant",
                &[
                    "all_black",
                    "black",
                    "british_shorthair",
                    "calico",
                    "jellie",
                    "persian",
                    "ragdoll",
                    "red",
                    "siamese",
                    "tabby",
                    "white",
                ],
            ),
            (
                "minecraft:wolf_variant",
                &[
                    "ashen", "black", "chestnut", "pale", "rusty", "snowy", "spotted", "striped",
                    "woods",
                ],
            ),
            ("minecraft:frog_variant", &["cold", "temperate", "warm"]),
            ("minecraft:pig_variant", &["cold", "temperate", "warm"]),
            ("minecraft:cow_variant", &["cold", "temperate", "warm"]),
            ("minecraft:chicken_variant", &["cold", "temperate", "warm"]),
            (
                "minecraft:trim_material",
                &[
                    "amethyst",
                    "copper",
                    "diamond",
                    "emerald",
                    "gold",
                    "iron",
                    "lapis",
                    "netherite",
                    "quartz",
                    "redstone",
                    "resin",
                ],
            ),
            (
                "minecraft:trim_pattern",
                &[
                    "bolt",
                    "coast",
                    "dune",
                    "eye",
                    "flow",
                    "host",
                    "raiser",
                    "rib",
                    "sentry",
                    "shaper",
                    "silence",
                    "snout",
                    "spire",
                    "tide",
                    "vex",
                    "ward",
                    "wayfinder",
                    "wild",
                ],
            ),
        ];
        for (registry, names) in lists {
            for (i, n) in names.iter().enumerate() {
                r.set(registry, i as i32, &format!("minecraft:{n}"));
            }
        }
        r
    }
}

impl Registries for TestRegistries {
    fn entry_name(&self, registry: &str, id: i32) -> Option<&str> {
        self.entries
            .get(&(registry.to_owned(), id))
            .map(String::as_str)
    }
}

#[derive(Default)]
pub struct Data(pub FxHashMap<u8, EntityDataValue>);

impl Data {
    pub fn with(mut self, index: u8, value: EntityDataValue) -> Self {
        self.0.insert(index, value);
        self
    }
}

pub fn item(name: &str) -> ItemStack {
    let id = mcr_data::v26_3()
        .item_by_name(name)
        .unwrap_or_else(|| panic!("item {name}"));
    ItemStack::new(id as i32, 1)
}

pub struct Shot {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct Camera {
    pub position: DVec3,
    pub target: Vec3,
    pub fov_deg: f32,
}

impl Camera {
    pub fn frame(&self, width: u32, height: u32) -> FrameParams {
        let forward = (self.target - self.position.as_vec3()).normalize();
        let view =
            mcr_entity_model::glam::camera::rh::view::look_to_mat4(Vec3::ZERO, forward, Vec3::Y);
        let proj = mcr_entity_model::glam::camera::rh::proj::directx::perspective_infinite_reverse(
            self.fov_deg.to_radians(),
            width as f32 / height as f32,
            0.05,
        );
        FrameParams {
            view_proj: proj * view,
            camera: self.position.to_array(),
            light: LightInfo::default(),
        }
    }
}

pub struct Harness {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub scene: EntityScene,
}

impl Harness {
    pub fn new() -> Option<Self> {
        let assets = assets()?;
        let (device, queue) = gpu().or_else(|| {
            eprintln!("skipping GPU test: no adapter");
            None
        })?;
        let scene = EntityScene::new(
            &device,
            &queue,
            TargetFormats {
                color: COLOR,
                depth: DEPTH,
                reverse_z: true,
                sample_count: 1,
            },
            assets,
        )
        .expect("scene");
        Some(Harness {
            device,
            queue,
            scene,
        })
    }

    pub fn render(&mut self, frame: &FrameParams, width: u32, height: u32) -> Shot {
        let device = &self.device;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let color_view = target.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.47,
                            g: 0.65,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.scene.render(&mut pass, frame);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let mut pixels = Vec::new();
        for y in 0..height {
            let start = (y * row) as usize;
            pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
        }
        Shot {
            pixels,
            width,
            height,
        }
    }
}

impl Shot {
    pub fn save(&self, name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/entity-shots");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.png"));
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&self.pixels)
            .unwrap();
        path
    }

    pub fn covered(&self) -> usize {
        let sky = [120u8, 166, 255];
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                p[0].abs_diff(sky[0]) > 14
                    || p[1].abs_diff(sky[1]) > 14
                    || p[2].abs_diff(sky[2]) > 14
            })
            .count()
    }
}
