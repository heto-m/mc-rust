use crate::NetError;
use libdeflater::{CompressionLvl, Compressor, Decompressor};
use std::fmt;

const LEVEL: i32 = 6;

pub struct Deflater(Compressor);

impl Deflater {
    pub fn new() -> Self {
        let lvl = CompressionLvl::new(LEVEL).unwrap_or_default();
        Self(Compressor::new(lvl))
    }

    pub fn deflate(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), NetError> {
        out.resize(self.0.zlib_compress_bound(input.len()), 0);
        let n = self
            .0
            .zlib_compress(input, out)
            .map_err(|_| NetError::Deflate)?;
        out.truncate(n);
        Ok(())
    }
}

impl Default for Deflater {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Deflater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Deflater")
    }
}

pub struct Inflater(Decompressor);

impl Inflater {
    pub fn new() -> Self {
        Self(Decompressor::new())
    }

    /// Output is capped at `size`, which bounds memory for hostile streams.
    pub fn inflate(
        &mut self,
        input: &[u8],
        size: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), NetError> {
        out.clear();
        out.resize(size, 0);
        match self.0.zlib_decompress(input, out) {
            Ok(n) if n == size => Ok(()),
            Ok(actual) => Err(NetError::SizeMismatch {
                declared: size,
                actual,
            }),
            Err(_) => Err(NetError::Inflate),
        }
    }
}

impl Default for Inflater {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Inflater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Inflater")
    }
}
