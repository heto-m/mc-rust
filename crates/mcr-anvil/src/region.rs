use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::{GzDecoder, ZlibDecoder};
use mcr_nbt::{Compound, Limits};

use crate::{AnvilError, Result};

pub const SECTOR_BYTES: usize = 4096;
pub const HEADER_BYTES: usize = 2 * SECTOR_BYTES;
const CHUNK_HEADER: usize = 5;
const EXTERNAL_FLAG: u8 = 0x80;
pub const MAX_CHUNK_BYTES: usize = 32 * 1024 * 1024;
const LZ4_MAGIC: &[u8; 8] = b"LZ4Block";
const LZ4_HEADER: usize = 21;
const LZ4_METHOD_RAW: u8 = 0x10;
const LZ4_METHOD_LZ4: u8 = 0x20;

pub const COMPRESSION_GZIP: u8 = 1;
pub const COMPRESSION_ZLIB: u8 = 2;
pub const COMPRESSION_NONE: u8 = 3;
pub const COMPRESSION_LZ4: u8 = 4;
#[cfg(test)]
const COMPRESSION_CUSTOM: u8 = 127;

/// A region file held in memory; chunk coordinates are local (0..32).
#[derive(Debug)]
pub struct RegionFile {
    bytes: Vec<u8>,
    dir: PathBuf,
    rx: i32,
    rz: i32,
}

impl RegionFile {
    pub fn open(path: &Path, rx: i32, rz: i32) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Self::from_bytes(bytes, dir, rx, rz)
    }

    /// `dir` is where external `.mcc` files are looked up.
    pub fn from_bytes(bytes: Vec<u8>, dir: PathBuf, rx: i32, rz: i32) -> Result<Self> {
        if !bytes.is_empty() && bytes.len() < HEADER_BYTES {
            return Err(AnvilError::TruncatedHeader(bytes.len()));
        }
        Ok(Self { bytes, dir, rx, rz })
    }

    pub fn region_x(&self) -> i32 {
        self.rx
    }

    pub fn region_z(&self) -> i32 {
        self.rz
    }

    fn slot(lx: usize, lz: usize) -> usize {
        (lx & 31) + (lz & 31) * 32
    }

    fn entry(&self, slot: usize) -> u32 {
        match self.bytes.get(slot * 4..slot * 4 + 4) {
            Some(b) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }

    pub fn has_chunk(&self, lx: usize, lz: usize) -> bool {
        self.entry(Self::slot(lx, lz)) != 0
    }

    /// Seconds since the epoch of the chunk's last save, 0 when absent.
    pub fn timestamp(&self, lx: usize, lz: usize) -> u32 {
        let at = SECTOR_BYTES + Self::slot(lx, lz) * 4;
        match self.bytes.get(at..at + 4) {
            Some(b) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }

    pub fn present_chunks(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..1024)
            .filter(|&s| self.entry(s) != 0)
            .map(|s| (s % 32, s / 32))
    }

    /// `Ok(None)` when the chunk was never saved.
    pub fn read_chunk(&self, lx: usize, lz: usize) -> Result<Option<Compound>> {
        let entry = self.entry(Self::slot(lx, lz));
        if entry == 0 {
            return Ok(None);
        }
        let start = (entry >> 8) as usize;
        let sectors = (entry & 0xff) as usize;
        let bad = |reason: &'static str| AnvilError::BadSector { lx, lz, reason };
        if start < 2 {
            return Err(bad("offset overlaps header"));
        }
        if sectors == 0 {
            return Err(bad("zero sectors"));
        }
        let begin = start * SECTOR_BYTES;
        let region_end = begin + sectors * SECTOR_BYTES;
        let Some(head) = self.bytes.get(begin..begin + CHUNK_HEADER) else {
            return Err(bad("offset past end of file"));
        };
        let length = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let compression = head[4];
        if length == 0 {
            return Err(bad("empty stream"));
        }
        let stream_len = length - 1;
        if compression & EXTERNAL_FLAG != 0 {
            let id = compression & !EXTERNAL_FLAG;
            let path = self.dir.join(format!(
                "c.{}.{}.mcc",
                self.rx * 32 + lx as i32,
                self.rz * 32 + lz as i32
            ));
            let data = std::fs::read(&path)?;
            return decode(&data, id).map(Some);
        }
        if begin + 4 + length > region_end {
            return Err(bad("length exceeds allocated sectors"));
        }
        let Some(stream) = self
            .bytes
            .get(begin + CHUNK_HEADER..begin + CHUNK_HEADER + stream_len)
        else {
            return Err(bad("stream truncated"));
        };
        decode(stream, compression).map(Some)
    }
}

fn decode(stream: &[u8], compression: u8) -> Result<Compound> {
    let raw = match compression {
        COMPRESSION_GZIP => inflate(GzDecoder::new(stream))?,
        COMPRESSION_ZLIB => inflate(ZlibDecoder::new(stream))?,
        COMPRESSION_NONE => stream.to_vec(),
        COMPRESSION_LZ4 => lz4_block_stream(stream)?,
        id => return Err(AnvilError::UnsupportedCompression(id)),
    };
    let limits = Limits {
        max_depth: 512,
        max_bytes: MAX_CHUNK_BYTES,
    };
    Ok(mcr_nbt::read_disk(&raw, limits)?.1)
}

fn inflate<R: Read>(reader: R) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .take(MAX_CHUNK_BYTES as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() > MAX_CHUNK_BYTES {
        return Err(AnvilError::TooLarge);
    }
    Ok(out)
}

/// Vanilla's LZ4 is the lz4-java block stream, not the frame format.
fn lz4_block_stream(mut input: &[u8]) -> Result<Vec<u8>> {
    let bad = |reason: &'static str| AnvilError::BadLz4(reason);
    let mut out = Vec::new();
    loop {
        if input.len() < LZ4_HEADER {
            return Err(bad("truncated block header"));
        }
        let (head, rest) = input.split_at(LZ4_HEADER);
        if &head[..8] != LZ4_MAGIC {
            return Err(bad("bad magic"));
        }
        let method = head[8] & 0xf0;
        let compressed = i32::from_le_bytes([head[9], head[10], head[11], head[12]]);
        let original = i32::from_le_bytes([head[13], head[14], head[15], head[16]]);
        let (Ok(compressed), Ok(original)) =
            (usize::try_from(compressed), usize::try_from(original))
        else {
            return Err(bad("negative length"));
        };
        if original > MAX_CHUNK_BYTES || out.len() + original > MAX_CHUNK_BYTES {
            return Err(AnvilError::TooLarge);
        }
        if original == 0 {
            return Ok(out);
        }
        if rest.len() < compressed {
            return Err(bad("truncated block"));
        }
        let (block, rest) = rest.split_at(compressed);
        match method {
            LZ4_METHOD_RAW => {
                if compressed != original {
                    return Err(bad("raw block length mismatch"));
                }
                out.extend_from_slice(block);
            }
            LZ4_METHOD_LZ4 => {
                let at = out.len();
                out.resize(at + original, 0);
                let n = lz4_flex::block::decompress_into(block, &mut out[at..])
                    .map_err(|_| bad("corrupt block"))?;
                if n != original {
                    return Err(bad("block length mismatch"));
                }
            }
            _ => return Err(bad("unknown block method")),
        }
        input = rest;
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::{GzEncoder, ZlibEncoder};

    use super::*;

    pub fn lz4_stream(raw: &[u8], compress: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in raw.chunks(1000) {
            let (method, body) = if compress {
                (LZ4_METHOD_LZ4, lz4_flex::block::compress(chunk))
            } else {
                (LZ4_METHOD_RAW, chunk.to_vec())
            };
            out.extend_from_slice(LZ4_MAGIC);
            out.push(method | 9);
            out.extend_from_slice(&(body.len() as i32).to_le_bytes());
            out.extend_from_slice(&(chunk.len() as i32).to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            out.extend_from_slice(&body);
        }
        out.extend_from_slice(LZ4_MAGIC);
        out.push(LZ4_METHOD_RAW | 9);
        out.extend_from_slice(&[0; 12]);
        out
    }

    pub fn compress(root: &Compound, id: u8) -> Vec<u8> {
        let raw = mcr_nbt::write_disk("", root, mcr_nbt::Compression::None).unwrap();
        match id {
            COMPRESSION_GZIP => {
                let mut e = GzEncoder::new(Vec::new(), Compression::fast());
                e.write_all(&raw).unwrap();
                e.finish().unwrap()
            }
            COMPRESSION_ZLIB => {
                let mut e = ZlibEncoder::new(Vec::new(), Compression::fast());
                e.write_all(&raw).unwrap();
                e.finish().unwrap()
            }
            COMPRESSION_NONE => raw,
            COMPRESSION_LZ4 => lz4_stream(&raw, true),
            _ => raw,
        }
    }

    #[derive(Default)]
    pub struct RegionWriter {
        header: Vec<u8>,
        body: Vec<u8>,
    }

    impl RegionWriter {
        pub fn new() -> Self {
            Self {
                header: vec![0; HEADER_BYTES],
                body: Vec::new(),
            }
        }

        pub fn put_raw(&mut self, lx: usize, lz: usize, compression: u8, stream: &[u8]) {
            let slot = lx + lz * 32;
            let sector = 2 + self.body.len() / SECTOR_BYTES;
            let mut record = Vec::new();
            record.extend_from_slice(&(stream.len() as u32 + 1).to_be_bytes());
            record.push(compression);
            record.extend_from_slice(stream);
            let sectors = record.len().div_ceil(SECTOR_BYTES);
            record.resize(sectors * SECTOR_BYTES, 0);
            self.body.extend_from_slice(&record);
            let entry = ((sector as u32) << 8) | sectors as u32;
            self.header[slot * 4..slot * 4 + 4].copy_from_slice(&entry.to_be_bytes());
            self.header[SECTOR_BYTES + slot * 4..SECTOR_BYTES + slot * 4 + 4]
                .copy_from_slice(&1_700_000_000u32.to_be_bytes());
        }

        pub fn put(&mut self, lx: usize, lz: usize, root: &Compound, compression: u8) {
            self.put_raw(lx, lz, compression, &compress(root, compression));
        }

        pub fn set_entry(&mut self, lx: usize, lz: usize, entry: u32) {
            let slot = lx + lz * 32;
            self.header[slot * 4..slot * 4 + 4].copy_from_slice(&entry.to_be_bytes());
        }

        pub fn finish(&self) -> Vec<u8> {
            let mut out = self.header.clone();
            out.extend_from_slice(&self.body);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn sample(n: i32) -> Compound {
        let mut c = Compound::new();
        c.insert("n", n);
        c.insert("text", "x".repeat(5000));
        c
    }

    fn region(bytes: Vec<u8>) -> RegionFile {
        RegionFile::from_bytes(bytes, PathBuf::from("/nonexistent"), 0, 0).unwrap()
    }

    #[test]
    fn every_compression_round_trips() {
        let mut w = RegionWriter::new();
        let ids = [
            COMPRESSION_GZIP,
            COMPRESSION_ZLIB,
            COMPRESSION_NONE,
            COMPRESSION_LZ4,
        ];
        for (i, id) in ids.iter().enumerate() {
            w.put(i, 0, &sample(i as i32), *id);
        }
        let r = region(w.finish());
        for i in 0..ids.len() {
            assert_eq!(r.read_chunk(i, 0).unwrap().unwrap(), sample(i as i32));
        }
        assert_eq!(r.present_chunks().count(), ids.len());
        assert_eq!(r.timestamp(0, 0), 1_700_000_000);
    }

    #[test]
    fn lz4_raw_blocks_decode() {
        let raw = mcr_nbt::write_disk("", &sample(3), mcr_nbt::Compression::None).unwrap();
        let mut w = RegionWriter::new();
        w.put_raw(0, 0, COMPRESSION_LZ4, &lz4_stream(&raw, false));
        assert_eq!(
            region(w.finish()).read_chunk(0, 0).unwrap().unwrap(),
            sample(3)
        );
    }

    #[test]
    fn absent_chunk_is_none() {
        let r = region(RegionWriter::new().finish());
        assert!(r.read_chunk(4, 4).unwrap().is_none());
        assert!(region(Vec::new()).read_chunk(0, 0).unwrap().is_none());
    }

    #[test]
    fn external_chunk_reads_mcc() {
        let dir = std::env::temp_dir().join(format!("mcr-anvil-mcc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("c.65.-30.mcc"),
            compress(&sample(9), COMPRESSION_ZLIB),
        )
        .unwrap();
        let mut w = RegionWriter::new();
        w.put_raw(1, 2, COMPRESSION_ZLIB | EXTERNAL_FLAG, &[]);
        let r = RegionFile::from_bytes(w.finish(), dir.clone(), 2, -1).unwrap();
        assert_eq!(r.read_chunk(1, 2).unwrap().unwrap(), sample(9));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_mcc_is_an_error() {
        let mut w = RegionWriter::new();
        w.put_raw(0, 0, COMPRESSION_ZLIB | EXTERNAL_FLAG, &[]);
        assert!(matches!(
            region(w.finish()).read_chunk(0, 0),
            Err(AnvilError::Io(_))
        ));
    }

    #[test]
    fn custom_compression_is_rejected() {
        let mut w = RegionWriter::new();
        w.put_raw(0, 0, COMPRESSION_CUSTOM, b"\0\x04abcd");
        w.put_raw(1, 0, 9, b"");
        let r = region(w.finish());
        assert!(matches!(
            r.read_chunk(0, 0),
            Err(AnvilError::UnsupportedCompression(127))
        ));
        assert!(matches!(
            r.read_chunk(1, 0),
            Err(AnvilError::UnsupportedCompression(9))
        ));
    }

    #[test]
    fn hostile_sector_tables_fail_cleanly() {
        let mut w = RegionWriter::new();
        w.put(0, 0, &sample(1), COMPRESSION_ZLIB);
        w.set_entry(1, 0, (1 << 8) | 1);
        w.set_entry(2, 0, (0xff_ffff << 8) | 1);
        w.set_entry(3, 0, 2 << 8);
        w.set_entry(4, 0, (2 << 8) | 1);
        let mut bytes = w.finish();
        let r = region(bytes.clone());
        assert!(r.read_chunk(0, 0).unwrap().is_some());
        for lx in 1..=3 {
            assert!(matches!(
                r.read_chunk(lx, 0),
                Err(AnvilError::BadSector { .. })
            ));
        }
        bytes[HEADER_BYTES..HEADER_BYTES + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            region(bytes.clone()).read_chunk(4, 0),
            Err(AnvilError::BadSector { .. })
        ));
        bytes[HEADER_BYTES..HEADER_BYTES + 4].copy_from_slice(&0u32.to_be_bytes());
        assert!(region(bytes.clone()).read_chunk(4, 0).is_err());
        bytes.truncate(HEADER_BYTES + 100);
        assert!(region(bytes).read_chunk(4, 0).is_err());
    }

    #[test]
    fn short_file_and_garbage_payloads_fail() {
        assert!(matches!(
            RegionFile::from_bytes(vec![0; 100], PathBuf::new(), 0, 0),
            Err(AnvilError::TruncatedHeader(100))
        ));
        for id in [
            COMPRESSION_GZIP,
            COMPRESSION_ZLIB,
            COMPRESSION_NONE,
            COMPRESSION_LZ4,
        ] {
            let mut w = RegionWriter::new();
            w.put_raw(0, 0, id, &[0xde, 0xad, 0xbe, 0xef, 1, 2, 3]);
            assert!(region(w.finish()).read_chunk(0, 0).is_err());
        }
    }

    #[test]
    fn lz4_oversize_block_is_rejected() {
        let mut s = Vec::new();
        s.extend_from_slice(LZ4_MAGIC);
        s.push(LZ4_METHOD_LZ4 | 9);
        s.extend_from_slice(&4i32.to_le_bytes());
        s.extend_from_slice(&i32::MAX.to_le_bytes());
        s.extend_from_slice(&[0; 8]);
        assert!(matches!(lz4_block_stream(&s), Err(AnvilError::TooLarge)));
    }
}
