//! `MapColor` palette with its four brightness shades.

pub const MAP_SIZE: usize = 128;

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

/// Straight-alpha RGBA of one packed map colour byte; transparent for "none" and unknown colours.
pub fn map_color(index: u8) -> [u8; 4] {
    let base = usize::from(index >> 2);
    if base == 0 || base >= BASE.len() {
        return [0; 4];
    }
    let c = BASE[base];
    let k = SHADE[usize::from(index & 3)];
    let ch = |shift: u32| (((c >> shift) & 0xFF) * k / 255) as u8;
    [ch(16), ch(8), ch(0), 255]
}

/// The 128x128 RGBA image of a map's colour bytes; missing bytes are transparent.
pub fn map_rgba(colors: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(MAP_SIZE * MAP_SIZE * 4);
    for i in 0..MAP_SIZE * MAP_SIZE {
        rgba.extend_from_slice(&colors.get(i).map_or([0; 4], |c| map_color(*c)));
    }
    rgba
}
