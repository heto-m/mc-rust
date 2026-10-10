use mcr_render::glam::DVec3;
use mcr_render::synthetic::{Terrain, generate_world};
use mcr_render::{
    Camera, CubeQuad, Face, FrameInputs, LIGHT_FULL_SKY, PackedQuad, RenderError, Renderer,
    RendererConfig, SectionMesh, SectionVisibility, Target,
};

fn renderer(w: u32, h: u32, occlusion: bool) -> Option<Renderer> {
    renderer_with(w, h, occlusion, true)
}

fn renderer_with(w: u32, h: u32, occlusion: bool, graph: bool) -> Option<Renderer> {
    match Renderer::new(
        Target::Offscreen {
            width: w,
            height: h,
        },
        RendererConfig {
            occlusion_culling: occlusion,
            visibility_graph: graph,
            ..RendererConfig::default()
        },
    ) {
        Ok(r) => Some(r),
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            None
        }
        Err(e) => panic!("renderer creation failed: {e}"),
    }
}

fn pair(w: u32, h: u32) -> Option<(Renderer, Renderer)> {
    Some((renderer(w, h, false)?, renderer(w, h, true)?))
}

fn frame(r: &mut Renderer, cam: &Camera) -> Vec<u8> {
    r.render(&FrameInputs {
        camera: cam,
        sky_light: 1.0,
    })
    .expect("render");
    r.read_offscreen().unwrap()
}

fn cam(x: f64, y: f64, z: f64, yaw_deg: f32, pitch_deg: f32) -> Camera {
    Camera {
        position: DVec3::new(x, y, z),
        yaw: yaw_deg.to_radians(),
        pitch: pitch_deg.to_radians(),
        ..Camera::default()
    }
}

fn upload_terrain(r: &mut Renderer) {
    for (k, mesh) in generate_world(&Terrain { seed: 11 }, 7, 4) {
        r.set_section(k, &mesh).expect("section fits");
    }
}

/// Renders the same camera path with and without occlusion culling, requires identical frames and
/// returns the total cube draws of each.
fn assert_path_identical(path: &[Camera]) -> Option<(u32, u32)> {
    let (w, h) = (480, 270);
    let (mut plain, mut culled) = pair(w, h)?;
    let mut draws = (0, 0);
    upload_terrain(&mut plain);
    upload_terrain(&mut culled);
    for (i, c) in path.iter().enumerate() {
        let expected = frame(&mut plain, c);
        let got = frame(&mut culled, c);
        let differing = expected
            .as_chunks::<4>()
            .0
            .iter()
            .zip(got.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "frame {i} differs in {differing} pixels");
        draws.0 += plain.wait_gpu_stats().expect("stats").cube_draws;
        draws.1 += culled.wait_gpu_stats().expect("stats").cube_draws;
    }
    Some(draws)
}

#[test]
fn occluded_frames_match_unculled_frames() {
    let draws = assert_path_identical(&[
        cam(8.5, 118.0, 8.5, 34.0, -26.0),
        cam(8.5, 118.0, 8.5, 34.0, -26.0),
        cam(8.5, 80.0, 8.5, 0.0, 0.0),
        cam(8.5, 80.0, 8.5, 0.0, 0.0),
        cam(-40.5, 72.0, 30.5, 120.0, -5.0),
        cam(-40.5, 72.0, 30.5, 120.0, -5.0),
        cam(20.0, 160.0, -20.0, 200.0, -60.0),
        cam(20.0, 160.0, -20.0, 200.0, -60.0),
        cam(3.0, 30.0, 3.0, 45.0, 10.0),
        cam(3.0, 30.0, 3.0, 45.0, 10.0),
    ]);
    if let Some((plain, culled)) = draws {
        assert!(
            culled < plain,
            "hills hide nothing: {culled} vs {plain} draws"
        );
    }
}

#[test]
fn fast_turns_and_teleports_never_drop_sections() {
    let at = DVec3::new(8.5, 82.0, 8.5);
    let mut path = Vec::new();
    for yaw in [0.0, 180.0, 90.0, 270.0, 0.0, 179.0] {
        path.push(cam(at.x, at.y, at.z, yaw, -4.0));
    }
    path.push(cam(at.x, at.y, at.z, 0.0, 89.0));
    path.push(cam(at.x, at.y, at.z, 0.0, -89.0));
    path.push(cam(-60.0, 75.0, 60.0, 315.0, 0.0));
    path.push(cam(60.0, 75.0, -60.0, 135.0, 0.0));
    assert_path_identical(&path);
}

fn plane(face: Face, at: u8) -> SectionMesh {
    let mut m = SectionMesh::default();
    m.push_cube(
        PackedQuad::encode(&CubeQuad {
            x: 0,
            y: 0,
            z: at,
            face,
            width: 16,
            height: 16,
            layer: 3,
            ao: [4; 4],
            light: [LIGHT_FULL_SKY; 4],
            tint: 0,
            rotation: 0,
        })
        .unwrap(),
    );
    m
}

fn cube_draws(r: &mut Renderer, c: &Camera) -> u32 {
    frame(r, c);
    r.wait_gpu_stats().expect("stats").cube_draws
}

#[test]
fn sections_behind_a_wall_are_not_drawn() {
    let Some((mut plain, mut culled)) = pair(128, 128) else {
        return;
    };
    let hidden = [[-1, -1, -5], [0, 0, -5], [1, 0, -6], [0, 1, -7]];
    for r in [&mut plain, &mut culled] {
        for x in -3..=3 {
            for y in -3..=3 {
                r.set_section([x, y, -2], &plane(Face::PosZ, 15)).unwrap();
            }
        }
        for key in hidden {
            r.set_section(key, &plane(Face::PosZ, 8)).unwrap();
        }
    }
    let c = cam(0.5, 0.5, 0.5, 0.0, 0.0);
    let all = cube_draws(&mut plain, &c);
    assert_eq!(cube_draws(&mut plain, &c), all);
    assert_eq!(
        cube_draws(&mut culled, &c),
        all,
        "without history the late phase draws everything"
    );
    assert_eq!(
        cube_draws(&mut culled, &c),
        all,
        "sections visible last frame are drawn before being retested"
    );
    let steady = cube_draws(&mut culled, &c);
    assert_eq!(steady + hidden.len() as u32, all);
    assert_eq!(cube_draws(&mut culled, &c), steady);
    assert_eq!(frame(&mut plain, &c), frame(&mut culled, &c));

    let wall_removed = cam(0.5, 0.5, 0.5, 0.0, 0.0);
    for x in -3..=3 {
        for y in -3..=3 {
            plain.remove_section([x, y, -2]);
            culled.remove_section([x, y, -2]);
        }
    }
    assert_eq!(
        frame(&mut plain, &wall_removed),
        frame(&mut culled, &wall_removed),
        "disoccluded sections appear in the same frame"
    );
}

#[test]
fn closed_sections_cut_the_visibility_graph() {
    let (Some(mut open), Some(mut graph)) = (
        renderer_with(128, 128, false, false),
        renderer_with(128, 128, false, true),
    ) else {
        return;
    };
    let mut wall = plane(Face::PosZ, 15);
    wall.visibility = SectionVisibility::NONE;
    let hidden = [[-1, -1, -5], [0, 0, -5], [1, 0, -6], [0, 1, -7]];
    for r in [&mut open, &mut graph] {
        for x in -3..=3 {
            for y in -3..=3 {
                r.set_section([x, y, -2], &wall).unwrap();
            }
        }
        for key in hidden {
            r.set_section(key, &plane(Face::PosZ, 8)).unwrap();
        }
    }
    let c = cam(0.5, 0.5, 0.5, 0.0, 0.0);
    let all = cube_draws(&mut open, &c);
    assert_eq!(cube_draws(&mut graph, &c) + hidden.len() as u32, all);
    assert_eq!(frame(&mut open, &c), frame(&mut graph, &c));

    let mut opened = plane(Face::PosZ, 15);
    opened.visibility = SectionVisibility::ALL;
    graph.set_section([0, 0, -2], &opened).unwrap();
    open.set_section([0, 0, -2], &opened).unwrap();
    let through = cube_draws(&mut graph, &c);
    assert!(
        through > all - hidden.len() as u32,
        "opening a wall section reconnects the search"
    );

    graph.set_camera_in_opaque_block(true);
    assert_eq!(
        cube_draws(&mut graph, &c),
        all,
        "no graph search from inside a block"
    );
}
