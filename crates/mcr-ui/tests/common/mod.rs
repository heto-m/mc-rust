#![allow(dead_code)]

use std::path::{Path, PathBuf};

use mcr_ui::{Ui, UiRenderer, vanilla_resources};

fn cache_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .map(|dir| dir.join(".cache/versions/26.3"))
        .find(|dir| dir.exists())
}

pub fn jar_path() -> Option<PathBuf> {
    std::env::var_os("MCR_CLIENT_JAR")
        .map(PathBuf::from)
        .or_else(|| cache_dir().map(|d| d.join("client.jar")))
        .filter(|p| p.exists())
}

pub fn asset_index() -> Option<(PathBuf, String)> {
    let dir = std::env::var_os("MCR_ASSETS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".minecraft/assets")))?;
    let index = std::env::var("MCR_ASSET_INDEX").unwrap_or_else(|_| "30".into());
    dir.join("indexes")
        .join(format!("{index}.json"))
        .exists()
        .then_some((dir, index))
}

pub fn ui_with_index(use_index: bool) -> Option<Ui> {
    let Some(jar) = jar_path() else {
        eprintln!("skipping: no client jar");
        return None;
    };
    let index = if use_index { asset_index() } else { None };
    let resources = vanilla_resources(&jar, index.as_ref().map(|(d, i)| (d.as_path(), i.as_str())))
        .expect("resources");
    Some(Ui::load(&resources).expect("ui"))
}

pub fn ui() -> Option<Ui> {
    ui_with_index(true)
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

pub fn gpu() -> Option<Gpu> {
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| eprintln!("skipping GPU test: {e}"))
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| eprintln!("skipping GPU test: {e}"))
            .ok()?;
        Some(Gpu { device, queue })
    })
}

pub fn render(
    gpu: &Gpu,
    renderer: &mut UiRenderer,
    ui: &mut Ui,
    (width, height): (u32, u32),
    clear: wgpu::Color,
) -> Vec<u8> {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    renderer.prepare(&gpu.device, &gpu.queue, ui);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.draw(&mut pass);
    }
    let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = buffer.slice(..).get_mapped_range().expect("mapped");
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * row) as usize;
        out.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    out
}

pub fn save_png(name: &str, width: u32, height: u32, rgba: &[u8]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-shots");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
    path
}

pub fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}
