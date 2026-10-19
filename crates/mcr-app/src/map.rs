//! Turns client map data into the images and markers the entity scenes draw.

use std::borrow::Cow;

use mcr_block_entity::{Align, layout_display};
use mcr_client::MapData;
use mcr_entity_model::mcr_protocol::play::clientbound::ui::MapDecoration as ProtoDecoration;
use mcr_entity_model::{ImageRgba, LABEL_SCALE, MapDecoration, MapLabels};
use mcr_ui::Ui;

const SIZE: usize = mcr_ui::MAP_SIZE;

#[cfg(test)]
fn rgba(index: u8) -> [u8; 4] {
    mcr_ui::map_color(index)
}

pub fn image(colors: &[u8]) -> ImageRgba {
    ImageRgba {
        width: SIZE as u32,
        height: SIZE as u32,
        rgba: mcr_ui::map_rgba(colors),
    }
}

/// `MapDecorationTypes`: the asset under `textures/map/decorations` and `showOnItemFrame` of a type.
fn decoration_type(name: &str) -> (Cow<'_, str>, bool) {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    if let Some(color) = name.strip_prefix("banner_") {
        return (Cow::Owned(format!("{color}_banner")), true);
    }
    let asset = match name {
        "mansion" => "woodland_mansion",
        "monument" => "ocean_monument",
        "village_desert" => "desert_village",
        "village_plains" => "plains_village",
        "village_savanna" => "savanna_village",
        "village_snowy" => "snowy_village",
        "village_taiga" => "taiga_village",
        "ocean_ruin_warm" => "warm_ocean_ruins",
        other => other,
    };
    let on_frame = !matches!(
        name,
        "player" | "red_marker" | "blue_marker" | "player_off_map" | "player_off_limits"
    );
    (Cow::Borrowed(asset), on_frame)
}

pub fn decorations(map: &MapData) -> Vec<MapDecoration> {
    let types = mcr_data::v26_3().registry("minecraft:map_decoration_type");
    map.decorations
        .iter()
        .filter_map(|d| {
            let id = u32::try_from(d.decoration_type).ok()?;
            let (sprite, on_frame) = decoration_type(types.as_ref()?.name_of(id)?);
            Some(MapDecoration {
                sprite: sprite.into_owned(),
                x: d.x,
                y: d.y,
                rot: d.rot & 15,
                on_frame,
            })
        })
        .collect()
}

const MAX_LABELS: usize = 32;
const MAX_LABEL_CELLS: usize = 64;
const LABEL_BACKGROUND: [u8; 4] = [0, 0, 0, 128];

/// The names of `map`'s decorations as `MapRenderer` writes them under each icon.
pub fn labels(ui: &mut Ui, map: &MapData) -> MapLabels {
    let types = mcr_data::v26_3().registry("minecraft:map_decoration_type");
    let named: Vec<(&ProtoDecoration, bool)> = map
        .decorations
        .iter()
        .filter(|d| {
            d.name
                .as_ref()
                .is_some_and(|n| !n.to_plain_string().is_empty())
        })
        .take(MAX_LABELS)
        .map(|d| {
            let on_frame = u32::try_from(d.decoration_type)
                .ok()
                .and_then(|id| types.as_ref()?.name_of(id))
                .is_none_or(|n| decoration_type(n).1);
            (d, on_frame)
        })
        .collect();
    if named.is_empty() {
        return MapLabels::default();
    }
    let held = bake_labels(ui, named.iter().map(|(d, _)| *d));
    if named.iter().all(|(_, on_frame)| *on_frame) {
        return MapLabels {
            held: Some(held),
            frame: None,
            frame_same: true,
        };
    }
    let framed: Vec<&ProtoDecoration> = named.iter().filter(|(_, f)| *f).map(|(d, _)| *d).collect();
    MapLabels {
        held: Some(held),
        frame: (!framed.is_empty()).then(|| bake_labels(ui, framed.into_iter())),
        frame_same: false,
    }
}

fn bake_labels<'a>(
    ui: &mut Ui,
    decorations: impl Iterator<Item = &'a ProtoDecoration>,
) -> ImageRgba {
    let size = SIZE as u32 * LABEL_SCALE;
    let mut img = ImageRgba::solid(size, size, [0; 4]);
    for d in decorations {
        let Some(name) = &d.name else { continue };
        let mut cells = ui.flatten(name);
        cells.truncate(MAX_LABEL_CELLS);
        let width = ui.width(&cells);
        if width <= 0.0 {
            continue;
        }
        let layout = layout_display(ui, &cells, i32::MAX, Align::Left, false);
        let scale = (25.0 / width).clamp(0.0, 6.0 / 9.0);
        let left = f32::from(d.x) / 2.0 + 64.0 - width * scale / 2.0;
        let top = f32::from(d.y) / 2.0 + 64.0 + 4.0;
        let to_texel = |x: f32, y: f32| {
            let k = LABEL_SCALE as f32;
            ((left + x * scale) * k, (top + y * scale) * k)
        };
        let (x0, y0) = to_texel(-1.0, -1.0);
        let (x1, y1) = to_texel(width, layout.height.max(8) as f32 + 1.0);
        fill(&mut img, [x0, y0, x1, y1], |_, _| LABEL_BACKGROUND);
        let atlas = ui.fonts().atlas();
        let page_size = atlas.size() as f32;
        for q in &layout.quads {
            let Some(page) = atlas.pages().get(q.page as usize) else {
                continue;
            };
            let (a, b) = (q.pos[0], q.pos[2]);
            let (ua, ub) = (q.uv[0], q.uv[2]);
            let (tx0, ty0) = to_texel(a[0].min(b[0]), a[1].min(b[1]));
            let (tx1, ty1) = to_texel(a[0].max(b[0]), a[1].max(b[1]));
            let (w, h) = ((tx1 - tx0).max(1e-3), (ty1 - ty0).max(1e-3));
            let flip_x = a[0] > b[0];
            let flip_y = a[1] > b[1];
            fill(&mut img, [tx0, ty0, tx1, ty1], |x, y| {
                let fx = ((x - tx0) / w).clamp(0.0, 1.0);
                let fy = ((y - ty0) / h).clamp(0.0, 1.0);
                let fx = if flip_x { 1.0 - fx } else { fx };
                let fy = if flip_y { 1.0 - fy } else { fy };
                let u = (ua[0] + (ub[0] - ua[0]) * fx) * page_size;
                let v = (ua[1] + (ub[1] - ua[1]) * fy) * page_size;
                let i = ((v as usize).min(page_size as usize - 1) * page_size as usize
                    + (u as usize).min(page_size as usize - 1))
                    * 4;
                let texel = page.pixels.get(i..i + 4).unwrap_or(&[0; 4]);
                let mul = |c: usize| (u16::from(texel[c]) * u16::from(q.color[c]) / 255) as u8;
                [mul(0), mul(1), mul(2), mul(3)]
            });
        }
    }
    img
}

/// Blends `color(x, y)` (sampled at texel centres) over every texel of `rect` inside `img`.
fn fill(img: &mut ImageRgba, rect: [f32; 4], color: impl Fn(f32, f32) -> [u8; 4]) {
    let (w, h) = (img.width as i32, img.height as i32);
    let x0 = (rect[0].round() as i32).clamp(0, w);
    let x1 = (rect[2].round() as i32).clamp(0, w);
    let y0 = (rect[1].round() as i32).clamp(0, h);
    let y1 = (rect[3].round() as i32).clamp(0, h);
    for y in y0..y1 {
        for x in x0..x1 {
            let src = color(x as f32 + 0.5, y as f32 + 0.5);
            let a = u32::from(src[3]);
            if a == 0 {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            let dst = &mut img.rgba[i..i + 4];
            let da = u32::from(dst[3]);
            let out_a = a + da * (255 - a) / 255;
            for c in 0..3 {
                let blended = (u32::from(src[c]) * a + u32::from(dst[c]) * da * (255 - a) / 255)
                    / out_a.max(1);
                dst[c] = blended.min(255) as u8;
            }
            dst[3] = out_a.min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_four_shades_and_transparent_base() {
        assert_eq!(rgba(0), [0; 4]);
        assert_eq!(rgba(3), [0; 4]);
        assert_eq!(rgba(4 + 2), [0x7f, 0xb2, 0x38, 255]);
        assert_eq!(rgba(4 + 3)[0], (0x7f * 135 / 255) as u8);
        assert_eq!(rgba(255), [0; 4]);
    }

    #[test]
    fn every_decoration_type_has_a_sprite() {
        let types = mcr_data::v26_3()
            .registry("minecraft:map_decoration_type")
            .unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/versions/26.3/src/assets/minecraft/textures/map/decorations");
        let mut seen = 0;
        for id in 0.. {
            let Some(name) = types.name_of(id) else { break };
            let (sprite, _) = decoration_type(name);
            if dir.is_dir() {
                assert!(
                    dir.join(format!("{sprite}.png")).is_file(),
                    "{name} -> {sprite}"
                );
            }
            seen += 1;
        }
        assert!(seen >= 35, "{seen}");
        assert_eq!(
            decoration_type("minecraft:banner_light_blue").0,
            "light_blue_banner"
        );
        assert_eq!(
            decoration_type("minecraft:desert_pyramid").0,
            "desert_pyramid"
        );
        assert_eq!(
            decoration_type("minecraft:ocean_ruin_warm").0,
            "warm_ocean_ruins"
        );
        assert_eq!(decoration_type("minecraft:mansion").0, "woodland_mansion");
    }

    #[test]
    fn players_and_markers_stay_off_item_frames() {
        for name in [
            "player",
            "player_off_map",
            "player_off_limits",
            "red_marker",
            "blue_marker",
        ] {
            assert!(!decoration_type(name).1, "{name}");
        }
        for name in [
            "frame",
            "banner_red",
            "target_x",
            "red_x",
            "mansion",
            "trial_chambers",
        ] {
            assert!(decoration_type(name).1, "{name}");
        }
    }

    #[test]
    fn label_fill_blends_and_clips() {
        let mut img = ImageRgba::solid(4, 4, [0; 4]);
        fill(&mut img, [-5.0, -5.0, 2.0, 2.0], |_, _| [0, 0, 0, 128]);
        fill(&mut img, [1.0, 1.0, 9.0, 9.0], |_, _| [255, 255, 255, 255]);
        assert_eq!(img.rgba[3], 128);
        assert_eq!(
            &img.rgba[(4 + 1) * 4..(4 + 1) * 4 + 4],
            &[255, 255, 255, 255]
        );
        assert_eq!(img.rgba[(3 * 4 + 3) * 4 + 3], 255);
    }

    #[test]
    fn short_or_hostile_colour_arrays_are_padded() {
        let img = image(&[6]);
        assert_eq!(img.rgba.len(), SIZE * SIZE * 4);
        assert_eq!(img.rgba[3], 255);
        assert_eq!(img.rgba[7], 0);
    }
}
