use super::{SOUND_FORMAT_BIG_ENDIAN, SOUND_FORMAT_BITS, SOUND_FORMAT_SIGNED};

pub const MIN_RATE: u32 = 8_000;
pub const MAX_RATE: u32 = 192_000;

/// A `javax.sound.sampled.AudioFormat` that OpenAL could have played: 8 or 16 bit, mono or stereo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmFormat {
    pub rate: u32,
    pub channels: u8,
    pub bits: u8,
    pub signed: bool,
    pub big_endian: bool,
}

impl PcmFormat {
    pub fn parse(rate: i32, channels: i32, flags: i32) -> Option<Self> {
        let rate = u32::try_from(rate)
            .ok()
            .filter(|r| (MIN_RATE..=MAX_RATE).contains(r))?;
        let channels = u8::try_from(channels)
            .ok()
            .filter(|c| (1..=2).contains(c))?;
        let bits = (flags & SOUND_FORMAT_BITS) as u8;
        if bits != 8 && bits != 16 {
            return None;
        }
        Some(Self {
            rate,
            channels,
            bits,
            signed: flags & SOUND_FORMAT_SIGNED != 0,
            big_endian: flags & SOUND_FORMAT_BIG_ENDIAN != 0,
        })
    }

    pub fn sample_bytes(&self) -> usize {
        usize::from(self.bits / 8)
    }

    pub fn frame_bytes(&self) -> usize {
        self.sample_bytes() * usize::from(self.channels)
    }

    /// Whole frames in `len` bytes.
    pub fn frames_in(&self, len: usize) -> usize {
        len / self.frame_bytes()
    }

    /// Sample `i` of `bytes`, which must hold at least `i + 1` samples.
    pub fn sample(&self, bytes: &[u8], i: usize) -> f32 {
        if self.bits == 8 {
            let b = bytes[i];
            return if self.signed {
                f32::from(b as i8) / 128.0
            } else {
                (f32::from(b) - 128.0) / 128.0
            };
        }
        let pair = [bytes[2 * i], bytes[2 * i + 1]];
        let raw = if self.big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        };
        if self.signed {
            f32::from(raw as i16) / 32768.0
        } else {
            (f32::from(raw) - 32768.0) / 32768.0
        }
    }

    /// Every whole frame of `bytes` as interleaved samples.
    pub fn samples<'a>(&'a self, bytes: &'a [u8]) -> impl Iterator<Item = f32> + 'a {
        let n = self.frames_in(bytes.len()) * usize::from(self.channels);
        (0..n).map(move |i| self.sample(bytes, i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S16LE: i32 = 16 | SOUND_FORMAT_SIGNED;

    #[test]
    fn parses_only_formats_openal_plays() {
        let f = PcmFormat::parse(44_100, 2, S16LE).unwrap();
        assert_eq!((f.frame_bytes(), f.signed, f.big_endian), (4, true, false));
        assert!(PcmFormat::parse(44_100, 3, S16LE).is_none());
        assert!(PcmFormat::parse(44_100, 0, S16LE).is_none());
        assert!(PcmFormat::parse(-1, 1, S16LE).is_none());
        assert!(PcmFormat::parse(10_000_000, 1, S16LE).is_none());
        assert!(PcmFormat::parse(7_999, 1, S16LE).is_none());
        assert!(PcmFormat::parse(192_001, 1, S16LE).is_none());
        assert!(PcmFormat::parse(44_100, 1, 24 | SOUND_FORMAT_SIGNED).is_none());
        assert!(PcmFormat::parse(44_100, 1, 8).is_some());
    }

    #[test]
    fn converts_every_sample_layout() {
        let le = PcmFormat::parse(48_000, 1, S16LE).unwrap();
        let s: Vec<f32> = le.samples(&[0x00, 0x80, 0xff, 0x7f, 0x00]).collect();
        assert_eq!(s, [-1.0, 32767.0 / 32768.0]);
        let be = PcmFormat::parse(48_000, 1, S16LE | SOUND_FORMAT_BIG_ENDIAN).unwrap();
        assert_eq!(be.samples(&[0x80, 0x00]).next(), Some(-1.0));
        let u16 = PcmFormat::parse(48_000, 1, 16).unwrap();
        assert_eq!(u16.samples(&[0x00, 0x80]).next(), Some(0.0));
        let u8 = PcmFormat::parse(48_000, 1, 8).unwrap();
        assert_eq!(u8.samples(&[0, 128]).collect::<Vec<_>>(), [-1.0, 0.0]);
        let s8 = PcmFormat::parse(48_000, 1, 8 | SOUND_FORMAT_SIGNED).unwrap();
        assert_eq!(s8.samples(&[0x80]).next(), Some(-1.0));
    }

    #[test]
    fn partial_frames_are_dropped() {
        let f = PcmFormat::parse(48_000, 2, S16LE).unwrap();
        assert_eq!(f.samples(&[0; 7]).count(), 2);
    }
}
