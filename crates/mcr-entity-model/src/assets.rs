use std::io::Read;
use std::path::Path;

use crate::Error;

#[derive(Debug, Clone)]
pub struct ImageRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Reads `assets/minecraft/<texture_path>` (e.g. `textures/entity/pig/pig_temperate.png`) from a jar.
pub fn load_jar_texture(jar: impl AsRef<Path>, texture_path: &str) -> Result<ImageRgba, Error> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut bytes = Vec::new();
    archive
        .by_name(&format!("assets/minecraft/{texture_path}"))?
        .read_to_end(&mut bytes)?;
    decode_png(&bytes)
}

fn decode_png(bytes: &[u8]) -> Result<ImageRgba, Error> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf)?;
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(Error::PngFormat(other)),
    };
    Ok(ImageRgba {
        width: info.width,
        height: info.height,
        rgba,
    })
}
