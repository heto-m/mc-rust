#![allow(dead_code)]
// Builds tiny fake mod jars at test time; shared with the integration tests through `#[path]`.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

#[derive(Debug, Default, Clone)]
pub struct Jar {
    entries: Vec<(String, Vec<u8>)>,
}

pub fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let pixels: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    out
}

impl Jar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fabric(id: &str, version: &str) -> Self {
        Self::new().manifest(&format!(
            r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","name":"{id} mod","environment":"*"}}"#
        ))
    }

    pub fn manifest(self, json: &str) -> Self {
        self.file("fabric.mod.json", json.as_bytes())
    }

    /// A Fabric manifest with extra top-level fields spliced in, e.g. `"depends":{"foo":"*"}`.
    pub fn fabric_with(id: &str, version: &str, extra: &str) -> Self {
        Self::new().manifest(&format!(
            r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","name":"{id} mod",{extra}}}"#
        ))
    }

    pub fn file(mut self, name: &str, bytes: &[u8]) -> Self {
        self.entries.push((name.to_owned(), bytes.to_vec()));
        self
    }

    pub fn nested(self, path: &str, jar: Jar) -> Self {
        let bytes = jar.build();
        self.file(path, &bytes)
    }

    pub fn build(&self) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in &self.entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    pub fn write(&self, dir: &Path, file_name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(file_name);
        std::fs::write(&path, self.build()).unwrap();
        path
    }
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcr-mods-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
