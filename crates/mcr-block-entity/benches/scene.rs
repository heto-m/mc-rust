use std::path::PathBuf;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use mcr_assets::ResourceManager;
use mcr_block_entity::mcr_entity_model::glam::{Quat, Vec3};
use mcr_block_entity::mcr_entity_model::{
    FrameParams, LightInfo, SceneAssets, TargetFormats, wgpu,
};
use mcr_block_entity::{BlockEntityData, BlockEntityFrame, BlockEntityKind, BlockEntityScene};

fn setup() -> Option<(wgpu::Device, wgpu::Queue, Arc<SceneAssets>)> {
    let jar =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/versions/26.3/client.jar");
    if !jar.exists() {
        return None;
    }
    let resources = Arc::new(ResourceManager::vanilla(&jar).ok()?);
    let assets = Arc::new(SceneAssets::load(resources, &jar.parent()?.join("extracted")).ok()?);
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some((device, queue, assets))
}

fn state(name: &str) -> u32 {
    let v = mcr_data::v26_3();
    let b = v.block_by_name(name).unwrap();
    v.block_info(b).unwrap().default_state
}

fn bench(c: &mut Criterion) {
    let Some((device, queue, assets)) = setup() else {
        eprintln!("skipping: assets or gpu missing");
        return;
    };
    let formats = TargetFormats {
        color: wgpu::TextureFormat::Rgba8Unorm,
        depth: wgpu::TextureFormat::Depth32Float,
        reverse_z: true,
        sample_count: 1,
    };
    let mut scene = BlockEntityScene::new(&device, &queue, formats, assets).unwrap();
    let kinds = [
        ("minecraft:chest", BlockEntityKind::Chest),
        ("minecraft:ender_chest", BlockEntityKind::EnderChest),
        ("minecraft:white_shulker_box", BlockEntityKind::ShulkerBox),
        ("minecraft:bell", BlockEntityKind::Bell),
        ("minecraft:white_banner", BlockEntityKind::Banner),
    ];
    for i in 0..5000i32 {
        let (name, kind) = kinds[(i % 5) as usize];
        scene.set(
            [i % 70 - 35, 0, i / 70 - 35],
            state(name),
            kind,
            BlockEntityData::None,
        );
    }
    let proj = mcr_block_entity::mcr_entity_model::glam::camera::rh::proj::directx::perspective_infinite_reverse(
        1.2, 1.78, 0.05,
    );
    let view = mcr_block_entity::mcr_entity_model::glam::camera::rh::view::look_to_mat4(
        Vec3::ZERO,
        Vec3::new(0.0, -0.5, -1.0).normalize(),
        Vec3::Y,
    );
    let frame = BlockEntityFrame {
        params: FrameParams {
            view_proj: proj * view,
            camera: [0.0, 30.0, 40.0],
            light: LightInfo::default(),
        },
        game_time: 100,
        partial_ticks: 0.5,
        camera_rotation: Quat::IDENTITY,
        player: None,
        render_distance_blocks: 128.0,
    };
    c.bench_function("update_prepare_5000", |b| {
        b.iter(|| {
            scene.update(&frame, None, &|_| [0, 15]);
            scene.prepare(&frame);
        })
    });
    eprintln!("{:?}", scene.stats());
}

criterion_group!(benches, bench);
criterion_main!(benches);
