use std::collections::HashSet;

use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{
    Camera, CubeQuad, Face, FrameInputs, ModelQuad, ModelQuadDesc, PackedQuad, RenderError,
    Renderer, RendererConfig, SectionMesh, Target,
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
        (0, 0, 0, Face::PosX, 1, 1, 0, [0, 0, 0, 0], 0),
        (31, 31, 31, Face::NegZ, 16, 16, 0xffff, [3, 2, 1, 0], 0xff),
        (15, 7, 3, Face::PosY, 9, 4, 1234, [1, 3, 0, 2], 0x5a),
    ]
    .into_iter()
    .map(|(x, y, z, face, width, height, layer, ao, light)| {
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
            tint: 0,
        })
        .unwrap()
    })
    .collect();
    let model_desc = ModelQuadDesc {
        corners: [
            [-1.0, 0.0, 0.5],
            [0.03125, 15.5, 30.96875],
            [30.96875, 8.0, 0.0],
            [16.0, 16.0, 16.0],
        ],
        uvs: [[0.0, 0.0], [15.9375, 0.0], [0.0, 1.5], [8.0, 15.9375]],
        layer: 65535,
        light: 0x3c,
        shade_face: Face::NegZ,
        flags: 17,
        ao: [3, 2, 1, 0],
        tint: 0,
        cell: [0, 0],
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
    let cube_out = init("cube out", &vec![0u8; cubes.len() * 48], out_usage);
    let model_out = init("model out", &vec![0u8; models.len() * 128], out_usage);

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
    let (cube_read, model_read) = (read(cubes.len() as u64 * 48), read(128));
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(cubes.len() as u32, 1, 1);
    }
    enc.copy_buffer_to_buffer(&cube_out, 0, &cube_read, 0, cube_read.size());
    enc.copy_buffer_to_buffer(&model_out, 0, &model_read, 0, 128);
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
            d.light.into(),
        ];
        assert_eq!(got[i * 12..i * 12 + 12], expect, "cube quad {i}");
    }

    let got: Vec<i32> =
        bytemuck::cast_slice(&model_read.slice(..).get_mapped_range().unwrap()).to_vec();
    let d = models[0].decode();
    let mut expect = Vec::new();
    for c in d.corners {
        expect.extend(c.map(|v| (v * 32.0).round() as i32));
    }
    for uv in d.uvs {
        expect.extend(uv.map(|v| (v * 16.0).round() as i32));
    }
    expect.extend([
        d.layer.into(),
        d.light.into(),
        d.shade_face as i32,
        d.flags.into(),
    ]);
    expect.extend(d.ao.map(i32::from));
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
    let sky_px = pixels.iter().filter(|p| p[2] > 240 && p[0] < 140).count();
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
                ao: [3; 4],
                light: 0xf0,
                tint: 0,
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
fn arena_exhaustion_keeps_existing_sections() {
    let Some(mut r) = renderer_with(
        32,
        32,
        RendererConfig {
            cube_arena_quads: 8,
            ..RendererConfig::default()
        },
    ) else {
        return;
    };
    r.set_section([0, 0, 0], &one_quad_per_face()).unwrap();
    let err = r.set_section([1, 0, 0], &one_quad_per_face()).unwrap_err();
    assert!(matches!(err, RenderError::ArenaFull("cube")));
    assert_eq!(r.cube_quads_used(), 6);
    r.set_section([0, 0, 0], &SectionMesh::default()).unwrap();
    assert_eq!(r.cube_quads_used(), 0);
    r.set_section([1, 0, 0], &one_quad_per_face()).unwrap();
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
            ao: [3; 4],
            light: 0xf0,
            tint: 0,
        })
        .unwrap(),
    );
    if with_glass {
        m.translucent.push(
            ModelQuad::encode(&ModelQuadDesc {
                corners: [
                    [4.0, 4.0, 10.0],
                    [12.0, 4.0, 10.0],
                    [4.0, 12.0, 10.0],
                    [12.0, 12.0, 10.0],
                ],
                uvs: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
                layer: 1,
                light: 0xf0,
                shade_face: Face::PosZ,
                flags: 0,
                ao: [3; 4],
                tint: 0,
                cell: [0, 0],
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
            ao: [3; 4],
            light: 0xf0,
            tint,
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
