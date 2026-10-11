use std::path::Path;

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn load_png(path: &Path) -> Rgba {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    let data = &buf[..frame.buffer_size()];
    let pixels = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("{}: unexpected {other:?}", path.display()),
    };
    Rgba {
        width: frame.width,
        height: frame.height,
        pixels,
    }
}

/// Mean SSIM of Rec. 601 luma over 8×8 windows with a 4-pixel stride.
pub fn ssim(a: &Rgba, b: &Rgba) -> f64 {
    assert_eq!(
        (a.width, a.height),
        (b.width, b.height),
        "image sizes differ"
    );
    let luma = |img: &Rgba| -> Vec<f64> {
        img.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
            .collect()
    };
    let (la, lb) = (luma(a), luma(b));
    let (w, h) = (a.width as usize, a.height as usize);
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let (mut total, mut windows) = (0.0, 0usize);
    for y in (0..=h - 8).step_by(4) {
        for x in (0..=w - 8).step_by(4) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for dy in 0..8 {
                for dx in 0..8 {
                    let i = (y + dy) * w + x + dx;
                    let (p, q) = (la[i], lb[i]);
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            let n = 64.0;
            let (ma, mb) = (sa / n, sb / n);
            let (va, vb, cov) = (saa / n - ma * ma, sbb / n - mb * mb, sab / n - ma * mb);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            windows += 1;
        }
    }
    total / windows as f64
}

pub fn save_png(path: &Path, img: &Rgba) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&img.pixels)
        .unwrap();
}
