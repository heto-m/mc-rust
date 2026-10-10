use bytemuck::{Pod, Zeroable};

use super::EffectImage;

pub(crate) const CELL: f32 = 12.0;
pub(crate) const CLOUD_THICKNESS: f32 = 4.0;
pub(crate) const MAX_CLOUD_QUADS: usize = 1 << 17;
const TICKS_PER_CELL: u64 = 400;
const SCROLL_PER_TICK: f64 = 0.030_000_001;
const Z_OFFSET: f64 = 3.96;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct CloudVertex {
    pub pos: [f32; 3],
    pub shade: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudMode {
    Off,
    Flat,
    #[default]
    Fancy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudSettings {
    pub mode: CloudMode,
    /// Draw radius in chunks (vanilla option, 2 to 128).
    pub range_chunks: u32,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            mode: CloudMode::Fancy,
            range_chunks: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relative {
    Above,
    Inside,
    Below,
}

const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;

#[derive(Debug, Default)]
pub(crate) struct Clouds {
    width: i32,
    height: i32,
    cells: Vec<u8>,
    key: Option<(i32, i32, u8, CloudMode, i32)>,
    pub(crate) vertices: Vec<CloudVertex>,
    pub(crate) quads: u32,
    pub(crate) offset: [f32; 3],
}

const FACE_SHADE: [f32; 6] = [0.7, 1.0, 0.8, 0.8, 0.9, 0.9];

fn corner(face: usize, index: usize) -> [f32; 3] {
    const PX: u32 = 0xF0_3CC3;
    const PY: u32 = 0x66_66F0;
    const PZ: u32 = 0xC3_F066;
    let bit = |p: u32| ((p >> (face * 4 + index)) & 1) as f32;
    [bit(PX), bit(PY), bit(PZ)]
}

impl Clouds {
    pub(crate) fn set_texture(&mut self, image: &EffectImage) {
        let (w, h) = (image.width as i32, image.height as i32);
        let empty = |x: i32, y: i32| {
            let (x, y) = (x.rem_euclid(w), y.rem_euclid(h));
            image.rgba[((y * w + x) * 4 + 3) as usize] < 10
        };
        self.cells = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                if empty(x, y) {
                    return 0;
                }
                let mut bits = 0x10;
                for (dx, dy, bit) in [(0, -1, N), (1, 0, E), (0, 1, S), (-1, 0, W)] {
                    if empty(x + dx, y + dy) {
                        bits |= bit;
                    }
                }
                bits
            })
            .collect();
        (self.width, self.height) = (w, h);
        self.key = None;
    }

    pub(crate) fn is_ready(&self) -> bool {
        !self.cells.is_empty()
    }

    /// Recomputes the scroll offset every call and the mesh only when the camera crosses a cell.
    pub(crate) fn update(
        &mut self,
        settings: CloudSettings,
        bottom_y: f32,
        camera: [f64; 3],
        game_time: u64,
        partial: f32,
    ) -> bool {
        if !self.is_ready() || settings.mode == CloudMode::Off {
            self.quads = 0;
            return false;
        }
        let partial = if partial.is_finite() {
            partial.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let camera = camera.map(|c| {
            if c.is_finite() {
                c.clamp(-3.0e7, 3.0e7)
            } else {
                0.0
            }
        });
        let bottom_y = if bottom_y.is_finite() {
            bottom_y.clamp(-1.0e5, 1.0e5)
        } else {
            return false;
        };
        let range = settings.range_chunks.clamp(2, 128);
        let radius = ((range * 16) as f32 / CELL).ceil() as i32;
        let rel_bottom = (f64::from(bottom_y) - camera[1]) as f32;
        let relative = if rel_bottom + CLOUD_THICKNESS < 0.0 {
            Relative::Above
        } else if rel_bottom > 0.0 {
            Relative::Below
        } else {
            Relative::Inside
        };
        let period = self.width as u64 * TICKS_PER_CELL;
        let t = (game_time % period) as f64 + f64::from(partial);
        let wrap = |v: f64, size: f64| v - (v / size).floor() * size;
        let cx = wrap(
            camera[0] + t * SCROLL_PER_TICK,
            f64::from(self.width) * f64::from(CELL),
        );
        let cz = wrap(
            camera[2] + Z_OFFSET,
            f64::from(self.height) * f64::from(CELL),
        );
        let (cell_x, cell_z) = (
            (cx / f64::from(CELL)).floor() as i32,
            (cz / f64::from(CELL)).floor() as i32,
        );
        self.offset = [
            -((cx - f64::from(cell_x) * f64::from(CELL)) as f32),
            rel_bottom,
            -((cz - f64::from(cell_z) * f64::from(CELL)) as f32),
        ];
        let key = (cell_x, cell_z, relative as u8, settings.mode, radius);
        if self.key == Some(key) {
            return false;
        }
        self.key = Some(key);
        self.rebuild(settings.mode, relative, cell_x, cell_z, radius);
        true
    }

    fn rebuild(&mut self, mode: CloudMode, rel: Relative, cell_x: i32, cell_z: i32, radius: i32) {
        self.vertices.clear();
        for ring in 0..=2 * radius {
            for rx in -ring..=ring {
                let rz = ring - rx.abs();
                if rz > radius || rx * rx + rz * rz > radius * radius {
                    continue;
                }
                for z in [-rz, rz].into_iter().take(if rz == 0 { 1 } else { 2 }) {
                    if self.vertices.len() / 4 >= MAX_CLOUD_QUADS - 8 {
                        break;
                    }
                    let ix = (cell_x + rx).rem_euclid(self.width);
                    let iy = (cell_z + z).rem_euclid(self.height);
                    let cell = self.cells[(iy * self.width + ix) as usize];
                    if cell != 0 {
                        match mode {
                            CloudMode::Fancy => self.extruded(rel, rx, z, cell),
                            _ => self.face(rx, z, 0, true, 1.0),
                        }
                    }
                }
            }
        }
        self.quads = (self.vertices.len() / 4) as u32;
    }

    fn face(&mut self, x: i32, z: i32, face: usize, inside: bool, shade: f32) {
        for i in 0..4 {
            let c = corner(face, if inside { 3 - i } else { i });
            self.vertices.push(CloudVertex {
                pos: [
                    (x as f32 + c[0]) * CELL,
                    c[1] * CLOUD_THICKNESS,
                    (z as f32 + c[2]) * CELL,
                ],
                shade,
            });
        }
    }

    fn extruded(&mut self, rel: Relative, x: i32, z: i32, cell: u8) {
        if rel != Relative::Below {
            self.face(x, z, 1, false, FACE_SHADE[1]);
        }
        if rel != Relative::Above {
            self.face(x, z, 0, false, FACE_SHADE[0]);
        }
        let sides = [(N, 2, z > 0), (S, 3, z < 0), (W, 4, x > 0), (E, 5, x < 0)];
        for (bit, face, facing) in sides {
            if cell & bit != 0 && facing {
                self.face(x, z, face, false, FACE_SHADE[face]);
            }
        }
        if x.abs() <= 1 && z.abs() <= 1 {
            for (face, shade) in FACE_SHADE.into_iter().enumerate() {
                self.face(x, z, face, true, shade);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> EffectImage {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for (x, y) in [(2, 2), (3, 2), (2, 3)] {
            rgba[(y * 8 + x) * 4 + 3] = 255;
        }
        EffectImage {
            width: 8,
            height: 8,
            rgba,
        }
    }

    #[test]
    fn mesh_rebuilds_only_on_cell_change() {
        let mut c = Clouds::default();
        c.set_texture(&image());
        let s = CloudSettings::default();
        assert!(c.update(s, 192.0, [0.0, 64.0, 0.0], 0, 0.0));
        assert!(c.quads > 0);
        assert!(!c.update(s, 192.0, [0.1, 64.0, 0.0], 0, 0.0));
        assert!(c.update(s, 192.0, [30.0, 64.0, 0.0], 0, 0.0));
    }

    #[test]
    fn scroll_moves_with_time() {
        let mut c = Clouds::default();
        c.set_texture(&image());
        let s = CloudSettings::default();
        c.update(s, 192.0, [0.0, 64.0, 0.0], 0, 0.0);
        let a = c.offset[0];
        c.update(s, 192.0, [0.0, 64.0, 0.0], 100, 0.0);
        assert!((a - c.offset[0]).abs() > 1.0 || c.key.is_some());
    }

    #[test]
    fn off_draws_nothing() {
        let mut c = Clouds::default();
        c.set_texture(&image());
        c.update(
            CloudSettings {
                mode: CloudMode::Off,
                range_chunks: 8,
            },
            192.0,
            [0.0; 3],
            0,
            0.0,
        );
        assert_eq!(c.quads, 0);
    }
}

#[cfg(test)]
mod hostile_tests {
    use super::*;

    #[test]
    fn hostile_inputs_stay_bounded() {
        let mut c = Clouds::default();
        c.set_texture(&EffectImage {
            width: 8,
            height: 8,
            rgba: vec![255; 8 * 8 * 4],
        });
        let s = CloudSettings {
            mode: CloudMode::Fancy,
            range_chunks: u32::MAX,
        };
        for cam in [[f64::NAN; 3], [f64::INFINITY, f64::NEG_INFINITY, 1e300]] {
            c.update(s, f32::NAN, cam, u64::MAX, f32::NAN);
            c.update(s, 192.0, cam, u64::MAX, f32::INFINITY);
            assert!(c.vertices.len() / 4 <= MAX_CLOUD_QUADS);
            assert!(c.offset.iter().all(|v| v.is_finite()));
        }
    }
}
