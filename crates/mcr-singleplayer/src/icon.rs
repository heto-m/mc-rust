//! World icons: a frame cropped to its centre square, scaled to 64×64 and written as `icon.png`.

use std::path::Path;

use crate::Result;
use crate::level::ICON_SIZE;

const SIDE: usize = ICON_SIZE as usize;

/// The centre square of a `width`×`height` RGBA (or BGRA with `bgra`) frame, box-filtered to 64×64 RGBA.
pub fn thumbnail(width: u32, height: u32, pixels: &[u8], bgra: bool) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || pixels.len() < w * h * 4 {
        return None;
    }
    let side = w.min(h);
    let (x0, y0) = ((w - side) / 2, (h - side) / 2);
    let mut out = vec![0u8; SIDE * SIDE * 4];
    for oy in 0..SIDE {
        let (sy0, sy1) = (
            y0 + oy * side / SIDE,
            y0 + ((oy + 1) * side / SIDE).max(oy * side / SIDE + 1),
        );
        for ox in 0..SIDE {
            let (sx0, sx1) = (
                x0 + ox * side / SIDE,
                x0 + ((ox + 1) * side / SIDE).max(ox * side / SIDE + 1),
            );
            let mut sum = [0u64; 3];
            let mut n = 0u64;
            for sy in sy0..sy1.min(h) {
                let row = &pixels[(sy * w + sx0) * 4..(sy * w + sx1.min(w)) * 4];
                for p in row.as_chunks::<4>().0 {
                    sum[0] += u64::from(p[0]);
                    sum[1] += u64::from(p[1]);
                    sum[2] += u64::from(p[2]);
                    n += 1;
                }
            }
            let n = n.max(1);
            let (r, b) = if bgra {
                (sum[2], sum[0])
            } else {
                (sum[0], sum[2])
            };
            let o = (oy * SIDE + ox) * 4;
            out[o..o + 4].copy_from_slice(&[(r / n) as u8, (sum[1] / n) as u8, (b / n) as u8, 255]);
        }
    }
    Some(out)
}

pub fn encode_png(rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != SIDE * SIDE * 4 {
        return None;
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, ICON_SIZE, ICON_SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Writes `rgba` as `<world>/icon.png` unless one exists; `Ok(false)` when it was already there.
pub fn write_if_missing(saves: &Path, folder: &str, rgba: &[u8]) -> Result<bool> {
    let world = crate::safe::checked_world(saves, folder)?;
    let path = world.join("icon.png");
    match std::fs::symlink_metadata(&path) {
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let png = encode_png(rgba).ok_or_else(|| crate::SpError::data("not a 64×64 icon"))?;
    crate::safe::write_file(&path, &png)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_centre_and_scales() {
        let (w, h) = (300u32, 100u32);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let inside = (100..200).contains(&x);
                frame[i..i + 4].copy_from_slice(&if inside { [10, 20, 30, 0] } else { [255; 4] });
            }
        }
        let icon = thumbnail(w, h, &frame, false).unwrap();
        assert_eq!(icon.len(), 64 * 64 * 4);
        assert!(
            icon.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p == &[10, 20, 30, 255]),
            "only the centre square"
        );
        let swapped = thumbnail(w, h, &frame, true).unwrap();
        assert_eq!(&swapped[..4], &[30, 20, 10, 255]);
        assert!(thumbnail(10, 10, &frame[..10], false).is_none());
        let tiny = thumbnail(2, 2, &[9; 16], false).unwrap();
        assert_eq!(&tiny[..4], &[9, 9, 9, 255], "upscales small frames");
    }

    #[test]
    fn written_once_and_read_back() {
        let saves = crate::level::tests::temp_dir("icon-write");
        std::fs::create_dir(saves.join("W")).unwrap();
        let icon = vec![77u8; 64 * 64 * 4];
        assert!(write_if_missing(&saves, "W", &icon).unwrap());
        assert!(!write_if_missing(&saves, "W", &[0; 64 * 64 * 4]).unwrap());
        let back = crate::level::read_icon(&saves.join("W/icon.png")).unwrap();
        assert_eq!(&back[..4], &[77, 77, 77, 77]);
        assert!(write_if_missing(&saves, "..", &icon).is_err());
        let _ = std::fs::remove_dir_all(&saves);
    }

    #[cfg(unix)]
    #[test]
    fn planted_link_is_left_alone() {
        let root = crate::level::tests::temp_dir("icon-link");
        let saves = root.join("saves");
        std::fs::create_dir_all(saves.join("W")).unwrap();
        std::os::unix::fs::symlink(root.join("victim"), saves.join("W/icon.png")).unwrap();
        assert!(!write_if_missing(&saves, "W", &[1; 64 * 64 * 4]).unwrap());
        assert!(!root.join("victim").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
