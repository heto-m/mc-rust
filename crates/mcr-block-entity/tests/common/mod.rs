#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_block_entity::mcr_entity_model::glam::{DVec3, Quat, Vec3};
use mcr_block_entity::mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_block_entity::mcr_entity_model::wgpu;
use mcr_block_entity::mcr_entity_model::{
    EntityScene, EntityView, FrameParams, LightInfo, SceneAssets, TargetFormats,
};
use mcr_block_entity::{BlockEntityFrame, BlockEntityScene};
use mcr_ui::Ui;
use rustc_hash::FxHashMap;

pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub fn jar() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/versions/26.3/client.jar")
}

pub fn formats() -> TargetFormats {
    TargetFormats {
        color: COLOR,
        depth: DEPTH,
        reverse_z: true,
        sample_count: 1,
    }
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

/// Block state id for `name` with the listed properties set (others default).
pub fn state(name: &str, props: &[(&str, &str)]) -> u32 {
    let v = mcr_data::v26_3();
    let block = v
        .block_by_name(&format!("minecraft:{name}"))
        .unwrap_or_else(|| panic!("block {name}"));
    let info = v.block_info(block).unwrap();
    let (first, end) = info.states;
    (first..end)
        .filter(|&s| {
            props
                .iter()
                .all(|(k, val)| v.state_properties(s).any(|(pk, pv)| pk == *k && pv == *val))
        })
        .find(|&s| s == info.default_state)
        .or_else(|| {
            (first..end).find(|&s| {
                props
                    .iter()
                    .all(|(k, val)| v.state_properties(s).any(|(pk, pv)| pk == *k && pv == *val))
            })
        })
        .unwrap_or_else(|| {
            let have: Vec<_> = v.state_properties(first).collect();
            panic!("state {name} {props:?}; first state has {have:?}")
        })
}

pub struct Harness {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub scene: BlockEntityScene,
    pub decor: EntityScene,
    pub ui: Ui,
    pub tick: i64,
}

pub struct Shot {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Harness {
    pub fn new() -> Option<Self> {
        let jar = jar();
        if !jar.exists() {
            eprintln!("skipping: client jar missing");
            return None;
        }
        let extracted = jar.parent().unwrap().join("extracted");
        let resources = Arc::new(ResourceManager::vanilla(&jar).ok()?);
        let assets = Arc::new(SceneAssets::load(resources.clone(), &extracted).ok()?);
        let (device, queue) = gpu().or_else(|| {
            eprintln!("skipping: no gpu");
            None
        })?;
        let scene = BlockEntityScene::new(&device, &queue, formats(), assets.clone()).ok()?;
        let decor = EntityScene::new(&device, &queue, formats(), assets).ok()?;
        let ui = Ui::load(&resources).ok()?;
        Some(Harness {
            device,
            queue,
            scene,
            decor,
            ui,
            tick: 1000,
        })
    }

    /// Draws a terrain stand-in block (sign boards, beds) at `pos` through the entity scene.
    pub fn decor_blocks(&mut self, blocks: &[([i32; 3], u32)]) {
        let datas: Vec<FxHashMap<u8, EntityDataValue>> = blocks
            .iter()
            .map(|(_, s)| {
                let mut m = FxHashMap::default();
                m.insert(23, EntityDataValue::BlockState(*s as i32));
                m
            })
            .collect();
        let views = blocks.iter().zip(&datas).map(|((p, _), d)| {
            let mut v = EntityView::named("minecraft:block_display", d);
            v.pos = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
            v.light = [0, 15];
            v
        });
        self.decor.update(&(), views);
    }

    pub fn frame(&self, cam: DVec3, target: Vec3, fov: f32, w: u32, h: u32) -> BlockEntityFrame {
        let forward = (target - cam.as_vec3()).normalize();
        let view = mcr_block_entity::mcr_entity_model::glam::camera::rh::view::look_to_mat4(
            Vec3::ZERO,
            forward,
            Vec3::Y,
        );
        let proj = mcr_block_entity::mcr_entity_model::glam::camera::rh::proj::directx::perspective_infinite_reverse(fov.to_radians(), w as f32 / h as f32, 0.05);
        BlockEntityFrame {
            params: FrameParams {
                view_proj: proj * view,
                camera: cam.to_array(),
                light: LightInfo::default(),
            },
            game_time: self.tick,
            partial_ticks: 0.5,
            camera_rotation: Quat::from_mat4(&view.inverse()),
            player: None,
            render_distance_blocks: 128.0,
        }
    }

    pub fn advance(&mut self, ticks: i64, frame: &BlockEntityFrame) {
        for _ in 0..ticks {
            self.tick += 1;
            let mut f = *frame;
            f.game_time = self.tick;
            self.scene.update(&f, Some(&mut self.ui), &|_| [0, 15]);
        }
    }

    pub fn render(&mut self, frame: &BlockEntityFrame, w: u32, h: u32) -> Shot {
        let mut f = *frame;
        f.game_time = self.tick;
        self.scene.update(&f, Some(&mut self.ui), &|_| [0, 15]);
        let device = &self.device;
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
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
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.decor.prepare(&f.params);
        self.scene.prepare(&f);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let cv = target.create_view(&Default::default());
            let dv = depth.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cv,
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
                    view: &dv,
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
            self.decor.draw(&mut pass);
            self.scene.draw(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let mut pixels = Vec::new();
        for y in 0..h {
            let s = (y * row) as usize;
            pixels.extend_from_slice(&mapped[s..s + (w * 4) as usize]);
        }
        Shot {
            pixels,
            width: w,
            height: h,
        }
    }
}

impl Shot {
    pub fn save(&self, name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/block-entity-shots");
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
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                p[0].abs_diff(120) > 14 || p[1].abs_diff(166) > 14 || p[2].abs_diff(255) > 14
            })
            .count()
    }
}
