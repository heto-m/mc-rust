use std::path::PathBuf;

use mcr_entity_model::glam::{Mat4, Vec3};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    AnimState, AnimationSet, BoundAnimation, EntityRenderer, Instance, LayerSet, Model, Rig,
    TextureArrayBuilder, evaluate, load_jar_texture,
};

fn cache() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/versions/26.3")
}

fn layers() -> Option<LayerSet> {
    let path = cache().join("extracted/entity_models.json");
    if !path.exists() {
        eprintln!(
            "skipping: {} missing (run tools/extractor/run-client.sh)",
            path.display()
        );
        return None;
    }
    Some(LayerSet::load(path).unwrap())
}

fn model(set: &LayerSet, name: &str) -> Model {
    Model::from_layer(set.get(name).unwrap_or_else(|| panic!("layer {name}")))
}

#[test]
fn pig_has_head_body_and_four_legs() {
    let Some(set) = layers() else { return };
    let pig = model(&set, "minecraft:pig#main");
    for part in [
        "head",
        "body",
        "right_hind_leg",
        "left_hind_leg",
        "right_front_leg",
        "left_front_leg",
    ] {
        assert!(pig.bone(part).is_some(), "{part}");
    }
    assert_eq!(pig.bones.len(), 7);
    assert_eq!(pig.cube_count(), 7);
    assert!(matches!(Rig::detect(&pig), Rig::Quadruped(_)));
}

#[test]
fn zombie_is_a_biped_with_hat_overlay() {
    let Some(set) = layers() else { return };
    let zombie = model(&set, "minecraft:zombie#main");
    assert!(zombie.bone("hat").is_some());
    assert_eq!(zombie.cube_count(), 7);
    assert!(matches!(Rig::detect(&zombie), Rig::Biped(_)));
}

#[test]
fn every_layer_builds_with_valid_vertices() {
    let Some(set) = layers() else { return };
    assert!(set.0.len() > 300);
    for (name, def) in &set.0 {
        let m = Model::from_layer(def);
        assert_eq!(m.vertices.len() % 4, 0, "{name}");
        assert!(m.vertices.iter().all(|v| (v.bone as usize) < m.bones.len()));
        assert!(
            m.vertices
                .iter()
                .all(|v| v.pos.iter().all(|c| c.is_finite()))
        );
    }
}

#[test]
fn bat_resting_animation_samples_its_keyframe() {
    let path = cache().join("extracted/animations.json");
    if !path.exists() {
        return;
    }
    let anims = AnimationSet::load(path).unwrap();
    assert!(anims.0.len() > 50);
    let Some(set) = layers() else { return };
    let bat = model(&set, "minecraft:bat#main");
    let anim = BoundAnimation::bind(anims.get("BatAnimation.BAT_RESTING").unwrap(), &bat);
    let mut pose = bat.rest_pose();
    anim.apply(0.0, 1.0, &mut pose);
    let head = bat.bone("head").unwrap() as usize;
    assert!((pose[head].rot.x - std::f32::consts::PI).abs() < 1e-5);
}

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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

const W: u32 = 1200;
const H: u32 = 400;

fn entity_transform(x: f32, yaw_deg: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, 0.0, 0.0))
        * Mat4::from_rotation_y((180.0 - yaw_deg).to_radians())
        * Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        * Mat4::from_translation(Vec3::new(0.0, -1.501, 0.0))
        * Mat4::from_scale(Vec3::splat(1.0 / 16.0))
}

#[test]
fn renders_row_of_mobs() {
    let Some(set) = layers() else { return };
    let jar = cache().join("client.jar");
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping GPU test: no adapter");
        return;
    };

    let mobs = [
        (
            "minecraft:pig#main",
            "textures/entity/pig/pig_temperate.png",
        ),
        ("minecraft:zombie#main", "textures/entity/zombie/zombie.png"),
        (
            "minecraft:creeper#main",
            "textures/entity/creeper/creeper.png",
        ),
        (
            "minecraft:cow#main",
            "textures/entity/cow/cow_temperate.png",
        ),
        (
            "minecraft:villager#main",
            "textures/entity/villager/villager.png",
        ),
        (
            "minecraft:armor_stand#main",
            "textures/entity/armorstand/armorstand.png",
        ),
    ];
    let color = wgpu::TextureFormat::Rgba8UnormSrgb;
    let depth = wgpu::TextureFormat::Depth32Float;
    let mut textures = TextureArrayBuilder::default();
    let layers_for: Vec<_> = mobs
        .iter()
        .map(|(_, tex)| textures.add(load_jar_texture(&jar, tex).unwrap()))
        .collect();
    let mut renderer = EntityRenderer::new(&device, &queue, color, depth, false, &textures);

    let models: Vec<Model> = mobs.iter().map(|(l, _)| model(&set, l)).collect();
    let handles: Vec<_> = models
        .iter()
        .map(|m| renderer.add_model(&device, m))
        .collect();
    let mut bone_mats = Vec::new();
    for (i, m) in models.iter().enumerate() {
        let mut pose = m.rest_pose();
        Rig::detect(m).apply(
            &AnimState {
                walk_pos: 2.0 + i as f32,
                walk_speed: 0.7,
                head_yaw_deg: 20.0,
                head_pitch_deg: 10.0,
                age_ticks: 5.0,
            },
            &mut pose,
        );
        let mut world = vec![Mat4::IDENTITY; m.bones.len()];
        evaluate(m, &pose, &mut world);
        bone_mats.push(world);
    }
    let instances: Vec<[Instance; 1]> = (0..mobs.len())
        .map(|i| {
            [Instance {
                transform: entity_transform((i as f32 - 2.5) * 1.6, 25.0),
                bones: &bone_mats[i],
                texture: layers_for[i],
            }]
        })
        .collect();
    let batches: Vec<_> = handles
        .iter()
        .zip(&instances)
        .map(|(h, inst)| (*h, inst.as_slice()))
        .collect();

    let view = mcr_entity_model::glam::camera::rh::view::look_at_mat4(
        Vec3::new(0.0, 1.6, 7.5),
        Vec3::new(0.0, 0.9, 0.0),
        Vec3::Y,
    );
    let proj = mcr_entity_model::glam::camera::rh::proj::directx::perspective(
        40f32.to_radians(),
        W as f32 / H as f32,
        0.1,
        50.0,
    );
    renderer.prepare(&device, &queue, proj * view, &batches);

    let extent = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: color,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: depth,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let row = (W * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * H),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let color_view = target.create_view(&Default::default());
        let depth_view = depth_tex.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.45,
                        g: 0.65,
                        b: 0.9,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(H),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let mut pixels = Vec::new();
    for y in 0..H {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&mapped[start..start + (W * 4) as usize]);
    }

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join("entity_models.png")).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();

    let sky = [115u8, 166, 230];
    let covered = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            p[0].abs_diff(sky[0]) > 12 || p[1].abs_diff(sky[1]) > 12 || p[2].abs_diff(sky[2]) > 12
        })
        .count();
    assert!(covered > 5000, "only {covered} pixels drawn");
}
