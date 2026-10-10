//! Turns client map data into the images and markers the entity scenes draw.

use mcr_client::MapData;
use mcr_entity_model::{ImageRgba, MapDecoration};

const SIZE: usize = 128;
const BASE: [u32; 62] = [
    0x000000, 0x7fb238, 0xf7e9a3, 0xc7c7c7, 0xff0000, 0xa0a0ff, 0xa7a7a7, 0x007c00, 0xffffff,
    0xa4a8b8, 0x976d4d, 0x707070, 0x4040ff, 0x8f7748, 0xfffcf5, 0xd87f33, 0xb24cd8, 0x6699d8,
    0xe5e533, 0x7fcc19, 0xf27fa5, 0x4c4c4c, 0x999999, 0x4c7f99, 0x7f3fb2, 0x334cb2, 0x664c33,
    0x667f33, 0x993333, 0x191919, 0xfaee4d, 0x5cdbd5, 0x4a80ff, 0x00d93a, 0x815631, 0x700200,
    0xd1b1a1, 0x9f5224, 0x95576c, 0x706c8a, 0xba8524, 0x677535, 0xa04d4e, 0x392923, 0x876b62,
    0x575c5c, 0x7a4958, 0x4c3e5c, 0x4c3223, 0x4c522a, 0x8e3c2e, 0x251610, 0xbd3031, 0x943e61,
    0x5c191d, 0x16b698, 0x127f62, 0x3a8e8c, 0x5a1c01, 0x181818, 0x6c4f31, 0x7a7a5c,
];
const SHADE: [u32; 4] = [180, 220, 255, 135];

fn rgba(index: u8) -> [u8; 4] {
    let base = usize::from(index >> 2);
    if base == 0 || base >= BASE.len() {
        return [0; 4];
    }
    let c = BASE[base];
    let k = SHADE[usize::from(index & 3)];
    let ch = |shift: u32| (((c >> shift) & 0xFF) * k / 255) as u8;
    [ch(16), ch(8), ch(0), 255]
}

pub fn image(colors: &[u8]) -> ImageRgba {
    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    for i in 0..SIZE * SIZE {
        rgba.extend_from_slice(&rgba_at(colors, i));
    }
    ImageRgba {
        width: SIZE as u32,
        height: SIZE as u32,
        rgba,
    }
}

fn rgba_at(colors: &[u8], i: usize) -> [u8; 4] {
    colors.get(i).map_or([0; 4], |c| rgba(*c))
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
