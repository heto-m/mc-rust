use std::collections::HashSet;

use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{
    Camera, CubeQuad, Face, FrameInputs, LIGHT_FULL_SKY, ModelQuad, ModelQuadDesc, PackedQuad,
    RenderError, Renderer, RendererConfig, SectionMesh, Target,
};
use wgpu::util::DeviceExt;

fn renderer_with(w: u32, h: u32, config: RendererConfig) -> Option<Renderer> {
    match Renderer::new(
        Target::Offscreen {
            width: w,
            height: h,
        },
        config,
    ) {
        Ok(r) => Some(r),
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            None
        }
        Err(e) => panic!("renderer creation failed: {e}"),
    }
}

fn renderer(w: u32, h: u32) -> Option<Renderer> {
    renderer_with(w, h, RendererConfig::default())
}

fn frame(r: &mut Renderer, cam: &Camera) {
    r.render(&FrameInputs {
        camera: cam,
        sky_light: 1.0,
    })
    .expect("render");
}

fn upload_world(r: &mut Renderer, radius: i32, shift: [i32; 3]) {
    for (k, mesh) in generate_world(&Terrain { seed: 11 }, radius, 4) {
        r.set_section([k[0] + shift[0], k[1] + shift[1], k[2] + shift[2]], &mesh)
            .expect("section fits");
    }
}

fn terrain_camera(offset: DVec3) -> Camera {
    Camera {
        position: DVec3::new(8.5, 118.0, 8.5) + offset,
        yaw: 0.6,
        pitch: -0.45,
        ..Camera::default()
    }
}

fn save_png(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(name)).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

#[test]
fn shader_decoder_matches_rust_encoder() {
    let Some(r) = renderer(8, 8) else { return };
    let (device, queue) = (r.device(), r.queue());

    let cubes: Vec<PackedQuad> = [
        (0, 0, 0, Face::PosX, 1, 1, 0, [0, 0, 0, 0], [0; 4], 0),
        (
            31,
            31,
            31,
            Face::NegZ,
            16,
            16,
            0xffff,
            [4, 3, 2, 1],
            [0xffff, 0xf000, 0x00f0, 0x1234],
            3,
        ),
        (
            15,
            7,
            3,
            Face::PosY,
            9,
            4,
            1234,
            [1, 3, 0, 2],
            [0x5a5a, 0xa5a5, 0x0102, 0x0408],
            1,
        ),
    ]
    .into_iter()
    .map(
        |(x, y, z, face, width, height, layer, ao, light, rotation)| {
            PackedQuad::encode(&CubeQuad {
                x,
                y,
                z,
                face,
                width,
                height,
                layer,
                ao,
                light,
                tint: 5,
                rotation,
                opaque: false,
            })
            .unwrap()
        },
    )
    .collect();
    let model_desc = ModelQuadDesc {
        corners: [
            [14.0, 3.0, 3.5],
            [15.0078125, 5.5, 5.9921875],
            [14.0, 3.0, 3.0],
            [15.0, 5.0, 5.0],
        ],
        uvs: [[0.0, 0.0], [15.9375, 0.0], [0.0, 1.5], [8.0, 15.9375]],
        layer: 65535,
        light: [0x3c3c, 0xf000, 0x00f0, 0x0102],
        shade_face: Face::NegZ,
        flags: 5,
        ao: [63, 32, 1, 0],
        tint: 9,
        cell: [15, 3, 4],
    };
    let models = [ModelQuad::encode(&model_desc).unwrap()];

    let init = |label, contents: &[u8], usage| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage,
        })
    };
    let cube_in = init(
        "cube in",
        bytemuck::cast_slice(&cubes),
        wgpu::BufferUsages::STORAGE,
    );
    let model_in = init(
        "model in",
        bytemuck::cast_slice(&models),
        wgpu::BufferUsages::STORAGE,
    );
    let out_usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
    let cube_out = init("cube out", &vec![0u8; cubes.len() * 80], out_usage);
    let model_out = init("model out", &vec![0u8; models.len() * 160], out_usage);

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("decode test"),
        source: wgpu::ShaderSource::Wgsl(
            [mcr_render::QUAD_WGSL, mcr_render::DECODE_TEST_WGSL]
                .concat()
                .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("decode test"),
        layout: None,
        module: &module,
        entry_point: Some("decode"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: cube_in.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: model_in.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: cube_out.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: model_out.as_entire_binding(),
            },
        ],
    });
    let read = |size: u64| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let (cube_read, model_read) = (read(cubes.len() as u64 * 80), read(160));
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(cubes.len() as u32, 1, 1);
    }
    enc.copy_buffer_to_buffer(&cube_out, 0, &cube_read, 0, cube_read.size());
    enc.copy_buffer_to_buffer(&model_out, 0, &model_read, 0, 160);
    queue.submit([enc.finish()]);
    cube_read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    model_read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    let got: Vec<u32> =
        bytemuck::cast_slice(&cube_read.slice(..).get_mapped_range().unwrap()).to_vec();
    for (i, q) in cubes.iter().enumerate() {
        let d = q.decode();
        let expect = [
            d.x.into(),
            d.y.into(),
            d.z.into(),
            d.face as u32,
            d.width.into(),
            d.height.into(),
            d.layer.into(),
            d.ao[0].into(),
            d.ao[1].into(),
            d.ao[2].into(),
            d.ao[3].into(),
            d.light[0].into(),
            d.light[1].into(),
            d.light[2].into(),
            d.light[3].into(),
            d.tint.into(),
            d.rotation.into(),
        ];
        assert_eq!(got[i * 20..i * 20 + 17], expect, "cube quad {i}");
    }

    let got: Vec<i32> =
        bytemuck::cast_slice(&model_read.slice(..).get_mapped_range().unwrap()).to_vec();
    let d = models[0].decode();
    let mut expect = Vec::new();
    for c in d.corners {
        expect.extend(c.map(|v| v.to_bits() as i32));
    }
    for uv in d.uvs {
        expect.extend(uv.map(|v| v.to_bits() as i32));
    }
    expect.extend([
        d.layer.into(),
        d.shade_face as i32,
        d.flags.into(),
        d.tint.into(),
    ]);
    expect.extend(d.light.map(i32::from));
    expect.extend(d.ao.map(i32::from));
    expect.extend(d.cell.map(i32::from));
    assert_eq!(got[..expect.len()], expect[..]);
}

#[test]
fn synthetic_terrain_renders_nontrivial_image() {
    let (w, h) = (960, 540);
    let Some(mut r) = renderer(w, h) else { return };
    upload_world(&mut r, 16, [0; 3]);
    frame(&mut r, &terrain_camera(DVec3::ZERO));
    let px = r.read_offscreen().unwrap();
    save_png("render-core-terrain.png", w, h, &px);

    let pixels = px.as_chunks::<4>().0;
    let colors: HashSet<[u8; 4]> = pixels.iter().copied().collect();
    let sky_px = pixels.iter().filter(|p| p[2] > 240 && p[1] > 150).count();
    let total = (w * h) as usize;
    assert!(colors.len() > 200, "only {} distinct colours", colors.len());
    assert!(sky_px > total / 50, "no sky visible");
    assert!(total - sky_px > total / 4, "no terrain visible");
    assert!(r.cube_quads_used() > 100_000);
    assert!(r.model_quads_used() > 0);
}

#[test]
fn draw_count_and_fallback_paths_match() {
    let (w, h) = (320, 180);
    let Some(mut a) = renderer(w, h) else { return };
    let Some(mut b) = renderer_with(
        w,
        h,
        RendererConfig {
            allow_draw_count: false,
            ..RendererConfig::default()
        },
    ) else {
        return;
    };
    for r in [&mut a, &mut b] {
        upload_world(r, 5, [0; 3]);
        frame(r, &terrain_camera(DVec3::ZERO));
    }
    assert_eq!(a.read_offscreen().unwrap(), b.read_offscreen().unwrap());
}

#[test]
fn rendering_far_from_origin_is_identical() {
    let (w, h) = (320, 180);
    let shift_sections = 1_874_999;
    let shift_blocks = f64::from(shift_sections) * 16.0;
    assert_eq!(shift_blocks, 29_999_984.0);

    let render = |sections: i32, offset: DVec3| {
        let mut r = renderer(w, h)?;
        upload_world(&mut r, 5, [sections, 0, -sections]);
        frame(&mut r, &terrain_camera(offset));
        Some(r.read_offscreen().unwrap())
    };
    let (Some(near), Some(far)) = (
        render(0, DVec3::ZERO),
        render(shift_sections, DVec3::new(shift_blocks, 0.0, -shift_blocks)),
    ) else {
        return;
    };
    assert!(near == far, "images differ at large coordinates");
}

fn one_quad_per_face() -> SectionMesh {
    let mut m = SectionMesh::default();
    for face in Face::ALL {
        m.push_cube(
            PackedQuad::encode(&CubeQuad {
                x: 4,
                y: 4,
                z: 4,
                face,
                width: 2,
                height: 2,
                layer: 3,
                ao: [4; 4],
                light: [LIGHT_FULL_SKY; 4],
                tint: 0,
                rotation: 0,
                opaque: false,
            })
            .unwrap(),
        );
    }
    m
}

fn cube_draws_for(cam: Camera) -> Option<u32> {
    let mut r = renderer(64, 64)?;
    r.set_section([0, 0, 0], &one_quad_per_face()).unwrap();
    frame(&mut r, &cam);
    r.wait_gpu_stats().map(|g| g.cube_draws)
}

#[test]
fn culling_skips_back_faces_and_offscreen_sections() {
    if renderer(8, 8).is_none() {
        return;
    }
    let at = |x: f64, yaw: f32| Camera {
        position: DVec3::new(x, 8.0, 8.0),
        yaw,
        ..Camera::default()
    };
    let toward_section_from_west = -std::f32::consts::FRAC_PI_2;
    let draws = cube_draws_for(at(-10.0, toward_section_from_west)).expect("stats");
    assert_eq!(draws, 5, "+X faces face away from a camera on the -X side");

    let draws = cube_draws_for(at(8.0, toward_section_from_west)).expect("stats");
    assert_eq!(draws, 6, "camera inside the section sees all directions");

    let behind = cube_draws_for(at(-10.0, std::f32::consts::FRAC_PI_2)).expect("stats");
    assert_eq!(behind, 0, "section behind the camera is frustum culled");
}

#[test]
fn full_arenas_grow_and_keep_every_section() {
    let (w, h) = (320, 180);
    let Some(mut roomy) = renderer(w, h) else {
        return;
    };
    let mut tiny = renderer_with(
        w,
        h,
        RendererConfig {
            cube_arena_quads: 8,
            model_arena_quads: 8,
            ..RendererConfig::default()
        },
    )
    .unwrap();
    let cam = terrain_camera(DVec3::ZERO);
    upload_world(&mut roomy, 5, [0; 3]);
    frame(&mut roomy, &cam);
    for (i, (k, mesh)) in generate_world(&Terrain { seed: 11 }, 5, 4)
        .into_iter()
        .enumerate()
    {
        tiny.set_section(k, &mesh).expect("arena grows");
        if i % 16 == 0 {
            frame(&mut tiny, &cam);
        }
    }
    frame(&mut tiny, &cam);
    assert_eq!(tiny.cube_quads_used(), roomy.cube_quads_used());
    assert_eq!(tiny.model_quads_used(), roomy.model_quads_used());
    assert!(tiny.memory_stats().model_arena_bytes < roomy.memory_stats().model_arena_bytes);
    assert!(tiny.read_offscreen().unwrap() == roomy.read_offscreen().unwrap());
}

#[test]
fn full_tint_arena_grows_and_keeps_its_tables() {
    let Some(mut r) = renderer_with(
        64,
        64,
        RendererConfig {
            tint_tables: 1,
            ..RendererConfig::default()
        },
    ) else {
        return;
    };
    r.set_texture_groups(&[flat_group(&[[255, 255, 255, 255]])]);
    let tinted = |rgb: u32| {
        let mut table = Box::new([0u32; mcr_render::TINT_TABLE_WORDS]);
        table[..256].fill(rgb);
        let mut m = wall(0, 1);
        m.tints = Some(table);
        m
    };
    let looking_at = |section: f64| Camera {
        position: DVec3::new(8.0, 8.0, 20.0) + DVec3::splat(section * 16.0),
        ..Camera::default()
    };
    let center = |r: &mut Renderer, section: f64| {
        frame(r, &looking_at(section));
        let px = r.read_offscreen().unwrap();
        let i = (32 * 64 + 32) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    r.set_section([0, 0, 0], &tinted(0x00ff00)).unwrap();
    assert!(center(&mut r, 0.0)[1] > 150);
    r.set_section([5, 5, 5], &tinted(0xff0000)).unwrap();
    let first = center(&mut r, 0.0);
    assert!(
        first[1] > 150 && first[0] < 10 && first[2] < 10,
        "{first:?}"
    );
    let second = center(&mut r, 5.0);
    assert!(second[2] > 150 && second[1] < 10, "{second:?}");
}

#[test]
fn clearing_sections_frees_every_world_and_height() {
    let Some(mut r) = renderer(32, 32) else {
        return;
    };
    let cam = terrain_camera(DVec3::ZERO);
    for round in 0..12 {
        upload_world(&mut r, 3, [round * 40, (round % 3) * 8 - 8, 0]);
        frame(&mut r, &cam);
        assert!(r.section_count() > 0);
        r.clear_sections();
        frame(&mut r, &cam);
        assert_eq!(r.section_count(), 0);
        assert_eq!(r.graph_nodes(), 0);
        assert_eq!(r.cube_quads_used(), 0);
        assert_eq!(r.model_quads_used(), 0);
    }
}

fn flat_group(layers: &[[u8; 4]]) -> mcr_render::TextureGroupData {
    let mips = (0..5)
        .map(|m| {
            let texels = (16usize >> m).pow(2);
            layers
                .iter()
                .flat_map(|c| std::iter::repeat_n(*c, texels).flatten())
                .collect()
        })
        .collect();
    mcr_render::TextureGroupData {
        width: 16,
        height: 16,
        layers: layers.len() as u32,
        mips,
        animations: Vec::new(),
    }
}

fn translucent_scene(with_glass: bool) -> Option<[u8; 4]> {
    let mut r = renderer(64, 64)?;
    r.set_texture_groups(&[flat_group(&[[255, 0, 0, 255], [0, 0, 255, 128]])]);
    let mut m = SectionMesh::default();
    m.push_cube(
        PackedQuad::encode(&CubeQuad {
            x: 4,
            y: 4,
            z: 4,
            face: Face::PosZ,
            width: 8,
            height: 8,
            layer: 0,
            ao: [4; 4],
            light: [LIGHT_FULL_SKY; 4],
            tint: 0,
            rotation: 0,
            opaque: false,
        })
        .unwrap(),
    );
    if with_glass {
        m.translucent.push(
            ModelQuad::encode(&ModelQuadDesc {
                corners: [
                    [7.0, 7.0, 10.0],
                    [9.5, 7.0, 10.0],
                    [7.0, 9.5, 10.0],
                    [9.5, 9.5, 10.0],
                ],
                uvs: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
                layer: 1,
                light: [LIGHT_FULL_SKY; 4],
                shade_face: Face::PosZ,
                flags: 0,
                ao: [63; 4],
                tint: 0,
                cell: [7, 7, 10],
            })
            .unwrap(),
        );
    }
    r.set_section([0, 0, 0], &m).unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn translucent_pass_blends_over_opaque() {
    let Some(plain) = translucent_scene(false) else {
        return;
    };
    let blended = translucent_scene(true).unwrap();
    assert!(plain[0] > 150 && plain[2] < 10, "{plain:?}");
    assert!(blended[2] > 60, "glass shows: {blended:?}");
    assert!(blended[0] > 60, "opaque shows through: {blended:?}");
}

fn glass(layer: u16) -> ModelQuad {
    ModelQuad::encode(&ModelQuadDesc {
        corners: [
            [7.0, 7.0, 10.0],
            [9.5, 7.0, 10.0],
            [7.0, 9.5, 10.0],
            [9.5, 9.5, 10.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        layer,
        light: [LIGHT_FULL_SKY; 4],
        shade_face: Face::PosZ,
        flags: 0,
        ao: [63; 4],
        tint: 0,
        cell: [7, 7, 10],
    })
    .unwrap()
}

#[test]
fn translucent_draws_wait_for_their_upload() {
    let config = RendererConfig {
        upload_budget_bytes: 1,
        ..RendererConfig::default()
    };
    let Some(mut r) = renderer_with(64, 64, config) else {
        return;
    };
    r.set_texture_groups(&[flat_group(&[
        [255, 0, 0, 255],
        [0, 0, 255, 128],
        [0, 255, 0, 128],
    ])]);
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    let center = |r: &mut Renderer| {
        frame(r, &cam);
        let px = r.read_offscreen().unwrap();
        let i = (32 * 64 + 32) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    let mut blue = wall(0, 0);
    blue.translucent.push(glass(1));
    let mut green = wall(0, 0);
    green.translucent.extend([glass(2), glass(2)]);
    for mesh in [&mut blue, &mut green] {
        mesh.sort_camera = Some(cam.position.to_array());
    }

    r.set_section([0, 0, 0], &blue).unwrap();
    let first = center(&mut r);
    assert!(first[2] > 60 && first[1] < 10, "old water: {first:?}");

    r.set_section([3, 0, 0], &wall(0, 0)).unwrap();
    r.set_section([0, 0, 0], &green).unwrap();
    let waiting = center(&mut r);
    assert_eq!(
        waiting, first,
        "the old water stays until the new upload is written"
    );
    let switched = center(&mut r);
    assert!(
        switched[1] > 60 && switched[2] < 10,
        "new water: {switched:?}"
    );

    r.set_section([4, 0, 0], &wall(0, 0)).unwrap();
    r.set_section([0, 0, 0], &blue).unwrap();
    r.remove_section([0, 0, 0]);
    r.set_section([0, 0, 0], &green).unwrap();
    for _ in 0..3 {
        let px = center(&mut r);
        let old_water = px[2] > 60 && px[1] < 10;
        assert!(
            !old_water,
            "a removed section's pending water never draws: {px:?}"
        );
    }
    let readded = center(&mut r);
    assert!(readded[1] > 60 && readded[2] < 10, "{readded:?}");
}

fn wall(layer: u16, tint: u8) -> SectionMesh {
    let mut m = SectionMesh::default();
    m.push_cube(
        PackedQuad::encode(&CubeQuad {
            x: 4,
            y: 4,
            z: 4,
            face: Face::PosZ,
            width: 8,
            height: 8,
            layer,
            ao: [4; 4],
            light: [LIGHT_FULL_SKY; 4],
            tint,
            rotation: 0,
            opaque: false,
        })
        .unwrap(),
    );
    m
}

fn center_pixel(
    groups: &[mcr_render::TextureGroupData],
    mesh: &SectionMesh,
    palette: &[u32],
) -> Option<[u8; 4]> {
    let mut r = renderer(64, 64)?;
    r.set_texture_groups(groups);
    r.set_tint_palette(palette);
    r.set_section([0, 0, 0], mesh).unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn texture_groups_are_addressed_by_layer_high_bits() {
    let groups = [
        flat_group(&[[255, 0, 0, 255]]),
        flat_group(&[[0, 255, 0, 255]]),
    ];
    let Some(first) = center_pixel(&groups, &wall(0, 0), &[]) else {
        return;
    };
    let second = center_pixel(&groups, &wall(1 << 14, 0), &[]).unwrap();
    assert!(first[0] > 150 && first[1] < 10, "{first:?}");
    assert!(second[1] > 150 && second[0] < 10, "{second:?}");
}

#[test]
fn tint_tables_and_constant_palette_colour_quads() {
    let white = [flat_group(&[[255, 255, 255, 255]])];
    let mut table = Box::new([0u32; mcr_render::TINT_TABLE_WORDS]);
    table[..256].fill(0x00ff00);
    let mut tinted = wall(0, 1);
    tinted.tints = Some(table);
    let Some(green) = center_pixel(&white, &tinted, &[]) else {
        return;
    };
    assert!(
        green[1] > 150 && green[0] < 10 && green[2] < 10,
        "{green:?}"
    );

    let constant = center_pixel(&white, &wall(0, 8), &[0xff0000]).unwrap();
    assert!(constant[2] > 150 && constant[0] < 10, "{constant:?}");
    let plain = center_pixel(&white, &wall(0, 0), &[]).unwrap();
    assert!(
        plain[0] > 150 && plain[1] > 150 && plain[2] > 150,
        "{plain:?}"
    );
}

#[test]
fn blocks_pick_their_tint_table_by_height() {
    let white = [flat_group(&[[255, 255, 255, 255]])];
    let words = mcr_render::TINT_TABLE_WORDS;
    let mut tables = vec![0u32; 2 * words];
    tables[..256].fill(0x00ff00);
    tables[words..words + 256].fill(0x0000ff);
    for word in &mut tables[..16] {
        *word |= 1 << 24;
    }
    let mut tinted = wall(0, 1);
    tinted.tints = Some(tables.into_boxed_slice());
    let Some(red) = center_pixel(&white, &tinted, &[]) else {
        return;
    };
    assert!(red[0] > 150 && red[1] < 10, "{red:?}");
}

#[test]
fn every_texture_filter_mode_draws_the_texture() {
    use mcr_render::{FilterMode, TextureFilter};
    for mode in [
        FilterMode::Rgss,
        FilterMode::Anisotropic,
        FilterMode::Nearest,
    ] {
        let Some(mut r) = renderer(64, 64) else {
            return;
        };
        r.set_texture_groups(&[flat_group(&[[255, 0, 0, 255]])]);
        r.set_texture_filter(TextureFilter {
            mode,
            anisotropy: 4,
            mip_levels: 4,
        });
        r.set_section([0, 0, 0], &wall(0, 0)).unwrap();
        let cam = Camera {
            position: DVec3::new(8.0, 8.0, 20.0),
            ..Camera::default()
        };
        frame(&mut r, &cam);
        let px = r.read_offscreen().unwrap();
        let i = (32 * 64 + 32) * 4;
        assert!(
            px[i] > 150 && px[i + 1] < 10,
            "{mode:?}: {:?}",
            &px[i..i + 4]
        );
    }
}

fn lit_wall(layer: u16, light: u16) -> SectionMesh {
    let mut m = SectionMesh::default();
    m.push_cube(
        PackedQuad::encode(&CubeQuad {
            x: 4,
            y: 4,
            z: 4,
            face: Face::PosZ,
            width: 8,
            height: 8,
            layer,
            ao: [4; 4],
            light: [light; 4],
            tint: 0,
            rotation: 0,
            opaque: false,
        })
        .unwrap(),
    );
    m
}

fn pixel_with(
    group: mcr_render::TextureGroupData,
    mesh: &SectionMesh,
    setup: impl FnOnce(&mut Renderer),
    camera_z: f64,
) -> Option<[u8; 4]> {
    let mut r = renderer(64, 64)?;
    r.set_texture_groups(&[group]);
    setup(&mut r);
    r.set_section([0, 0, 0], mesh).unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, camera_z),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn day_time_relights_sky_light_but_not_block_light() {
    let white = flat_group(&[[255, 255, 255, 255]]);
    let sky = lit_wall(0, mcr_render::pack_light(240, 0));
    let torch = lit_wall(0, mcr_render::pack_light(0, 128));
    let at = |time: u64, mesh: &SectionMesh| {
        pixel_with(
            white.clone(),
            mesh,
            |r| r.set_environment(&mcr_render::Environment::overworld(time, 0.0, 0.0)),
            20.0,
        )
    };
    let Some(day) = at(6000, &sky) else { return };
    let night = at(18000, &sky).unwrap();
    assert!(day[0] > 150 && night[0] < day[0] / 2, "{day:?} {night:?}");
    let torch_day = at(6000, &torch).unwrap();
    let torch_night = at(18000, &torch).unwrap();
    assert!(
        torch_day[0].abs_diff(torch_night[0]) <= 6,
        "{torch_day:?} {torch_night:?}"
    );
    assert!(
        torch_day[0] > torch_day[2],
        "block light is warm: {torch_day:?}"
    );
}

#[test]
fn distant_surfaces_fade_to_fog() {
    let white = flat_group(&[[255, 255, 255, 255]]);
    let wall = lit_wall(0, mcr_render::LIGHT_FULL_SKY);
    let env = mcr_render::Environment {
        fog_color: [1.0, 0.0, 0.0],
        render_distance: 30.0,
        ..Default::default()
    };
    let near = pixel_with(white.clone(), &wall, |r| r.set_environment(&env), 20.0);
    let Some(near) = near else { return };
    let far = pixel_with(white, &wall, |r| r.set_environment(&env), 45.0).unwrap();
    assert!(
        near[1] > 100 && far[1] < 20 && far[0] > 200,
        "{near:?} {far:?}"
    );
}

fn animated_group(interpolate: bool) -> mcr_render::TextureGroupData {
    let mut g = flat_group(&[[255, 0, 0, 255], [0, 255, 0, 255]]);
    g.animations = vec![mcr_render::SpriteAnimation {
        layer: 0,
        frames: vec![
            mcr_render::SpriteFrame {
                layer_offset: 0,
                ticks: 4,
            },
            mcr_render::SpriteFrame {
                layer_offset: 1,
                ticks: 4,
            },
        ],
        interpolate,
    }];
    g
}

#[test]
fn animated_sprites_advance_without_new_geometry() {
    let wall = lit_wall(0, mcr_render::LIGHT_FULL_SKY);
    let at = |tick: u64, interpolate: bool| {
        pixel_with(
            animated_group(interpolate),
            &wall,
            |r| r.set_animation_tick(tick),
            20.0,
        )
    };
    let Some(first) = at(0, false) else { return };
    let second = at(4, false).unwrap();
    assert!(first[0] > 150 && first[1] < 10, "{first:?}");
    assert!(second[1] > 150 && second[0] < 10, "{second:?}");
    let mid = at(2, true).unwrap();
    assert!(mid[0] > 60 && mid[1] > 60, "interpolated: {mid:?}");
}

#[test]
fn animation_tick_changes_reuse_uploaded_sections() {
    let Some(mut r) = renderer(64, 64) else {
        return;
    };
    r.set_texture_groups(&[animated_group(false)]);
    r.set_section([0, 0, 0], &lit_wall(0, mcr_render::LIGHT_FULL_SKY))
        .unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    let read = |r: &mut Renderer, tick: u64| {
        r.set_animation_tick(tick);
        frame(r, &cam);
        let px = r.read_offscreen().unwrap();
        let i = (32 * 64 + 32) * 4;
        [px[i], px[i + 1]]
    };
    let a = read(&mut r, 0);
    let b = read(&mut r, 4);
    let c = read(&mut r, 8);
    assert!(a[0] > 150 && b[1] > 150 && c[0] > 150);
}

fn dark_wall_pixel(dynamic_lights: bool, lights: &[mcr_render::DynamicLight]) -> Option<[u8; 4]> {
    let mut r = renderer_with(
        64,
        64,
        RendererConfig {
            dynamic_lights,
            ..RendererConfig::default()
        },
    )?;
    r.set_texture_groups(&[flat_group(&[[255, 255, 255, 255]])]);
    r.set_environment(&mcr_render::Environment::overworld(18000, 0.0, 0.0));
    r.set_section([0, 0, 0], &lit_wall(0, mcr_render::pack_light(0, 0)))
        .unwrap();
    r.set_dynamic_lights(lights);
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn dynamic_light_brightens_terrain_near_it_only() {
    use mcr_render::DynamicLight;
    let Some(dark) = dark_wall_pixel(true, &[]) else {
        return;
    };
    // The wall's face is at z = 5; the pixel at the centre looks at (8, 8, 5).
    let near = [DynamicLight::point(DVec3::new(8.0, 8.0, 6.5), 14.0)];
    let lit = dark_wall_pixel(true, &near).unwrap();
    assert!(
        lit[0] > dark[0].saturating_add(80),
        "torch lights the wall: {dark:?} -> {lit:?}"
    );
    assert!(lit[0] > lit[2], "block light is warm: {lit:?}");
    let far = [DynamicLight::point(DVec3::new(8.0, 8.0, 15.0), 14.0)];
    assert_eq!(dark_wall_pixel(true, &far).unwrap(), dark, "out of reach");
    assert_eq!(
        dark_wall_pixel(false, &near).unwrap(),
        dark,
        "ignored without the variants"
    );
    let beam = [DynamicLight {
        start: DVec3::new(-20.0, 8.0, 6.0),
        end: DVec3::new(40.0, 8.0, 6.0),
        level: 15.0,
    }];
    let beam_lit = dark_wall_pixel(true, &beam).unwrap();
    assert!(
        beam_lit[0] > dark[0].saturating_add(80),
        "segment: {beam_lit:?}"
    );
}

fn pane(layer: u16, z: f32) -> ModelQuad {
    lit_pane(layer, z, LIGHT_FULL_SKY)
}

fn lit_pane(layer: u16, z: f32, light: u16) -> ModelQuad {
    ModelQuad::encode(&ModelQuadDesc {
        corners: [[7.0, 7.0, z], [9.5, 7.0, z], [7.0, 9.5, z], [9.5, 9.5, z]],
        uvs: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        layer,
        light: [light; 4],
        shade_face: Face::PosZ,
        flags: 0,
        ao: [63; 4],
        tint: 0,
        cell: [7, 7, z as u8],
    })
    .unwrap()
}

/// Green glass at z 12 in front of blue water at z 10 over a red wall, uploaded in `order`.
fn water_behind_glass(improved: bool, glass_first: bool) -> Option<[u8; 4]> {
    let mut r = renderer(64, 64)?;
    r.set_improved_transparency(improved);
    r.set_texture_groups(&[flat_group(&[
        [255, 0, 0, 255],
        [0, 0, 255, 128],
        [0, 255, 0, 128],
    ])]);
    let mut m = wall(0, 0);
    let (glass, water) = (pane(2, 12.0), pane(1, 10.0));
    m.translucent = if glass_first {
        vec![glass, water]
    } else {
        vec![water, glass]
    };
    r.set_section([0, 0, 0], &m).unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn improved_transparency_composites_water_behind_glass_in_any_order() {
    let Some(sorted) = water_behind_glass(false, false) else {
        return;
    };
    let unsorted = water_behind_glass(false, true).unwrap();
    let oit_a = water_behind_glass(true, true).unwrap();
    let oit_b = water_behind_glass(true, false).unwrap();
    let close = |a: [u8; 4], b: [u8; 4], tol: u8| (0..3).all(|c| a[c].abs_diff(b[c]) <= tol);
    assert!(
        close(oit_a, oit_b, 1),
        "order independent: {oit_a:?} {oit_b:?}"
    );
    assert!(
        close(oit_a, sorted, 8),
        "matches back to front: {oit_a:?} {sorted:?} (unsorted {unsorted:?})"
    );
    assert!(
        sorted[1] > sorted[2],
        "the nearer green glass dominates: {sorted:?}"
    );
    assert!(
        unsorted[2] > unsorted[1],
        "classic blending follows draw order: {unsorted:?}"
    );
}

#[test]
fn improved_transparency_turns_off_cleanly() {
    let Some(mut r) = renderer(32, 32) else {
        return;
    };
    let cam = Camera::default();
    r.set_improved_transparency(true);
    assert!(r.improved_transparency());
    frame(&mut r, &cam);
    r.resize(48, 40);
    frame(&mut r, &cam);
    r.set_improved_transparency(false);
    assert!(!r.improved_transparency());
    frame(&mut r, &cam);
}

/// Paints, after the world, a full-screen quad 10 blocks from the camera, depth-tested against the
/// world depth: between the glass (8 blocks away) and the wall behind it.
struct BehindGlass {
    pipeline: Option<wgpu::RenderPipeline>,
}

impl mcr_render::Overlay for BehindGlass {
    fn record(&mut self, ctx: &mut mcr_render::OverlayCtx<'_>) {
        let depth = ctx.camera.near / 10.0;
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let src = format!(
                "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {{
                    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
                    return vec4f(p * 2.0 - 1.0, {depth:?}, 1.0);
                }}
                @fragment fn fs() -> @location(0) vec4f {{ return vec4f(1.0, 0.0, 1.0, 1.0); }}"
            );
            let module = ctx
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: None,
                    source: wgpu::ShaderSource::Wgsl(src.into()),
                });
            ctx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &module,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(ctx.format.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
        });
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.draw(0..3, 0..1);
    }
}

fn behind_glass_pixel(improved: bool) -> Option<[u8; 4]> {
    let mut r = renderer(64, 64)?;
    r.set_improved_transparency(improved);
    r.set_texture_groups(&[flat_group(&[[255, 0, 0, 255], [0, 255, 0, 128]])]);
    let mut m = wall(0, 0);
    m.translucent = vec![pane(1, 12.0)];
    r.set_section([0, 0, 0], &m).unwrap();
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    let mut overlay = BehindGlass { pipeline: None };
    r.render_with_overlay(
        &FrameInputs {
            camera: &cam,
            sky_light: 1.0,
        },
        Some(&mut overlay),
    )
    .expect("render");
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn improved_transparency_composite_writes_the_nearest_translucent_depth() {
    let Some(classic) = behind_glass_pixel(false) else {
        return;
    };
    let improved = behind_glass_pixel(true).unwrap();
    assert_eq!(classic[..3], [255, 0, 255], "classic writes no glass depth");
    assert!(
        improved[1] > 60 && improved[2] < 40,
        "the glass hides what is drawn behind it: {improved:?}"
    );
}

fn dark_pane_pixel(lights: &[mcr_render::DynamicLight]) -> Option<[u8; 4]> {
    let mut r = renderer_with(
        64,
        64,
        RendererConfig {
            dynamic_lights: true,
            ..RendererConfig::default()
        },
    )?;
    r.set_improved_transparency(true);
    r.set_texture_groups(&[flat_group(&[[0, 0, 0, 255], [255, 255, 255, 160]])]);
    r.set_environment(&mcr_render::Environment::overworld(18000, 0.0, 0.0));
    let mut m = lit_wall(0, mcr_render::pack_light(0, 0));
    m.translucent = vec![lit_pane(1, 12.0, mcr_render::pack_light(0, 0))];
    r.set_section([0, 0, 0], &m).unwrap();
    r.set_dynamic_lights(lights);
    let cam = Camera {
        position: DVec3::new(8.0, 8.0, 20.0),
        ..Camera::default()
    };
    frame(&mut r, &cam);
    let px = r.read_offscreen().unwrap();
    let i = (32 * 64 + 32) * 4;
    Some([px[i], px[i + 1], px[i + 2], px[i + 3]])
}

#[test]
fn dynamic_lights_reach_translucent_terrain_with_improved_transparency() {
    use mcr_render::DynamicLight;
    let Some(dark) = dark_pane_pixel(&[]) else {
        return;
    };
    let lit = dark_pane_pixel(&[DynamicLight::point(DVec3::new(8.0, 8.0, 13.0), 14.0)]).unwrap();
    assert!(
        lit[0] > dark[0].saturating_add(60),
        "a torch lights the glass: {dark:?} -> {lit:?}"
    );
}
