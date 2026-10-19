use std::collections::HashMap;
use std::sync::Arc;

use mcr_assets::{
    BakedItemModel, BakedQuad, DisplayContext, Face, GuiLight, IDENTITY4, Mat4, Sprite, SpriteId,
    mat4_mul,
};

const LIGHT_POWER: f32 = 0.6;
const AMBIENT: f32 = 0.4;
const CUTOUT: u8 = 26;

type Vec3 = [f32; 3];
type Mat3 = [[f32; 3]; 3];

fn mat3_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn rot_x(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn rot_y(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn rot_z(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn mat3_apply(m: &Mat3, v: Vec3) -> Vec3 {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn normalize(v: Vec3) -> Vec3 {
    let len = v.iter().map(|c| c * c).sum::<f32>().sqrt();
    v.map(|c| c / len)
}

fn mat3_inverse_transpose(m: &Mat3) -> Mat3 {
    let c =
        |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let cof = [
        [c(1, 2, 1, 2), -c(1, 2, 0, 2), c(1, 2, 0, 1)],
        [-c(0, 2, 1, 2), c(0, 2, 0, 2), -c(0, 2, 0, 1)],
        [c(0, 1, 1, 2), -c(0, 1, 0, 2), c(0, 1, 0, 1)],
    ];
    let det = m[0][0] * cof[0][0] + m[0][1] * cof[0][1] + m[0][2] * cof[0][2];
    cof.map(|row| row.map(|v| v / det))
}

/// The two GUI light directions in view space, `(light0, light1)`.
fn gui_lights(light: GuiLight) -> (Vec3, Vec3) {
    let l0 = normalize([0.2, 1.0, -0.7]);
    let l1 = normalize([-0.2, 1.0, 0.7]);
    let pi = std::f32::consts::PI;
    let pose = match light {
        GuiLight::Front => mat3_mul(&rot_y(-pi / 8.0), &rot_x(pi * 3.0 / 4.0)),
        GuiLight::Side => {
            let flip = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
            let a = mat3_mul(&rot_y(1.0821041), &rot_x(3.2375858));
            let b = mat3_mul(&rot_y(-pi / 8.0), &rot_x(pi * 3.0 / 4.0));
            mat3_mul(&flip, &mat3_mul(&a, &b))
        }
    };
    (mat3_apply(&pose, l0), mat3_apply(&pose, l1))
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Display transform as a column-vector matrix: `T * Rx * Ry * Rz * S * T(-0.5)`.
fn display_matrix(rotation: Vec3, translation: Vec3, scale: Vec3) -> Mat4 {
    let d = std::f32::consts::PI / 180.0;
    let r = mat3_mul(
        &rot_x(rotation[0] * d),
        &mat3_mul(&rot_y(rotation[1] * d), &rot_z(rotation[2] * d)),
    );
    let mut out = IDENTITY4;
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = r[i][j] * scale[j];
        }
        out[i][3] = translation[i] - (0..3).map(|j| out[i][j]).sum::<f32>() * 0.5;
    }
    out
}

/// Animated enchantment glint sampled the way vanilla's `GLINT_TEXTURING` does.
#[derive(Clone, Debug)]
pub struct Glint {
    pub texture: Arc<Sprite>,
    pub millis: u64,
    pub speed: f32,
    pub strength: f32,
}

/// Fraction of the glint texture one item sprite spans, per vanilla's atlas-sized item UVs.
const GLINT_UV_SPAN: f32 = 16.0 / 512.0 * 8.0;

impl Glint {
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let ms = (self.millis as f64 * f64::from(self.speed) * 8.0) as u64;
        let o0 = (ms % 110_000) as f32 / 110_000.0;
        let o1 = (ms % 30_000) as f32 / 30_000.0;
        let (su, sv) = (u * GLINT_UV_SPAN, v * GLINT_UV_SPAN);
        let (s, c) = (std::f32::consts::PI / 18.0).sin_cos();
        let (gu, gv) = (-o0 + c * su - s * sv, o1 + s * su + c * sv);
        let t = &self.texture;
        let (w, h) = (t.width as usize, t.height as usize);
        let (x, y) = (gu * w as f32 - 0.5, gv * h as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |xi: f32, yi: f32| {
            let px = (xi as i64).rem_euclid(w as i64) as usize;
            let py = (yi as i64).rem_euclid(h as i64) as usize;
            let i = (py * w + px) * 4;
            let p = &t.frames[0][i..i + 3];
            [p[0], p[1], p[2]].map(|c| f32::from(c) / 255.0)
        };
        let (a, b, c2, d) = (
            at(x0, y0),
            at(x0 + 1.0, y0),
            at(x0, y0 + 1.0),
            at(x0 + 1.0, y0 + 1.0),
        );
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c2[i] + (d[i] - c2[i]) * fx;
            let tex = top + (bottom - top) * fy;
            let m = self.strength * tex;
            m * m
        })
    }
}

pub struct LayerDraw<'a> {
    pub model: &'a BakedItemModel,
    pub tints: &'a [u32],
    pub local: Mat4,
}

pub struct Raster<'a> {
    pub size: usize,
    pub images: &'a HashMap<SpriteId, Arc<Sprite>>,
    pub glint: Option<&'a Glint>,
}

struct Target {
    size: usize,
    color: Vec<u8>,
    depth: Vec<f32>,
}

#[derive(Clone, Copy)]
struct Screen {
    x: f32,
    y: f32,
    z: f32,
}

impl Raster<'_> {
    /// Renders layers of an item in the GUI into a `size`x`size` straight-alpha RGBA image.
    pub fn render(&self, layers: &[LayerDraw]) -> Vec<u8> {
        let mut target = Target {
            size: self.size,
            color: vec![0; self.size * self.size * 4],
            depth: vec![f32::NEG_INFINITY; self.size * self.size],
        };
        for layer in layers {
            self.layer(&mut target, layer);
        }
        target.color
    }

    fn layer(&self, target: &mut Target, layer: &LayerDraw) {
        let model = layer.model;
        let pose = match model.display(DisplayContext::Gui) {
            Some(t) => display_matrix(t.rotation, t.translation, t.scale),
            None => display_matrix([0.0; 3], [0.0; 3], [1.0; 3]),
        };
        let pose = mat4_mul(&pose, &layer.local);
        let size = self.size as f32;
        let linear: Mat3 = std::array::from_fn(|i| std::array::from_fn(|j| pose[i][j]));
        let flip = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
        let normal_matrix = mat3_inverse_transpose(&mat3_mul(&flip, &linear));
        let (l0, l1) = gui_lights(model.gui_light);
        for quad in model.quads.iter() {
            let n = Face::normal(quad.normal).map(f32::from);
            let n = normalize(mat3_apply(&normal_matrix, n));
            let light =
                ((dot(n, l0).max(0.0) + dot(n, l1).max(0.0)) * LIGHT_POWER + AMBIENT).min(1.0);
            let screen: [Screen; 4] = std::array::from_fn(|i| {
                let p = quad.vertices[i].pos;
                let v: Vec3 = std::array::from_fn(|r| {
                    (0..3).map(|c| pose[r][c] * p[c]).sum::<f32>() + pose[r][3]
                });
                Screen {
                    x: size / 2.0 + v[0] * size,
                    y: size / 2.0 - v[1] * size,
                    z: v[2] * size,
                }
            });
            self.quad(target, quad, &screen, light, layer.tints);
        }
    }

    fn quad(
        &self,
        target: &mut Target,
        quad: &BakedQuad,
        s: &[Screen; 4],
        light: f32,
        tints: &[u32],
    ) {
        let Some(image) = self.images.get(&quad.sprite) else {
            return;
        };
        let tint = usize::try_from(quad.tint)
            .ok()
            .and_then(|i| tints.get(i))
            .map_or([1.0; 3], |&c| {
                [(c >> 16) as u8, (c >> 8) as u8, c as u8].map(|b| f32::from(b) / 255.0)
            });
        let shade = tint.map(|c| c * light);
        for tri in [[0, 1, 2], [0, 2, 3]] {
            self.triangle(target, quad, s, tri, image, shade);
        }
    }

    fn triangle(
        &self,
        target: &mut Target,
        quad: &BakedQuad,
        s: &[Screen; 4],
        tri: [usize; 3],
        image: &Sprite,
        shade: [f32; 3],
    ) {
        let [a, b, c] = tri.map(|i| s[i]);
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if area.abs() < 1e-6 {
            return;
        }
        let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).ceil().max(0.0) as usize).min(target.size);
        let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).ceil().max(0.0) as usize).min(target.size);
        let uv = tri.map(|i| quad.vertices[i].uv);
        let (iw, ih) = (image.width as usize, image.height as usize);
        let frame = &image.frames[0];
        for py in min_y..max_y {
            for px in min_x..max_x {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let w0 = ((b.x - x) * (c.y - y) - (b.y - y) * (c.x - x)) / area;
                let w1 = ((c.x - x) * (a.y - y) - (c.y - y) * (a.x - x)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a.z + w1 * b.z + w2 * c.z;
                let slot = py * target.size + px;
                if z < target.depth[slot] - 1e-3 {
                    continue;
                }
                let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
                let v = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
                let tx = ((u * iw as f32).floor().max(0.0) as usize).min(iw - 1);
                let ty = ((v * ih as f32).floor().max(0.0) as usize).min(ih - 1);
                let t = &frame[(ty * iw + tx) * 4..][..4];
                if t[3] < CUTOUT {
                    continue;
                }
                let mut rgb: [f32; 3] = std::array::from_fn(|i| f32::from(t[i]) / 255.0 * shade[i]);
                if let Some(glint) = self.glint {
                    let g = glint.sample(u, v);
                    rgb = std::array::from_fn(|i| rgb[i] + g[i]);
                }
                let alpha = f32::from(t[3]) / 255.0;
                let out = &mut target.color[slot * 4..slot * 4 + 4];
                if t[3] == 255 {
                    target.depth[slot] = z;
                    for i in 0..3 {
                        out[i] = (rgb[i] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    }
                    out[3] = 255;
                } else {
                    let dst_a = f32::from(out[3]) / 255.0;
                    let out_a = alpha + dst_a * (1.0 - alpha);
                    for i in 0..3 {
                        let d = f32::from(out[i]) / 255.0;
                        let v = (rgb[i] * alpha + d * dst_a * (1.0 - alpha)) / out_a.max(1e-6);
                        out[i] = (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                    }
                    out[3] = (out_a * 255.0 + 0.5) as u8;
                }
            }
        }
    }
}
