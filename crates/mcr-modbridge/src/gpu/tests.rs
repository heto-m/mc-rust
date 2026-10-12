use super::backend::{Backend, State, backend};
use super::info::{
    Attribute, Blend, ColorTarget, PipelineInfo, Shader, Stage, Uniform, VertexBuffer,
};
use super::pass::{ColorAttachment, PassDesc, StreamWriter, encode_pass};
use super::slab::Handle;
use super::spv::UniformKind;
use super::spv::tests::{test_fragment, test_vertex};

const RGBA8: u32 = 6;
const RG32_FLOAT: u32 = 45;
const TOPOLOGY_TRIANGLES: u32 = 4;
const TOPOLOGY_FAN: u32 = 6;
const USAGE_MAP_READ: u32 = 1;
const USAGE_COPY_DST: u32 = 8;
const USAGE_VERTEX: u32 = 32;
const USAGE_UNIFORM: u32 = 128;
const TEXTURE_RENDER: u32 = 8 | 4 | 1 | 2;
const SIZE: u32 = 4;

fn gpu() -> Option<&'static Backend> {
    match backend() {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn info(topology: u32, blend: bool) -> PipelineInfo {
    PipelineInfo {
        name: format!("test/{topology}"),
        shaders: vec![
            Shader {
                stage: Stage::Vertex,
                spirv: test_vertex(),
            },
            Shader {
                stage: Stage::Fragment,
                spirv: test_fragment(),
            },
        ],
        vertex_buffers: vec![VertexBuffer {
            slot: 0,
            stride: 16,
            step_rate: 0,
        }],
        attributes: vec![
            Attribute {
                slot: 0,
                location: 0,
                offset: 0,
                format: RG32_FLOAT,
            },
            Attribute {
                slot: 0,
                location: 1,
                offset: 8,
                format: RG32_FLOAT,
            },
        ],
        uniforms: vec![
            Uniform {
                kind: UniformKind::Buffer,
                name: "Color".into(),
            },
            Uniform {
                kind: UniformKind::CombinedSampler,
                name: "Sampler0".into(),
            },
        ],
        push_constants_size: 16,
        depth: None,
        wireframe: false,
        cull: false,
        topology,
        color_targets: vec![Some(ColorTarget {
            format: RGBA8,
            write_mask: 0xF,
            blend: blend.then_some(Blend {
                color: [11, 9, 0],
                alpha: [4, 9, 0],
            }),
        })],
    }
}

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

struct Scene {
    b: &'static Backend,
    enc: Handle,
    target: Handle,
    target_view: Handle,
    quad: Handle,
    tex: Handle,
    view: Handle,
    sampler: Handle,
    ubo: Handle,
    push: Handle,
    fan: Handle,
}

/// A 4×4 target, a fan quad over all of it, and a 2×2 texture (red, green / blue, white).
fn scene() -> Option<Scene> {
    let b = gpu()?;
    let enc = b.create_encoder().unwrap();
    let target = b
        .create_texture(TEXTURE_RENDER, RGBA8, SIZE, SIZE, 1, 1)
        .unwrap();
    let target_view = b.create_view(target, 0, 1).unwrap();
    let quad = f32s(&[
        -1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 0.0, 1.0,
    ]);
    let quad = b
        .create_buffer(USAGE_VERTEX, quad.len() as u64, Some(&quad))
        .unwrap();
    let tex = b.create_texture(TEXTURE_RENDER, RGBA8, 2, 2, 1, 1).unwrap();
    let view = b.create_view(tex, 0, 1).unwrap();
    let sampler = b.create_sampler(1, 1, 0, 0, 1, Some(0.0)).unwrap();
    let ubo = b
        .create_buffer(USAGE_UNIFORM | USAGE_COPY_DST, 512, None)
        .unwrap();
    let push = b.create_transient_block(1024).unwrap();
    let fan = b
        .compile_pipeline(&info(TOPOLOGY_FAN, false).encode())
        .unwrap();
    let texels: Vec<u8> = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255; 4],
    ]
    .concat();
    {
        let mut s = b.lock();
        s.write_texture(b.gpu, enc, tex, &texels, 0, 0, 0, 0, 2, 2)
            .unwrap();
        s.write_buffer(b.gpu, enc, ubo, 0, &f32s(&[1.0; 4]))
            .unwrap();
    }
    b.write_host(push, 0, &f32s(&[0.0; 4])).unwrap();
    b.lock().transient_dirty(push, 0, 16).unwrap();
    Some(Scene {
        b,
        enc,
        target,
        target_view,
        quad,
        tex,
        view,
        sampler,
        ubo,
        push,
        fan,
    })
}

impl Scene {
    fn desc(&self, clear: Option<[f32; 4]>) -> PassDesc {
        PassDesc {
            colors: vec![Some(ColorAttachment {
                view: self.target_view,
                clear,
            })],
            depth: None,
            area: [0, 0, SIZE, SIZE],
            flags: 0,
        }
    }

    fn bind(&self, w: &mut StreamWriter, pipeline: Handle, ubo_offset: u64) {
        w.pipeline(pipeline)
            .uniform_buffer(0, self.ubo, ubo_offset, 16)
            .uniform_texture(1, self.view, self.sampler)
            .push_constants(self.push, 0, 16)
            .vertex(0, self.quad, 0);
    }

    fn pass(&self, s: &mut State, desc: &PassDesc, w: &StreamWriter) -> super::pass::PassReport {
        encode_pass(self.b, s, self.enc, desc, &w.words).unwrap()
    }

    /// Submits and returns the target's pixels, row 0 first.
    fn read(&self) -> Vec<[u8; 4]> {
        let b = self.b;
        let out = b
            .create_buffer(
                USAGE_MAP_READ | USAGE_COPY_DST,
                u64::from(SIZE * SIZE * 4),
                None,
            )
            .unwrap();
        let mut s = b.lock();
        s.copy_texture_to_buffer(
            b.gpu,
            self.enc,
            self.target,
            out,
            0,
            42,
            0,
            0,
            0,
            SIZE,
            SIZE,
        )
        .unwrap();
        let n = s.submit(b.gpu, &b.stats, &b.slots, self.enc).unwrap();
        assert!(s.wait(b.gpu, self.enc, n, None).unwrap());
        let mut ids = Vec::new();
        for _ in 0..100 {
            ids.extend(s.poll(b.gpu, self.enc).unwrap());
            if ids.contains(&42) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(ids, vec![42], "the readback completes after its submit");
        drop(s);
        let bytes = b.read_host(out).unwrap();
        b.destroy(out).unwrap();
        bytes.as_chunks::<4>().0.to_vec()
    }
}

#[test]
fn fan_quad_samples_the_texture_without_flipping() {
    let Some(sc) = scene() else { return };
    let mut w = StreamWriter::default();
    sc.bind(&mut w, sc.fan, 0);
    w.draw(4, 1, 0, 0);
    let report = sc.pass(&mut sc.b.lock(), &sc.desc(Some([0.0; 4])), &w);
    assert_eq!((report.draws, report.skipped), (1, 0), "{report:?}");
    let px = sc.read();
    let at = |x: u32, y: u32| px[(y * SIZE + x) as usize];
    assert_eq!(
        at(0, 0),
        [255, 0, 0, 255],
        "texture row 0 lands in target row 0"
    );
    assert_eq!(at(3, 0), [0, 255, 0, 255]);
    assert_eq!(at(0, 3), [0, 0, 255, 255]);
    assert_eq!(at(3, 3), [255; 4]);
}

#[test]
fn write_after_draw_keeps_order() {
    let Some(sc) = scene() else { return };
    let white =
        sc.b.create_texture(TEXTURE_RENDER, RGBA8, 1, 1, 1, 1)
            .unwrap();
    let white_view = sc.b.create_view(white, 0, 1).unwrap();
    let mut s = sc.b.lock();
    s.write_texture(sc.b.gpu, sc.enc, white, &[255; 4], 0, 0, 0, 0, 1, 1)
        .unwrap();
    s.write_buffer(sc.b.gpu, sc.enc, sc.ubo, 0, &f32s(&[1.0, 0.0, 0.0, 1.0]))
        .unwrap();
    let mut w = StreamWriter::default();
    sc.bind(&mut w, sc.fan, 0);
    w.uniform_texture(1, white_view, sc.sampler)
        .scissor(0, 0, 2, 4)
        .draw(4, 1, 0, 0);
    sc.pass(&mut s, &sc.desc(Some([0.0; 4])), &w);
    s.write_buffer(sc.b.gpu, sc.enc, sc.ubo, 0, &f32s(&[0.0, 1.0, 0.0, 1.0]))
        .unwrap();
    let mut w = StreamWriter::default();
    sc.bind(&mut w, sc.fan, 0);
    w.uniform_texture(1, white_view, sc.sampler)
        .scissor(2, 0, 2, 4)
        .draw(4, 1, 0, 0);
    sc.pass(&mut s, &sc.desc(None), &w);
    drop(s);
    let px = sc.read();
    assert_eq!(
        px[0],
        [255, 0, 0, 255],
        "the first draw saw the old uniform"
    );
    assert_eq!(px[3], [0, 255, 0, 255], "the second draw saw the new one");
}

#[test]
fn closed_texture_skips_the_draw_and_keeps_recording() {
    let Some(sc) = scene() else { return };
    sc.b.destroy(sc.tex).unwrap();
    let mut w = StreamWriter::default();
    sc.bind(&mut w, sc.fan, 0);
    w.draw(4, 1, 0, 0);
    w.pipeline(0xdead_0000_5000_0001);
    w.draw(4, 1, 0, 0);
    let report = sc.pass(&mut sc.b.lock(), &sc.desc(Some([0.0, 0.0, 0.0, 1.0])), &w);
    assert_eq!((report.draws, report.skipped), (0, 3), "{report:?}");
    assert!(report.first_error.unwrap().contains("invalid"));
    assert_eq!(sc.read()[5], [0, 0, 0, 255], "the clear still ran");
    assert!(
        sc.b.destroy(sc.tex).is_err(),
        "double close is reported, not UB"
    );
}

#[test]
fn push_constants_come_from_transient_memory() {
    let Some(sc) = scene() else { return };
    sc.b.write_host(
        sc.push,
        256,
        &f32s(&[0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0]),
    )
    .unwrap();
    let mut s = sc.b.lock();
    s.transient_dirty(sc.push, 256, 32).unwrap();
    s.write_buffer(sc.b.gpu, sc.enc, sc.ubo, 256, &f32s(&[0.0; 4]))
        .unwrap();
    let mut w = StreamWriter::default();
    sc.bind(&mut w, sc.fan, 256);
    w.push_constants(sc.push, 256, 16).draw(4, 1, 0, 0);
    let report = sc.pass(&mut s, &sc.desc(Some([0.0; 4])), &w);
    assert_eq!(report.skipped, 0, "{report:?}");
    drop(s);
    assert_eq!(
        sc.read()[0],
        [0, 0, 0, 0],
        "texture × zero color + zero push"
    );
}

#[test]
fn hostile_draws_are_skipped() {
    let Some(sc) = scene() else { return };
    let tri =
        sc.b.compile_pipeline(&info(TOPOLOGY_TRIANGLES, true).encode())
            .unwrap();
    let mut w = StreamWriter::default();
    sc.bind(&mut w, tri, 0);
    w.draw(1000, 1, 0, 0)
        .draw(3, 1, u32::MAX, 0)
        .draw_indexed(3, 1, 0, 0, 0)
        .index(sc.quad, true)
        .draw_indexed(1 << 20, 1, 0, 0, 0)
        .uniform_buffer(0, sc.ubo, 3, 16)
        .draw(3, 1, 0, 0)
        .uniform_buffer(0, sc.ubo, 0, 1 << 30)
        .draw(3, 1, 0, 0)
        .scissor(-5, -5, i32::MAX, i32::MAX)
        .uniform_buffer(0, sc.ubo, 0, 16)
        .draw(3, 1, 0, 0);
    let report = sc.pass(&mut sc.b.lock(), &sc.desc(Some([0.0; 4])), &w);
    assert_eq!((report.draws, report.skipped), (1, 6), "{report:?}");
    let mut s = sc.b.lock();
    let bad = [0x0Bu32, 1, 2];
    assert!(
        encode_pass(sc.b, &mut s, sc.enc, &sc.desc(None), &bad).is_err(),
        "truncated op"
    );
    assert!(
        encode_pass(sc.b, &mut s, sc.enc, &sc.desc(None), &[999]).is_err(),
        "unknown op"
    );
    let mut huge = vec![super::pass::P_MULTI_DRAW, u32::MAX, 1, 0];
    huge.extend([0; 8]);
    assert!(
        encode_pass(sc.b, &mut s, sc.enc, &sc.desc(None), &huge).is_err(),
        "count overruns"
    );
    drop(s);
    assert!(PassDesc::parse(&[9]).is_err());
    assert!(PassDesc::parse(&[u64::MAX]).is_err());
    sc.read();
}

#[test]
fn clears_cover_whole_textures_and_regions() {
    let Some(sc) = scene() else { return };
    let depth =
        sc.b.create_texture(TEXTURE_RENDER, 51, SIZE, SIZE, 1, 1)
            .unwrap();
    let mut s = sc.b.lock();
    s.clear(
        sc.b.gpu,
        sc.enc,
        Some((sc.target, [0.0, 0.0, 1.0, 1.0])),
        Some((depth, 0.0)),
        None,
    )
    .unwrap();
    s.clear_region(
        sc.b.gpu,
        sc.enc,
        (sc.target, [1.0, 0.0, 0.0, 1.0]),
        (depth, 1.0),
        1,
        1,
        2,
        2,
        0,
        false,
    )
    .unwrap();
    assert!(
        s.clear(sc.b.gpu, sc.enc, Some((depth, [0.0; 4])), None, None)
            .is_err()
    );
    assert!(
        s.clear_region(
            sc.b.gpu,
            sc.enc,
            (sc.target, [0.0; 4]),
            (depth, 1.0),
            3,
            3,
            2,
            2,
            0,
            false
        )
        .is_err()
    );
    drop(s);
    let px = sc.read();
    assert_eq!(px[0], [0, 0, 255, 255]);
    assert_eq!(px[5], [255, 0, 0, 255]);
    assert_eq!(px[15], [0, 0, 255, 255]);
}

#[test]
fn copies_validate_ranges() {
    let Some(sc) = scene() else { return };
    let mut s = sc.b.lock();
    let g = sc.b.gpu;
    assert!(s.write_buffer(g, sc.enc, sc.ubo, 510, &[1; 8]).is_err());
    assert!(s.write_buffer(g, sc.enc, sc.ubo, 2, &[1; 4]).is_err());
    assert!(
        s.copy_buffer(g, sc.enc, sc.ubo, 0, sc.ubo, 4, 16).is_err(),
        "overlap"
    );
    assert!(
        s.copy_buffer(g, sc.enc, sc.ubo, 0, sc.quad, 0, 1 << 20)
            .is_err()
    );
    assert!(
        s.write_texture(g, sc.enc, sc.tex, &[0; 16], 0, 0, 1, 1, 2, 2)
            .is_err()
    );
    assert!(
        s.write_texture(g, sc.enc, sc.tex, &[0; 4], 0, 0, 0, 0, 2, 2)
            .is_err()
    );
    assert!(
        s.write_texture(g, sc.enc, sc.tex, &[0; 16], 1, 0, 0, 0, 1, 1)
            .is_err()
    );
    assert!(
        s.copy_texture_to_buffer(g, sc.enc, sc.tex, sc.ubo, 500, 1, 0, 0, 0, 2, 2)
            .is_err()
    );
    s.write_buffer(g, sc.enc, sc.ubo, 0, &[7; 5]).unwrap();
    drop(s);
    sc.read();
}

#[test]
fn a_cut_pass_renders_like_the_uncut_one() {
    use super::pass::{PASS_CONTINUES, PASS_RESUMES};
    let Some(sc) = scene() else { return };
    let white =
        sc.b.create_texture(TEXTURE_RENDER, RGBA8, 1, 1, 1, 1)
            .unwrap();
    let white_view = sc.b.create_view(white, 0, 1).unwrap();
    sc.b.lock()
        .write_texture(
            sc.b.gpu,
            sc.enc,
            white,
            &[255, 0, 255, 255],
            0,
            0,
            0,
            0,
            1,
            1,
        )
        .unwrap();
    let mut head = StreamWriter::default();
    sc.bind(&mut head, sc.fan, 0);
    head.scissor(0, 0, 2, 4).draw(4, 1, 0, 0);
    let mut tail = StreamWriter::default();
    tail.uniform_texture(1, white_view, sc.sampler)
        .draw(4, 1, 0, 0);
    let clear = Some([0.0, 0.0, 0.0, 1.0]);

    let mut whole = head.clone();
    whole.words.extend(&tail.words);
    sc.pass(&mut sc.b.lock(), &sc.desc(clear), &whole);
    let uncut = sc.read();

    let green = Some([0.0, 1.0, 0.0, 1.0]);
    sc.pass(&mut sc.b.lock(), &sc.desc(green), &StreamWriter::default());
    let mut s = sc.b.lock();
    let mut first = sc.desc(clear);
    first.flags = PASS_CONTINUES;
    sc.pass(&mut s, &first, &head);
    s.split(sc.b.gpu, &sc.b.stats, &sc.b.slots, sc.enc).unwrap();
    let mut rest = sc.desc(green);
    rest.flags = PASS_RESUMES;
    let report = sc.pass(&mut s, &rest, &tail);
    drop(s);
    assert_eq!((report.draws, report.skipped), (1, 0), "{report:?}");
    let cut = sc.read();
    assert_eq!(
        uncut[0],
        [255, 0, 255, 255],
        "the second draw lands in the first scissor"
    );
    assert_eq!(uncut[3], [0, 0, 0, 255]);
    assert_eq!(
        cut, uncut,
        "pipeline, bindings, vertex buffer and scissor carry over the cut"
    );
}

#[test]
fn a_resume_without_a_cut_starts_from_scratch() {
    use super::pass::PASS_RESUMES;
    let Some(sc) = scene() else { return };
    let mut tail = StreamWriter::default();
    tail.draw(4, 1, 0, 0);
    let mut desc = sc.desc(None);
    desc.flags = PASS_RESUMES;
    let report = sc.pass(&mut sc.b.lock(), &desc, &tail);
    assert_eq!((report.draws, report.skipped), (0, 1), "{report:?}");
}

#[test]
fn rectangles_mirror_for_top_down_targets() {
    use super::pass::target_rect;
    let max = i64::from(i32::MAX);
    assert_eq!(
        target_rect([10, 0, 20, 5], (100, 50), false),
        (10, 0, 20, 5)
    );
    assert_eq!(
        target_rect([10, 0, 20, 5], (100, 50), true),
        (10, 45, 20, 5)
    );
    assert_eq!(
        target_rect([0, 40, 100, 20], (100, 50), true),
        (0, 0, 100, 10),
        "a rectangle past the top keeps its visible rows"
    );
    assert_eq!(
        target_rect([-5, -5, max, max], (100, 50), true),
        (0, 0, 100, 50)
    );
    assert_eq!(target_rect([0, 0, -3, 7], (100, 50), true), (0, 43, 0, 7));
}

#[test]
fn projected_pipelines_get_a_top_down_variant() {
    use super::pipeline::Pipeline;
    let Some(b) = gpu() else { return };
    let mut projected = info(TOPOLOGY_TRIANGLES, false);
    projected.uniforms[0].name = "Projection".into();
    let p = Pipeline::create(&b.gpu.device, &projected, false).unwrap();
    assert!(p.uses_projection);
    let mut key = p.declared_key(None);
    p.variant(&b.gpu.device, &key).unwrap();
    key.top_down = true;
    p.variant(&b.gpu.device, &key).unwrap();
    assert_eq!(p.variant_count(), 2);

    let plain = Pipeline::create(&b.gpu.device, &info(TOPOLOGY_TRIANGLES, false), false).unwrap();
    assert!(!plain.uses_projection);
    assert!(
        plain.variant(&b.gpu.device, &key).is_err(),
        "only projected pipelines have a top-down vertex stage"
    );
}
