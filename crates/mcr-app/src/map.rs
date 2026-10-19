//! Turns client map data into the images and markers the entity scenes draw.

use mcr_client::MapData;
use mcr_entity_model::{ImageRgba, MapDecoration};

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

fn sprite(name: &str) -> &str {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    match name {
        "mansion" => "woodland_mansion",
        "monument" => "ocean_monument",
        "village_desert" => "desert_village",
        "village_plains" => "plains_village",
        "village_savanna" => "savanna_village",
        "village_snowy" => "snowy_village",
        "village_taiga" => "taiga_village",
        "desert_pyramid" | "jungle_pyramid" => "jungle_temple",
        other => other,
    }
}

pub fn decorations(map: &MapData) -> Vec<MapDecoration> {
    let types = mcr_data::v26_3().registry("minecraft:map_decoration_type");
    map.decorations
        .iter()
        .filter_map(|d| {
            let id = u32::try_from(d.decoration_type).ok()?;
            let name = types.as_ref()?.name_of(id)?;
            Some(MapDecoration {
                sprite: sprite(name).to_owned(),
                x: d.x,
                y: d.y,
                rot: d.rot & 15,
            })
        })
        .collect()
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
    fn short_or_hostile_colour_arrays_are_padded() {
        let img = image(&[6]);
        assert_eq!(img.rgba.len(), SIZE * SIZE * 4);
        assert_eq!(img.rgba[3], 255);
        assert_eq!(img.rgba[7], 0);
    }
}
