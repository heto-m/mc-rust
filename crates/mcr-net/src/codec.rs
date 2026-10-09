use crate::NetError;
use crate::pool::{BufPool, PooledBuf};
use crate::zlib::{Deflater, Inflater};
use aes::Aes128;
use aes::cipher::KeyIvInit;
use mcr_buf::{read_varint, varint_len, write_varint};
use std::io::{self, Read};

pub const MAX_FRAME_LEN: usize = (1 << 21) - 1;
pub const MAX_UNCOMPRESSED_LEN: usize = 1 << 23;
pub const SECRET_LEN: usize = 16;

const INITIAL_BUF: usize = 64 * 1024;
const MIN_READ: usize = 4096;

type Encryptor = cfb8::Encryptor<Aes128>;
type Decryptor = crate::cipher::Cfb8Decryptor;

pub fn new_encryptor(secret: &[u8; SECRET_LEN]) -> Encryptor {
    Encryptor::new(&(*secret).into(), &(*secret).into())
}

pub fn new_decryptor(secret: &[u8; SECRET_LEN]) -> Decryptor {
    Decryptor::new(secret)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// Negative disables compression.
    Compression(i32),
    Encryption([u8; SECRET_LEN]),
}

#[derive(Debug)]
pub struct InboundFrame {
    pub id: i32,
    pub payload: PooledBuf,
}

#[derive(Debug)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    start: usize,
    end: usize,
    dec_end: usize,
    want: usize,
    cipher: Option<Decryptor>,
    threshold: Option<usize>,
    inflater: Inflater,
    pool: BufPool,
}

impl FrameDecoder {
    pub fn new(pool: BufPool) -> Self {
        Self {
            buf: vec![0; INITIAL_BUF],
            start: 0,
            end: 0,
            dec_end: 0,
            want: 0,
            cipher: None,
            threshold: None,
            inflater: Inflater::new(),
            pool,
        }
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }

    /// Applies to bytes read after this call; use for keys armed before the peer starts encrypting.
    pub fn set_encryption(&mut self, secret: &[u8; SECRET_LEN]) {
        self.cipher = Some(new_decryptor(secret));
    }

    /// Also covers bytes already buffered past the last returned frame.
    pub fn set_encryption_at_boundary(&mut self, secret: &[u8; SECRET_LEN]) {
        self.cipher = Some(new_decryptor(secret));
        self.dec_end = self.start;
    }

    pub fn apply(&mut self, switch: Switch) {
        match switch {
            Switch::Compression(t) => self.set_compression(t),
            Switch::Encryption(key) => self.set_encryption_at_boundary(&key),
        }
    }

    pub fn fill<R: Read>(&mut self, r: &mut R) -> Result<usize, NetError> {
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
            self.dec_end = 0;
        }
        let target = self.want.max(self.end - self.start + MIN_READ);
        if self.buf.len() - self.start < target {
            if self.start > 0 {
                self.buf.copy_within(self.start..self.end, 0);
                self.dec_end -= self.start;
                self.end -= self.start;
                self.start = 0;
            }
            if self.buf.len() < target {
                self.buf.resize(target.next_power_of_two(), 0);
            }
        }
        loop {
            match r.read(&mut self.buf[self.end..]) {
                Ok(0) => return Err(NetError::Eof),
                Ok(n) => {
                    self.end += n;
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// `Ok(None)` means more bytes are needed from [`fill`](Self::fill).
    pub fn next_frame(&mut self) -> Result<Option<InboundFrame>, NetError> {
        if self.dec_end < self.end {
            if let Some(c) = &mut self.cipher {
                c.decrypt(&mut self.buf[self.dec_end..self.end]);
            }
            self.dec_end = self.end;
        }
        let avail = &self.buf[self.start..self.end];
        let mut len = 0usize;
        let mut hlen = 0;
        loop {
            let Some(&b) = avail.get(hlen) else {
                self.want = hlen + 1;
                return Ok(None);
            };
            len |= usize::from(b & 0x7f) << (7 * hlen);
            hlen += 1;
            if b & 0x80 == 0 {
                break;
            }
            if hlen == 3 {
                return Err(NetError::LengthTooWide);
            }
        }
        if len == 0 {
            return Err(NetError::ZeroLength);
        }
        let total = hlen + len;
        if avail.len() < total {
            self.want = total;
            return Ok(None);
        }
        let frame = decode_body(
            &avail[hlen..total],
            self.threshold,
            &mut self.inflater,
            &self.pool,
        )?;
        self.start += total;
        self.want = 0;
        Ok(Some(frame))
    }
}

fn decode_body(
    body: &[u8],
    threshold: Option<usize>,
    inflater: &mut Inflater,
    pool: &BufPool,
) -> Result<InboundFrame, NetError> {
    let mut rest = body;
    if let Some(threshold) = threshold {
        let data_len = read_varint(&mut rest).map_err(|_| NetError::BadVarInt)?;
        if data_len < 0 {
            return Err(NetError::BadDataLength(data_len));
        }
        if data_len != 0 {
            let size = data_len as usize;
            if size < threshold {
                return Err(NetError::BelowThreshold { size, threshold });
            }
            if size > MAX_UNCOMPRESSED_LEN {
                return Err(NetError::UncompressedTooLarge(size));
            }
            let mut v = pool.get();
            inflater.inflate(rest, size, &mut v)?;
            let mut s = &v[..];
            let id = read_varint(&mut s).map_err(|_| NetError::BadVarInt)?;
            let offset = v.len() - s.len();
            return Ok(InboundFrame {
                id,
                payload: pool.pooled(v, offset),
            });
        }
    }
    let id = read_varint(&mut rest).map_err(|_| NetError::BadVarInt)?;
    let mut v = pool.get();
    v.extend_from_slice(rest);
    Ok(InboundFrame {
        id,
        payload: pool.pooled(v, 0),
    })
}

#[derive(Debug)]
pub struct FrameEncoder {
    threshold: Option<usize>,
    deflater: Deflater,
    body: Vec<u8>,
    packed: Vec<u8>,
    cipher: Option<Encryptor>,
    enc_from: usize,
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameEncoder {
    pub fn new() -> Self {
        Self {
            threshold: None,
            deflater: Deflater::new(),
            body: Vec::new(),
            packed: Vec::new(),
            cipher: None,
            enc_from: 0,
        }
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }

    /// Bytes already in the output batch (`pending_len`) stay plaintext.
    pub fn set_encryption(&mut self, secret: &[u8; SECRET_LEN], pending_len: usize) {
        self.cipher = Some(new_encryptor(secret));
        self.enc_from = pending_len;
    }

    pub fn encode(&mut self, id: i32, payload: &[u8], out: &mut Vec<u8>) -> Result<(), NetError> {
        let mut head = Vec::with_capacity(5);
        write_varint(&mut head, id);
        self.encode_parts(&head, payload, out)
    }

    /// `body` is the packet id followed by the payload.
    pub fn encode_body(&mut self, body: &[u8], out: &mut Vec<u8>) -> Result<(), NetError> {
        self.encode_parts(body, &[], out)
    }

    fn encode_parts(
        &mut self,
        head: &[u8],
        payload: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), NetError> {
        let body_len = head.len() + payload.len();
        match self.threshold {
            Some(t) if body_len >= t => {
                if body_len > MAX_UNCOMPRESSED_LEN {
                    return Err(NetError::UncompressedTooLarge(body_len));
                }
                self.body.clear();
                self.body.extend_from_slice(head);
                self.body.extend_from_slice(payload);
                self.deflater.deflate(&self.body, &mut self.packed)?;
                let total = varint_len(body_len as i32) + self.packed.len();
                check_frame_len(total)?;
                write_varint(out, total as i32);
                write_varint(out, body_len as i32);
                out.extend_from_slice(&self.packed);
            }
            Some(_) => {
                check_frame_len(body_len + 1)?;
                write_varint(out, (body_len + 1) as i32);
                out.push(0);
                out.extend_from_slice(head);
                out.extend_from_slice(payload);
            }
            None => {
                check_frame_len(body_len)?;
                write_varint(out, body_len as i32);
                out.extend_from_slice(head);
                out.extend_from_slice(payload);
            }
        }
        Ok(())
    }

    /// Encrypts the batch in place; clear `out` after writing it.
    pub fn seal(&mut self, out: &mut [u8]) {
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut out[self.enc_from..]);
        }
        self.enc_from = 0;
    }
}

fn check_frame_len(len: usize) -> Result<(), NetError> {
    if len > MAX_FRAME_LEN {
        Err(NetError::FrameTooLarge(len))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pool() -> BufPool {
        BufPool::new(8)
    }

    fn decode_all(dec: &mut FrameDecoder, bytes: &[u8]) -> Result<Vec<(i32, Vec<u8>)>, NetError> {
        let mut r = Cursor::new(bytes.to_vec());
        let mut out = Vec::new();
        loop {
            while let Some(f) = dec.next_frame()? {
                out.push((f.id, f.payload.to_vec()));
            }
            match dec.fill(&mut r) {
                Ok(_) => {}
                Err(NetError::Eof) => return Ok(out),
                Err(e) => return Err(e),
            }
        }
    }

    fn encode(enc: &mut FrameEncoder, id: i32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        enc.encode(id, payload, &mut out).unwrap();
        out
    }

    #[test]
    fn plain_roundtrip() {
        let mut enc = FrameEncoder::new();
        let mut wire = encode(&mut enc, 0x26, b"hello");
        wire.extend(encode(&mut enc, 1, b""));
        let got = decode_all(&mut FrameDecoder::new(pool()), &wire).unwrap();
        assert_eq!(got, vec![(0x26, b"hello".to_vec()), (1, vec![])]);
    }

    #[test]
    fn byte_at_a_time() {
        struct Drip<'a>(&'a [u8]);
        impl Read for Drip<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.0.len().min(1).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let mut enc = FrameEncoder::new();
        enc.set_compression(8);
        let payload = vec![7u8; 300];
        let wire = encode(&mut enc, 5, &payload);
        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(8);
        let mut r = Drip(&wire);
        let frame = loop {
            if let Some(f) = dec.next_frame().unwrap() {
                break f;
            }
            dec.fill(&mut r).unwrap();
        };
        assert_eq!((frame.id, &*frame.payload), (5, &payload[..]));
    }

    #[test]
    fn max_frame_boundary() {
        let mut enc = FrameEncoder::new();
        let payload = vec![1u8; MAX_FRAME_LEN - 1];
        let wire = encode(&mut enc, 2, &payload);
        let got = decode_all(&mut FrameDecoder::new(pool()), &wire).unwrap();
        assert_eq!(got[0].1.len(), payload.len());
        let too_big = vec![1u8; MAX_FRAME_LEN];
        let mut out = Vec::new();
        assert!(matches!(
            enc.encode(2, &too_big, &mut out),
            Err(NetError::FrameTooLarge(_))
        ));
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_bad_length_prefixes() {
        let mut dec = FrameDecoder::new(pool());
        assert!(matches!(
            decode_all(&mut dec, &[0x80, 0x80, 0x80, 0x01]),
            Err(NetError::LengthTooWide)
        ));
        let mut dec = FrameDecoder::new(pool());
        assert!(matches!(
            decode_all(&mut dec, &[0]),
            Err(NetError::ZeroLength)
        ));
    }

    #[test]
    fn truncated_frame_is_eof_not_frame() {
        let mut enc = FrameEncoder::new();
        let wire = encode(&mut enc, 3, b"abcdef");
        let got = decode_all(&mut FrameDecoder::new(pool()), &wire[..wire.len() - 1]).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn compression_threshold_boundary() {
        let mut enc = FrameEncoder::new();
        enc.set_compression(64);
        let at = vec![9u8; 64 - 1];
        let below = vec![9u8; 64 - 2];
        let compressed = encode(&mut enc, 1, &at);
        let plain = encode(&mut enc, 1, &below);
        assert_ne!(compressed[1], 0);
        assert_eq!(compressed[1] as usize, 64);
        assert_eq!(plain[1], 0);

        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(64);
        let mut wire = compressed;
        wire.extend(plain);
        let got = decode_all(&mut dec, &wire).unwrap();
        assert_eq!(got[0].1, at);
        assert_eq!(got[1].1, below);
    }

    #[test]
    fn zero_threshold_compresses_everything() {
        let mut enc = FrameEncoder::new();
        enc.set_compression(0);
        let wire = encode(&mut enc, 1, b"");
        assert_ne!(wire[1], 0);
        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(0);
        assert_eq!(decode_all(&mut dec, &wire).unwrap(), vec![(1, vec![])]);
    }

    #[test]
    fn rejects_declared_size_below_threshold() {
        let mut enc = FrameEncoder::new();
        enc.set_compression(10);
        let wire = encode(&mut enc, 1, &[5u8; 40]);
        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(1000);
        assert!(matches!(
            decode_all(&mut dec, &wire),
            Err(NetError::BelowThreshold { .. })
        ));
    }

    #[test]
    fn rejects_oversize_declared_and_mismatch() {
        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(1);
        let mut body = Vec::new();
        write_varint(&mut body, (MAX_UNCOMPRESSED_LEN + 1) as i32);
        body.push(0);
        let mut wire = Vec::new();
        write_varint(&mut wire, body.len() as i32);
        wire.extend(body);
        assert!(matches!(
            decode_all(&mut dec, &wire),
            Err(NetError::UncompressedTooLarge(_))
        ));

        let mut enc = FrameEncoder::new();
        enc.set_compression(1);
        let mut wire = encode(&mut enc, 1, &[3u8; 50]);
        wire[1] += 1;
        let mut dec = FrameDecoder::new(pool());
        dec.set_compression(1);
        assert!(decode_all(&mut dec, &wire).is_err());
    }

    #[test]
    fn encryption_midstream_roundtrip() {
        let key = [0x42u8; 16];
        let mut enc = FrameEncoder::new();
        let mut out = Vec::new();
        enc.encode(1, b"plain", &mut out).unwrap();
        enc.set_encryption(&key, out.len());
        enc.encode(2, b"secret payload", &mut out).unwrap();
        enc.seal(&mut out);
        assert!(!out.windows(6).any(|w| w == b"secret"));

        let mut dec = FrameDecoder::new(pool());
        let mut r = Cursor::new(out);
        dec.fill(&mut r).unwrap_or(0);
        let first = dec.next_frame().unwrap().unwrap();
        assert_eq!((first.id, &*first.payload), (1, &b"plain"[..]));
        dec.set_encryption_at_boundary(&key);
        let second = dec.next_frame().unwrap().unwrap();
        assert_eq!((second.id, &*second.payload), (2, &b"secret payload"[..]));
    }

    #[test]
    fn cfb8_matches_nist_vector() {
        let key = hex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = hex("000102030405060708090a0b0c0d0e0f");
        let mut data = hex("6bc1bee22e409f96e93d7e117393172aae2d");
        let mut e = Encryptor::new(
            &<[u8; 16]>::try_from(key).unwrap().into(),
            &<[u8; 16]>::try_from(iv).unwrap().into(),
        );
        e.encrypt(&mut data);
        assert_eq!(data, hex("3b79424c9c0dd436bace9e0ed4586a4f32b9"));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
