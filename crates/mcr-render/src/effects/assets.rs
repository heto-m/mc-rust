#[derive(Debug, Clone)]
pub struct EffectImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl EffectImage {
    pub fn decode(png_bytes: &[u8]) -> Option<Self> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let data = &buf[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => data.to_vec(),
            png::ColorType::Rgb => data
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Indexed => return None,
        };
        let (width, height) = (info.width, info.height);
        (rgba.len() == (width * height * 4) as usize).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    fn solid(rgba: [u8; 4]) -> Self {
        Self {
            width: 1,
            height: 1,
            rgba: rgba.to_vec(),
        }
    }
}

const MOON_NAMES: [&str; 8] = [
    "full_moon",
    "waning_gibbous",
    "third_quarter",
    "waning_crescent",
    "new_moon",
    "waxing_crescent",
    "first_quarter",
    "waxing_gibbous",
];

/// Decoded environment textures. Moon images are indexed by moon phase, destroy images by stage.
#[derive(Debug, Clone)]
pub struct EffectAssets {
    pub sun: EffectImage,
    pub moons: [EffectImage; 8],
    pub end_sky: EffectImage,
    pub clouds: EffectImage,
    pub rain: EffectImage,
    pub snow: EffectImage,
    pub destroy: [EffectImage; 10],
}

impl EffectAssets {
    /// Loads from resource paths like `assets/minecraft/textures/environment/clouds.png`; `read` returns
    /// the file bytes. Missing pieces fall back to blank images so a partial pack still renders.
    pub fn load(read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Self {
        let get = |path: &str, fallback: [u8; 4]| {
            read(&format!("assets/minecraft/textures/{path}.png"))
                .and_then(|b| EffectImage::decode(&b))
                .unwrap_or_else(|| EffectImage::solid(fallback))
        };
        let clear = [0; 4];
        Self {
            sun: get("environment/celestial/sun", clear),
            moons: std::array::from_fn(|i| {
                get(
                    &format!("environment/celestial/moon/{}", MOON_NAMES[i]),
                    clear,
                )
            }),
            end_sky: get("environment/end_sky", [20, 10, 30, 255]),
            clouds: get("environment/clouds", clear),
            rain: get("environment/rain", clear),
            snow: get("environment/snow", clear),
            destroy: std::array::from_fn(|i| get(&format!("block/destroy_stage_{i}"), clear)),
        }
    }
}
