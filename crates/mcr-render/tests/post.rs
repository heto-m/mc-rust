use mcr_render::{
    Camera, FrameInputs, GlowOutline, Overlay, OverlayCtx, PostChain, PostStack, RenderError,
    Renderer, RendererConfig, Target,
};

const W: u32 = 64;
const H: u32 = 66;

fn renderer(w: u32, h: u32) -> Option<Renderer> {
    match Renderer::new(
        Target::Offscreen {
            width: w,
            height: h,
        },
        RendererConfig::default(),
    ) {
        Ok(r) => Some(r),
        Err(RenderError::NoAdapter(why)) => {
            eprintln!("skipping GPU test: {why}");
            None
        }
        Err(e) => panic!("renderer creation failed: {e}"),
    }
}

fn image(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("post test"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture) -> Vec<u8> {
    let row = (W * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * H),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(H),
            },
        },
        tex.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = buffer.slice(..).get_mapped_range().expect("mapped");
    (0..H)
        .flat_map(|y| mapped[(y * row) as usize..(y * row + W * 4) as usize].to_vec())
        .collect()
}

/// Runs `chains` over an image whose pixel `(x, y)` (top row first) is `pattern(x, y)`.
fn run(chains: &[PostChain], pattern: impl Fn(u32, u32) -> [u8; 4]) -> Option<Vec<u8>> {
    let r = renderer(8, 8)?;
    let (device, queue) = (r.device(), r.queue());
    let tex = image(device);
    let pattern = &pattern;
    let pixels: Vec<u8> = (0..H)
        .flat_map(|y| (0..W).flat_map(move |x| pattern(x, y)))
        .collect();
    queue.write_texture(
        tex.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(W * 4),
            rows_per_image: Some(H),
        },
        tex.size(),
    );
    let mut stack = PostStack::new(device, wgpu::TextureFormat::Rgba8Unorm);
    let mut encoder = device.create_command_encoder(&Default::default());
    stack.record(device, queue, &mut encoder, &tex, chains, 5);
    queue.submit([encoder.finish()]);
    Some(read(device, queue, &tex))
}

fn px(img: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [img[i], img[i + 1], img[i + 2]]
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 1)
}

const BASE: [u8; 4] = [51, 102, 153, 255];

#[test]
fn invert_mixes_eighty_percent_towards_the_complement() {
    let Some(img) = run(&[PostChain::Invert], |_, _| BASE) else {
        return;
    };
    for (x, y) in [(0, 0), (31, 40), (63, 65)] {
        assert!(
            close(px(&img, x, y), [173, 143, 112]),
            "{:?}",
            px(&img, x, y)
        );
    }
}

fn unorm(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

#[test]
fn creeper_keeps_quantised_saturated_green() {
    let Some(img) = run(&[PostChain::Creeper], |_, _| BASE) else {
        return;
    };
    let c = BASE.map(|v| f32::from(v) / 255.0);
    let luma = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let green = unorm(luma * (0.41 * 1.8 + 0.59));
    let level = (green * 16.0).floor() / 16.0;
    let expected = (level * (0.41 * 1.5 + 0.59) * 255.0).round() as u8;
    assert!(
        close(px(&img, 10, 10), [0, expected, 0]),
        "{:?} vs {expected}",
        px(&img, 10, 10)
    );
}

#[test]
fn creeper_mosaic_cells_anchor_at_the_bottom_like_vanilla() {
    let Some(img) = run(&[PostChain::Creeper], |_, y| {
        if y == 5 { [255; 4] } else { [0, 0, 0, 255] }
    }) else {
        return;
    };
    // H is 66: bottom-anchored 4-row cells put top rows 2..=5 in the cell that samples row 5.
    for y in 0..H {
        let lit = px(&img, 9, y)[1] > 200;
        assert_eq!(lit, (2..=5).contains(&y), "row {y}: {:?}", px(&img, 9, y));
    }
}

#[test]
fn spider_tints_a_uniform_image() {
    let Some(img) = run(&[PostChain::Spider], |_, _| BASE) else {
        return;
    };
    for (x, y) in [(0, 0), (32, 33), (63, 65), (10, 50)] {
        assert!(close(px(&img, x, y), [51, 82, 122]), "{:?}", px(&img, x, y));
    }
}

fn sharpness(img: &[u8], rows: std::ops::Range<u32>) -> u32 {
    rows.flat_map(|y| (1..W).map(move |x| (x, y)))
        .map(|(x, y)| u32::from(px(img, x, y)[0].abs_diff(px(img, x - 1, y)[0])))
        .sum()
}

#[test]
fn spider_main_eye_sits_in_the_lower_half() {
    let Some(img) = run(&[PostChain::Spider], |x, y| {
        if (x + y) % 2 == 0 {
            [255; 4]
        } else {
            [0, 0, 0, 255]
        }
    }) else {
        return;
    };
    let top = sharpness(&img, 0..H / 3);
    let bottom = sharpness(&img, H / 2..H * 5 / 6);
    assert!(bottom > top * 4, "bottom {bottom} top {top}");
}

#[test]
fn blur_keeps_flat_colour_and_softens_edges() {
    let Some(img) = run(&[PostChain::Blur], |x, _| {
        if x < W / 2 { [255; 4] } else { [0, 0, 0, 255] }
    }) else {
        return;
    };
    assert!(close(px(&img, 2, 30), [255; 3]));
    assert!(close(px(&img, 61, 30), [0; 3]));
    let edge = px(&img, W / 2, 30)[0];
    assert!((40..215).contains(&edge), "edge {edge}");
    let Some(flat) = run(&[PostChain::Blur], |_, _| BASE) else {
        return;
    };
    assert!(close(px(&flat, 30, 30), [51, 102, 153]));
}

#[test]
fn chains_compose_in_order() {
    let Some(img) = run(&[PostChain::Invert, PostChain::Invert], |_, _| BASE) else {
        return;
    };
    let once = [173.0f32, 143.0, 112.0].map(|v| v / 255.0);
    let twice = once.map(|c| ((c + 0.8 * (1.0 - 2.0 * c)) * 255.0).round() as u8);
    assert!(close(px(&img, 5, 5), twice), "{:?}", px(&img, 5, 5));
}

const FLAT_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
";

/// Paints the world one colour and a green UI strip over the left quarter.
struct Paint {
    ui: Option<wgpu::RenderPipeline>,
}

impl Overlay for Paint {
    fn record(&mut self, ctx: &mut OverlayCtx<'_>) {
        ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.2,
                        g: 0.4,
                        b: 0.6,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }

    fn record_ui(&mut self, ctx: &mut OverlayCtx<'_>) {
        let pipeline = self.ui.get_or_insert_with(|| {
            let module = ctx
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: None,
                    source: wgpu::ShaderSource::Wgsl(FLAT_WGSL.into()),
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
                    depth_stencil: None,
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
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_scissor_rect(0, 0, ctx.size.0 / 4, ctx.size.1);
        pass.draw(0..3, 0..1);
    }
}

#[test]
fn renderer_runs_chains_after_the_world_and_before_the_ui() {
    let Some(mut r) = renderer(W, H) else {
        return;
    };
    let mut paint = Paint { ui: None };
    let camera = Camera::default();
    let inputs = FrameInputs {
        camera: &camera,
        sky_light: 1.0,
    };
    r.set_post_effects(&[PostChain::Invert]);
    r.render_with_overlay(&inputs, Some(&mut paint)).unwrap();
    let img = r.read_offscreen().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * W + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2]]
    };
    assert!(close(at(40, 30), [173, 143, 112]), "world {:?}", at(40, 30));
    assert_eq!(
        at(3, 30),
        [0, 255, 0],
        "the UI strip is drawn after the chain"
    );

    r.set_post_effects(&[]);
    r.render_with_overlay(&inputs, Some(&mut paint)).unwrap();
    let img = r.read_offscreen().unwrap();
    let i = ((30 * W + 40) * 4) as usize;
    assert!(close([img[i], img[i + 1], img[i + 2]], [51, 102, 153]));
}

const RED_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.5, 1.0);
}
@fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }
";

#[test]
fn glow_outlines_the_silhouette_edge_only() {
    let Some(r) = renderer(8, 8) else {
        return;
    };
    let (device, queue) = (r.device(), r.queue());
    let target = image(device);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(RED_WGSL.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            format: GlowOutline::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(GlowOutline::COLOR_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
    let mut glow = GlowOutline::new(device, wgpu::TextureFormat::Rgba8Unorm);
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = glow.begin(device, &mut encoder, (W, H));
        pass.set_pipeline(&pipeline);
        pass.set_scissor_rect(16, 16, 32, 32);
        pass.draw(0..3, 0..1);
    }
    let view = target.create_view(&Default::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    glow.finish(device, queue, &mut encoder, &view);
    queue.submit([encoder.finish()]);
    let img = read(device, queue, &target);
    assert_eq!(px(&img, 32, 32), [0, 0, 0], "inside stays untouched");
    assert_eq!(px(&img, 2, 2), [0, 0, 0], "far outside stays untouched");
    let edge = px(&img, 16, 32);
    assert!(
        edge[0] > 60 && edge[1] == 0 && edge[2] == 0,
        "edge {edge:?}"
    );
}
