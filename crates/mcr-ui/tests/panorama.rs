mod common;

use mcr_ui::{Panorama, PanoramaTarget, vanilla_resources};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const RUNS: usize = 64;

fn real_panorama() -> Option<Panorama> {
    let jar = common::jar_path()?;
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|dir| dir.join(".cache/assets"))
        .find(|dir| dir.exists())?;
    let index = std::env::var("MCR_ASSET_INDEX").unwrap_or_else(|_| "34".into());
    let resources = vanilla_resources(&jar, Some((assets.as_path(), index.as_str()))).ok()?;
    Panorama::load(&resources)
}

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .ok()?;
        let wanted =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let required_features = adapter.features() & wanted;
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                ..Default::default()
            })
            .await
            .ok()
    })
}

fn target(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: (u32, u32),
) -> Vec<u8> {
    let row = (size.0 * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row) * u64::from(size.1),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size.1),
            },
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buffer.slice(..).get_mapped_range().expect("mapped");
    data.chunks(row as usize)
        .flat_map(|r| &r[..size.0 as usize * 4])
        .copied()
        .collect()
}

#[test]
fn the_cube_fills_the_target_and_turns_with_the_spin() {
    let Some(mut panorama) = real_panorama() else {
        eprintln!("skipping: no client jar or asset index with the panorama");
        return;
    };
    let Some((device, queue)) = device() else {
        eprintln!("skipping: no GPU");
        return;
    };
    let size = (320, 180);
    let render = |panorama: &mut Panorama, spin: f32| {
        let texture = target(&device, size);
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let target = PanoramaTarget {
            view: &view,
            format: FORMAT,
            size,
        };
        panorama.record(&device, &queue, &mut encoder, target, spin);
        queue.submit([encoder.finish()]);
        read(&device, &queue, &texture, size)
    };
    let still = render(&mut panorama, 0.0);
    assert!(still.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    assert_ne!(still, render(&mut panorama, 20.0));
    assert_eq!(still, render(&mut panorama, 0.0));
}

#[test]
fn gpu_cost_per_resolution() {
    let Some(mut panorama) = real_panorama() else {
        return;
    };
    let Some((device, queue)) = device() else {
        return;
    };
    if !device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
    {
        eprintln!("no timestamp queries");
        return;
    }
    let period = f64::from(queue.get_timestamp_period());
    let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("panorama"),
        ty: wgpu::QueryType::Timestamp,
        count: 2,
    });
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("resolve"),
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for size in [(1280, 720), (1920, 1080), (3840, 2160)] {
        let texture = target(&device, size);
        let view = texture.create_view(&Default::default());
        let mut micros = Vec::new();
        for run in 0..RUNS {
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.write_timestamp(&queries, 0);
            let target = PanoramaTarget {
                view: &view,
                format: FORMAT,
                size,
            };
            panorama.record(&device, &queue, &mut encoder, target, run as f32);
            encoder.write_timestamp(&queries, 1);
            encoder.resolve_query_set(&queries, 0..2, &resolve, 0);
            encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
            queue.submit([encoder.finish()]);
            readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            {
                let data = readback.slice(..).get_mapped_range().expect("mapped");
                let t: &[u64] = bytemuck::cast_slice(&data);
                micros.push(t[1].saturating_sub(t[0]) as f64 * period / 1000.0);
            }
            readback.unmap();
        }
        micros.sort_by(f64::total_cmp);
        eprintln!(
            "panorama {}x{}: median {:.1} us over {RUNS} passes (min {:.1}, max {:.1})",
            size.0,
            size.1,
            micros[RUNS / 2],
            micros[0],
            micros[RUNS - 1]
        );
    }
}
