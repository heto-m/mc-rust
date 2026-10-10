use std::sync::Arc;

use crate::bake::{BakedQuad, SpriteId, Vertex};
use crate::error::Result;
use crate::geometry::Face;
use crate::identifier::Identifier;
use crate::item_bake::{BakedItemModel, ItemBaker};
use crate::item_def::SpecialKind;
use crate::model::GuiLight;
use crate::resources::ResourceManager;

const DYE_RGB: [(&str, u32); 16] = [
    ("white", 0xF9FFFE),
    ("orange", 0xF9801D),
    ("magenta", 0xC74EBD),
    ("light_blue", 0x3AB3DA),
    ("yellow", 0xFED83D),
    ("lime", 0x80C71F),
    ("pink", 0xF38BAA),
    ("gray", 0x474F52),
    ("light_gray", 0x9D9D97),
    ("cyan", 0x169C9C),
    ("purple", 0x8932B8),
    ("blue", 0x3C44AA),
    ("brown", 0x835432),
    ("green", 0x5E7C16),
    ("red", 0xB02E26),
    ("black", 0x1D1D21),
];

/// A cuboid in block-space pixels (0..16) textured with the vanilla entity box UV layout.
struct EntityBox {
    from: [f32; 3],
    to: [f32; 3],
    tex: (f32, f32),
    grow: f32,
}

fn bake_box(b: &EntityBox, sprite: SpriteId, tex_size: (f32, f32), out: &mut Vec<BakedQuad>) {
    let [x0, y0, z0] = b.from.map(|c| c - b.grow);
    let [x1, y1, z1] = [b.to[0] + b.grow, b.to[1] + b.grow, b.to[2] + b.grow];
    let (w, h, d) = (
        b.to[0] - b.from[0],
        b.to[1] - b.from[1],
        b.to[2] - b.from[2],
    );
    let (u, v) = b.tex;
    let (u1, u2, u3, u4) = (u + d, u + d + w, u + 2.0 * d + w, u + 2.0 * d + 2.0 * w);
    let (v1, v2) = (v + d, v + d + h);
    let faces: [(Face, [f32; 4], [[f32; 3]; 4]); 6] = [
        (
            Face::North,
            [u1, v1, u2, v2],
            [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        ),
        (
            Face::South,
            [u3, v1, u4, v2],
            [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        ),
        (
            Face::East,
            [u2, v1, u3, v2],
            [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
        ),
        (
            Face::West,
            [u, v1, u1, v2],
            [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        ),
        (
            Face::Up,
            [u1, v, u2, v1],
            [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        ),
        (
            Face::Down,
            [u2, v1, u3, v],
            [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        ),
    ];
    for (face, uv, corners) in faces {
        let corner_uv = [
            [uv[0], uv[1]],
            [uv[0], uv[3]],
            [uv[2], uv[3]],
            [uv[2], uv[1]],
        ];
        let vertices = std::array::from_fn(|i| Vertex {
            pos: corners[i].map(|c| c / 16.0),
            uv: [corner_uv[i][0] / tex_size.0, corner_uv[i][1] / tex_size.1],
        });
        out.push(quad(vertices, sprite, face));
    }
}

fn quad(vertices: [Vertex; 4], sprite: SpriteId, normal: Face) -> BakedQuad {
    BakedQuad {
        vertices,
        sprite,
        normal,
        cull: None,
        tint: 0,
        shade_dir: normal,
        light_emission: 0,
        force_translucent: false,
    }
}

fn flat_quad(rect: [f32; 4], uv: [f32; 4], sprite: SpriteId) -> BakedQuad {
    let [x0, y0, x1, y1] = rect.map(|c| c / 16.0);
    let z = 0.5;
    let vertices = [
        Vertex {
            pos: [x0, y1, z],
            uv: [uv[0], uv[1]],
        },
        Vertex {
            pos: [x0, y0, z],
            uv: [uv[0], uv[3]],
        },
        Vertex {
            pos: [x1, y0, z],
            uv: [uv[2], uv[3]],
        },
        Vertex {
            pos: [x1, y1, z],
            uv: [uv[2], uv[1]],
        },
    ];
    quad(vertices, sprite, Face::South)
}

fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

impl ItemBaker {
    fn entity_texture(
        &mut self,
        resources: &ResourceManager,
        path: &str,
    ) -> Result<(SpriteId, (f32, f32))> {
        let id = self.intern(&Identifier::parse(path));
        let image = self.image(resources, id)?;
        Ok((id, (image.width as f32, image.height as f32)))
    }

    /// Geometry approximating a special item model's entity renderer, built from entity textures.
    /// Unsupported kinds fall back to the base model's particle texture as a flat icon.
    pub fn bake_special(
        &mut self,
        resources: &ResourceManager,
        base: &BakedItemModel,
        kind: &SpecialKind,
    ) -> Result<Arc<BakedItemModel>> {
        let mut quads = Vec::new();
        let mut tints = Vec::new();
        let mut keep_display = true;
        let mut gui_light = base.gui_light;
        match kind {
            SpecialKind::Chest { texture } => {
                let (s, size) =
                    self.entity_texture(resources, &format!("entity/chest/{}", strip_ns(texture)))?;
                for b in [
                    EntityBox {
                        from: [1.0, 0.0, 1.0],
                        to: [15.0, 10.0, 15.0],
                        tex: (0.0, 19.0),
                        grow: 0.0,
                    },
                    EntityBox {
                        from: [1.0, 10.0, 1.0],
                        to: [15.0, 15.0, 15.0],
                        tex: (0.0, 0.0),
                        grow: 0.0,
                    },
                    EntityBox {
                        from: [7.0, 8.0, 0.0],
                        to: [9.0, 12.0, 1.0],
                        tex: (0.0, 0.0),
                        grow: 0.0,
                    },
                ] {
                    bake_box(&b, s, size, &mut quads);
                }
            }
            SpecialKind::ShulkerBox { texture } => {
                let (s, size) = self
                    .entity_texture(resources, &format!("entity/shulker/{}", strip_ns(texture)))?;
                for b in [
                    EntityBox {
                        from: [0.0, 0.0, 0.0],
                        to: [16.0, 8.0, 16.0],
                        tex: (0.0, 28.0),
                        grow: 0.0,
                    },
                    EntityBox {
                        from: [0.0, 4.0, 0.0],
                        to: [16.0, 16.0, 16.0],
                        tex: (0.0, 0.0),
                        grow: 0.0,
                    },
                ] {
                    bake_box(&b, s, size, &mut quads);
                }
            }
            SpecialKind::Head { kind: head } => {
                let path = match head.as_str() {
                    "skeleton" => Some("entity/skeleton/skeleton"),
                    "wither_skeleton" => Some("entity/skeleton/wither_skeleton"),
                    "zombie" => Some("entity/zombie/zombie"),
                    "creeper" => Some("entity/creeper/creeper"),
                    "piglin" => Some("entity/piglin/piglin"),
                    _ => None,
                };
                if let Some(path) = path {
                    self.head(resources, path, &mut quads)?;
                }
            }
            SpecialKind::PlayerHead => {
                self.head(resources, "entity/player/wide/steve", &mut quads)?;
                let (s, size) = self.entity_texture(resources, "entity/player/wide/steve")?;
                bake_box(
                    &EntityBox {
                        from: [4.0, 0.0, 4.0],
                        to: [12.0, 8.0, 12.0],
                        tex: (32.0, 0.0),
                        grow: 0.5,
                    },
                    s,
                    size,
                    &mut quads,
                );
            }
            SpecialKind::Banner { color } => {
                let (s, size) = self.entity_texture(resources, "entity/banner/banner_base")?;
                quads.push(flat_quad(
                    [4.0, 0.0, 12.0, 16.0],
                    [1.0 / size.0, 1.0 / size.1, 21.0 / size.0, 41.0 / size.1],
                    s,
                ));
                let rgb = DYE_RGB
                    .iter()
                    .find(|(n, _)| *n == strip_ns(color))
                    .map_or(0xFFFFFF, |(_, c)| *c);
                tints.push(0xFF00_0000 | rgb);
                keep_display = false;
                gui_light = GuiLight::Front;
            }
            _ => {}
        }
        if quads.is_empty() {
            if let Some(particle) = base.particle {
                quads.push(flat_quad(
                    [0.0, 0.0, 16.0, 16.0],
                    [0.0, 0.0, 1.0, 1.0],
                    particle,
                ));
            }
            keep_display = false;
            gui_light = GuiLight::Front;
        }
        Ok(Arc::new(base.derive(quads, tints, gui_light, keep_display)))
    }

    fn head(
        &mut self,
        resources: &ResourceManager,
        path: &str,
        out: &mut Vec<BakedQuad>,
    ) -> Result<()> {
        let (s, size) = self.entity_texture(resources, path)?;
        bake_box(
            &EntityBox {
                from: [4.0, 0.0, 4.0],
                to: [12.0, 8.0, 12.0],
                tex: (0.0, 0.0),
                grow: 0.0,
            },
            s,
            size,
            out,
        );
        Ok(())
    }
}
