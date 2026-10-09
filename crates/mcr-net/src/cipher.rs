use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Block};

const CHUNK: usize = 64;

/// CFB8 decryption has no serial dependency (every keystream byte depends only on already
/// known ciphertext), so whole chunks go through the block cipher in parallel.
pub struct Cfb8Decryptor {
    aes: Aes128,
    history: [u8; 16],
}

impl Cfb8Decryptor {
    pub fn new(secret: &[u8; 16]) -> Self {
        Self {
            aes: Aes128::new(&(*secret).into()),
            history: *secret,
        }
    }

    pub fn decrypt(&mut self, buf: &mut [u8]) {
        let mut window = [0u8; 16 + CHUNK];
        let mut blocks = [Block::default(); CHUNK];
        for chunk in buf.chunks_mut(CHUNK) {
            let n = chunk.len();
            window[..16].copy_from_slice(&self.history);
            window[16..16 + n].copy_from_slice(chunk);
            for (i, block) in blocks[..n].iter_mut().enumerate() {
                block.copy_from_slice(&window[i..i + 16]);
            }
            self.aes.encrypt_blocks(&mut blocks[..n]);
            for (byte, block) in chunk.iter_mut().zip(&blocks[..n]) {
                *byte ^= block[0];
            }
            self.history.copy_from_slice(&window[n..n + 16]);
        }
    }
}

impl std::fmt::Debug for Cfb8Decryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cfb8Decryptor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::KeyIvInit;

    #[test]
    fn matches_reference_across_arbitrary_splits() {
        let key = *b"0123456789abcdef";
        let plain: Vec<u8> = (0..1000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut wire = plain.clone();
        cfb8::Encryptor::<Aes128>::new(&key.into(), &key.into()).encrypt(&mut wire);

        let mut mine = Cfb8Decryptor::new(&key);
        let mut out = wire.clone();
        let mut at = 0;
        for step in [1usize, 7, 64, 65, 3, 200, 1, 659] {
            let end = (at + step).min(out.len());
            mine.decrypt(&mut out[at..end]);
            at = end;
        }
        assert_eq!(at, out.len());
        assert_eq!(out, plain);

        let mut reference = wire;
        cfb8::Decryptor::<Aes128>::new(&key.into(), &key.into()).decrypt(&mut reference);
        assert_eq!(reference, plain);
    }
}
