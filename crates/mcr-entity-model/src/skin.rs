use crate::assets::ImageRgba;

/// Turns a downloaded skin into the 64x64 layout the models expect, as vanilla does for legacy skins.
pub fn prepare(width: u32, height: u32, rgba: &[u8]) -> ImageRgba {
    let mut img = if width == 0
        || height == 0
        || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4
    {
        ImageRgba::solid(64, 64, [0, 0, 0, 0])
    } else {
        ImageRgba {
            width,
            height,
            rgba: rgba.to_vec(),
        }
    };
    if img.width == 64 && img.height == 32 {
        img = upgrade_legacy(&img);
    }
    if img.width >= 64 && img.height >= 64 {
        let s = img.width / 64;
        opaque(&mut img, 0, 0, 32 * s, 16 * s);
        opaque(&mut img, 0, 16 * s, 64 * s, 32 * s);
        opaque(&mut img, 16 * s, 48 * s, 48 * s, 64 * s);
    }
    img
}

/// A downloaded cape as-is; mismatched pixel data becomes a transparent 64x32 image.
pub fn prepare_cape(width: u32, height: u32, rgba: &[u8]) -> ImageRgba {
    if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return ImageRgba::solid(64, 32, [0, 0, 0, 0]);
    }
    ImageRgba {
        width,
        height,
        rgba: rgba.to_vec(),
    }
}

fn opaque(img: &mut ImageRgba, x0: u32, y0: u32, x1: u32, y1: u32) {
    for y in y0..y1.min(img.height) {
        for x in x0..x1.min(img.width) {
            let i = ((y * img.width + x) * 4 + 3) as usize;
            img.rgba[i] = 255;
        }
    }
}

fn upgrade_legacy(src: &ImageRgba) -> ImageRgba {
    let mut out = ImageRgba::solid(64, 64, [0, 0, 0, 0]);
    out.rgba[..src.rgba.len()].copy_from_slice(&src.rgba);
    const COPIES: [(i32, i32, i32, i32, i32, i32); 12] = [
        (4, 16, 16, 32, 4, 4),
        (8, 16, 16, 32, 4, 4),
        (0, 20, 24, 32, 4, 12),
        (4, 20, 16, 32, 4, 12),
        (8, 20, 8, 32, 4, 12),
        (12, 20, 16, 32, 4, 12),
        (44, 16, -8, 32, 4, 4),
        (48, 16, -8, 32, 4, 4),
        (40, 20, 0, 32, 4, 12),
        (44, 20, -8, 32, 4, 12),
        (48, 20, -16, 32, 4, 12),
        (52, 20, -8, 32, 4, 12),
    ];
    for (x, y, xo, yo, w, h) in COPIES {
        for j in 0..h {
            for i in 0..w {
                let (sx, sy) = ((x + i) as u32, (y + j) as u32);
                let (dx, dy) = ((x + xo + (w - 1 - i)) as u32, (y + yo + j) as u32);
                if dx < 64 && dy < 64 {
                    let p = out.pixel(sx, sy);
                    out.set(dx, dy, p);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_skin_gets_mirrored_left_leg() {
        let mut src = ImageRgba::solid(64, 32, [0, 0, 0, 0]);
        src.set(4, 16, [9, 8, 7, 255]);
        let out = prepare(64, 32, &src.rgba);
        assert_eq!(out.height, 64);
        assert_eq!(out.pixel(23, 48), [9, 8, 7, 255]);
    }

    #[test]
    fn hostile_sizes_fall_back_to_blank() {
        let out = prepare(1 << 30, 1 << 30, &[0; 4]);
        assert_eq!((out.width, out.height), (64, 64));
    }
}
