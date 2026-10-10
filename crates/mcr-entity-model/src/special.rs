//! Held, framed and dropped items whose model is an entity model (shield, trident).

use glam::{Mat4, Vec3};
use mcr_assets::SpecialKind;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, DyeColor, component_type_id};

use crate::cube::Vertex;
use crate::gpu::Pipeline;
use crate::model::Model;
use crate::pose::evaluate;
use crate::scene::MODEL_SCALE;

const MAX_PATTERN_LAYERS: usize = 16;

const DYE_RGB: [u32; 16] = [
    0xF9FFFE, 0xF9801D, 0xC74EBD, 0x3AB3DA, 0xFED83D, 0x80C71F, 0xF38BAA, 0x474F52, 0x9D9D97,
    0x169C9C, 0x8932B8, 0x3C44AA, 0x835432, 0x5E7C16, 0xB02E26, 0x1D1D21,
];

pub(crate) struct SpecialPart {
    pub mesh: Model,
    pub texture: String,
    pub tint: [u8; 4],
    pub pipeline: Pipeline,
}

fn rgb(color: DyeColor) -> [u8; 4] {
    let c = DYE_RGB[(color as usize).min(15)];
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]
}

/// The layer at rest as one static mesh in block units, y still down.
fn flatten(model: &Model) -> Model {
    let rest = model.rest_pose();
    let mut world = vec![Mat4::IDENTITY; model.bones.len()];
    evaluate(model, &rest, &mut world);
    let scale = Mat4::from_scale(Vec3::splat(MODEL_SCALE));
    let vertices = model
        .vertices
        .iter()
        .map(|v| {
            let m = scale
                * world
                    .get(v.bone as usize)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
            let n = m
                .transform_vector3(Vec3::from(v.normal))
                .normalize_or_zero();
            Vertex {
                pos: m.transform_point3(Vec3::from(v.pos)).to_array(),
                bone: 0,
                normal: n.to_array(),
                uv: v.uv,
            }
        })
        .collect();
    let mut flat = Model::from_geometry(vertices, model.indices.clone());
    flat.tex_size = model.tex_size;
    flat
}

fn pattern_names(stack: &ItemStack) -> Vec<(String, DyeColor)> {
    let Some(DataComponent::BannerPatterns(layers)) =
        component_type_id("banner_patterns").and_then(|id| stack.components.get(id))
    else {
        return Vec::new();
    };
    layers
        .iter()
        .take(MAX_PATTERN_LAYERS)
        .filter_map(|l| {
            let name = match &l.pattern {
                mcr_buf::IdOr::Inline(p) => p.asset_id.as_str().to_owned(),
                mcr_buf::IdOr::Id(i) => mcr_data::v26_3()
                    .registry("minecraft:banner_pattern")?
                    .name_of(u32::try_from(*i).ok()?)?
                    .to_owned(),
            };
            let short = name.strip_prefix("minecraft:").unwrap_or(&name);
            short
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                .then(|| (short.to_owned(), l.color))
        })
        .collect()
}

fn base_color(stack: &ItemStack) -> Option<DyeColor> {
    match component_type_id("base_color").and_then(|id| stack.components.get(id))? {
        DataComponent::BaseColor(c) => Some(*c),
        _ => None,
    }
}

/// Distinguishes shields (and tridents) of one item id that look different.
pub(crate) fn variant_hash(stack: &ItemStack) -> u64 {
    let mut h = 17u64;
    if let Some(c) = base_color(stack) {
        h = h.wrapping_mul(31).wrapping_add(1 + c as u64);
    }
    for (name, color) in pattern_names(stack) {
        h = name
            .bytes()
            .fold(h.wrapping_mul(31), |h, b| {
                h.wrapping_mul(31).wrapping_add(u64::from(b))
            })
            .wrapping_add(color as u64);
    }
    h
}

pub(crate) fn parts(
    kind: &SpecialKind,
    layer: impl Fn(&str) -> Option<Model>,
    stack: Option<&ItemStack>,
) -> Vec<SpecialPart> {
    match kind {
        SpecialKind::Shield => {
            let Some(model) = layer("minecraft:shield#main") else {
                return Vec::new();
            };
            let mesh = flatten(&model);
            let color = stack.and_then(base_color);
            let patterns = stack.map(pattern_names).unwrap_or_default();
            let dyed = color.is_some() || !patterns.is_empty();
            let mut out = vec![SpecialPart {
                mesh: mesh.clone(),
                texture: if dyed {
                    "entity/shield/shield_base"
                } else {
                    "entity/shield/shield_base_nopattern"
                }
                .to_owned(),
                tint: color.map_or([255; 4], rgb),
                pipeline: Pipeline::Cutout,
            }];
            for (name, color) in patterns {
                out.push(SpecialPart {
                    mesh: mesh.clone(),
                    texture: format!("entity/shield/{name}"),
                    tint: rgb(color),
                    pipeline: Pipeline::Decal,
                });
            }
            out
        }
        SpecialKind::Trident => layer("minecraft:trident#main")
            .map(|m| SpecialPart {
                mesh: flatten(&m),
                texture: "entity/trident/trident".to_owned(),
                tint: [255; 4],
                pipeline: Pipeline::Cutout,
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_protocol::types::component::BannerPatternLayer;

    #[test]
    fn hostile_patterns_are_capped_and_hash_does_not_overflow() {
        let mut stack = ItemStack::new(1, 1);
        let layers = (0..10_000)
            .map(|i| BannerPatternLayer {
                pattern: mcr_buf::IdOr::Id(i),
                color: DyeColor::Black,
            })
            .collect();
        stack
            .components
            .added
            .push(DataComponent::BannerPatterns(layers));
        assert!(pattern_names(&stack).len() <= MAX_PATTERN_LAYERS);
        let _ = variant_hash(&stack);
    }

    #[test]
    fn hostile_registry_ids_are_ignored() {
        let mut stack = ItemStack::new(1, 1);
        let layers = vec![
            BannerPatternLayer {
                pattern: mcr_buf::IdOr::Id(i32::MIN),
                color: DyeColor::Red,
            },
            BannerPatternLayer {
                pattern: mcr_buf::IdOr::Id(i32::MAX),
                color: DyeColor::Red,
            },
        ];
        stack
            .components
            .added
            .push(DataComponent::BannerPatterns(layers));
        assert!(pattern_names(&stack).is_empty());
        assert_eq!(rgb(DyeColor::Black), [0x1D, 0x1D, 0x21, 255]);
    }
}
