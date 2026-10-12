use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use lewton::inside_ogg::OggStreamReader;

use crate::error::AudioError;

#[derive(Debug)]
pub struct Pcm {
    pub channels: u8,
    pub sample_rate: u32,
    pub samples: Box<[f32]>,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn mono(sample_rate: u32, samples: Vec<f32>) -> Self {
        Self {
            channels: 1,
            sample_rate,
            samples: samples.into(),
        }
    }

    /// Linear resample to `rate`; the result keeps the channel count.
    pub fn resampled(&self, rate: u32) -> Self {
        let ch = self.channels.max(1) as usize;
        let frames = self.frames();
        if rate == self.sample_rate || rate == 0 || self.sample_rate == 0 || frames == 0 {
            return Self {
                channels: self.channels,
                sample_rate: if frames == 0 { rate } else { self.sample_rate },
                samples: self.samples.clone(),
            };
        }
        let out_frames =
            (frames as u128 * u128::from(rate)).div_ceil(u128::from(self.sample_rate)) as usize;
        let step = f64::from(self.sample_rate) / f64::from(rate);
        let mut samples = Vec::with_capacity(out_frames * ch);
        for i in 0..out_frames {
            let at = i as f64 * step;
            let a = (at as usize).min(frames - 1);
            let b = (a + 1).min(frames - 1);
            let t = (at - a as f64) as f32;
            for c in 0..ch {
                let x = self.samples[a * ch + c];
                let y = self.samples[b * ch + c];
                samples.push(x + (y - x) * t);
            }
        }
        Self {
            channels: self.channels,
            sample_rate: rate,
            samples: samples.into(),
        }
    }
}

fn reader(bytes: Vec<u8>) -> Result<OggStreamReader<Cursor<Vec<u8>>>, AudioError> {
    OggStreamReader::new(Cursor::new(bytes)).map_err(|e| AudioError::Decode(e.to_string()))
}

fn to_f32_stereo_max(packet: &[i16], src_channels: usize, out: &mut Vec<f32>) {
    let keep = src_channels.min(2);
    for frame in packet.chunks_exact(src_channels) {
        out.extend(frame[..keep].iter().map(|&s| f32::from(s) / 32768.0));
    }
}

pub fn decode_ogg(bytes: Vec<u8>) -> Result<Pcm, AudioError> {
    let mut r = reader(bytes)?;
    let src_channels = r.ident_hdr.audio_channels as usize;
    let mut samples = Vec::new();
    while let Some(packet) = r
        .read_dec_packet_itl()
        .map_err(|e| AudioError::Decode(e.to_string()))?
    {
        to_f32_stereo_max(&packet, src_channels, &mut samples);
    }
    Ok(Pcm {
        channels: src_channels.min(2) as u8,
        sample_rate: r.ident_hdr.audio_sample_rate,
        samples: samples.into(),
    })
}

const STREAM_RING_SECONDS: usize = 1;
const STREAM_PREFILL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct StreamSource {
    pub(crate) consumer: rtrb::Consumer<f32>,
    pub(crate) channels: u8,
    pub(crate) sample_rate: u32,
    pub(crate) finished: Arc<AtomicBool>,
    pub(crate) underruns: Arc<AtomicU64>,
}

impl StreamSource {
    pub fn channels(&self) -> u8 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(crate) fn pull_frame(&mut self) -> Pull {
        let ch = self.channels as usize;
        if self.consumer.slots() < ch {
            if self.finished.load(Ordering::Acquire) && self.consumer.slots() < ch {
                return Pull::End;
            }
            self.underruns.fetch_add(1, Ordering::Relaxed);
            return Pull::Underrun;
        }
        let l = self.consumer.pop().unwrap_or(0.0);
        let r = if ch == 2 {
            self.consumer.pop().unwrap_or(0.0)
        } else {
            l
        };
        Pull::Frame([l, r])
    }
}

pub(crate) enum Pull {
    Frame([f32; 2]),
    Underrun,
    End,
}

pub fn open_stream(bytes: Vec<u8>) -> Result<StreamSource, AudioError> {
    let mut r = reader(bytes)?;
    let src_channels = r.ident_hdr.audio_channels as usize;
    let channels = src_channels.min(2) as u8;
    let sample_rate = r.ident_hdr.audio_sample_rate;
    let capacity = sample_rate as usize * channels as usize * STREAM_RING_SECONDS;
    let (mut producer, consumer) = rtrb::RingBuffer::new(capacity);
    let finished = Arc::new(AtomicBool::new(false));
    let done = finished.clone();

    thread::Builder::new()
        .name("mcr-audio-stream".into())
        .spawn(move || {
            let mut pending = Vec::new();
            let mut offset = 0;
            loop {
                if offset == pending.len() {
                    pending.clear();
                    offset = 0;
                    match r.read_dec_packet_itl() {
                        Ok(Some(packet)) => to_f32_stereo_max(&packet, src_channels, &mut pending),
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!("stream decode error: {e}");
                            break;
                        }
                    }
                }
                if producer.is_abandoned() {
                    return;
                }
                let n = producer.slots().min(pending.len() - offset);
                if n == 0 {
                    thread::sleep(Duration::from_millis(4));
                    continue;
                }
                for v in &pending[offset..offset + n] {
                    let _ = producer.push(*v);
                }
                offset += n;
            }
            done.store(true, Ordering::Release);
        })
        .map_err(|e| AudioError::Decode(e.to_string()))?;

    let start = Instant::now();
    let want = capacity.min(sample_rate as usize * channels as usize / 10);
    while consumer.slots() < want
        && !finished.load(Ordering::Acquire)
        && start.elapsed() < STREAM_PREFILL
    {
        thread::sleep(Duration::from_millis(1));
    }

    Ok(StreamSource {
        consumer,
        channels,
        sample_rate,
        finished,
        underruns: Arc::default(),
    })
}

/// The producing end of a pushed stream: whole interleaved frames go in, the mixer plays them.
#[derive(Debug)]
pub struct PcmProducer {
    producer: rtrb::Producer<f32>,
    channels: u8,
    finished: Arc<AtomicBool>,
}

impl PcmProducer {
    pub fn channels(&self) -> u8 {
        self.channels
    }

    pub fn capacity_frames(&self) -> usize {
        self.producer.buffer().capacity() / self.channels as usize
    }

    pub fn free_frames(&self) -> usize {
        self.producer.slots() / self.channels as usize
    }

    /// Writes up to `frames` whole frames taken from `samples`; returns the frames written.
    pub fn push(&mut self, frames: usize, samples: impl Iterator<Item = f32>) -> usize {
        let n = frames.min(self.free_frames());
        if n == 0 {
            return 0;
        }
        let ch = self.channels as usize;
        let Ok(chunk) = self.producer.write_chunk_uninit(n * ch) else {
            return 0;
        };
        chunk.fill_from_iter(samples.take(n * ch)) / ch
    }

    /// No more data: the voice ends once the ring drains.
    pub fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }

    /// The voice reading this stream is gone.
    pub fn is_abandoned(&self) -> bool {
        self.producer.is_abandoned()
    }
}

impl Drop for PcmProducer {
    fn drop(&mut self) {
        self.finish();
    }
}

/// A stream fed by the caller instead of a decoder thread; underruns add to `underruns`.
pub fn pushed_stream(
    sample_rate: u32,
    channels: u8,
    capacity_frames: usize,
    underruns: Arc<AtomicU64>,
) -> (PcmProducer, StreamSource) {
    let channels = channels.clamp(1, 2);
    let (producer, consumer) = rtrb::RingBuffer::new(capacity_frames.max(1) * channels as usize);
    let finished = Arc::new(AtomicBool::new(false));
    let source = StreamSource {
        consumer,
        channels,
        sample_rate,
        finished: finished.clone(),
        underruns,
    };
    let producer = PcmProducer {
        producer,
        channels,
        finished,
    };
    (producer, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pull_all(source: &mut StreamSource, max: usize) -> (Vec<[f32; 2]>, usize, bool) {
        let (mut frames, mut underruns) = (Vec::new(), 0);
        for _ in 0..max {
            match source.pull_frame() {
                Pull::Frame(f) => frames.push(f),
                Pull::Underrun => underruns += 1,
                Pull::End => return (frames, underruns, true),
            }
        }
        (frames, underruns, false)
    }

    #[test]
    fn pushed_frames_come_out_in_order_and_partial_frames_are_refused() {
        let counter = Arc::new(AtomicU64::new(0));
        let (mut tx, mut rx) = pushed_stream(48_000, 2, 4, counter.clone());
        assert_eq!(tx.capacity_frames(), 4);
        assert_eq!(tx.push(10, (0..20).map(|i| i as f32)), 4);
        assert_eq!(tx.free_frames(), 0);
        assert_eq!(tx.push(1, [9.0, 9.0].into_iter()), 0);
        let (frames, underruns, ended) = pull_all(&mut rx, 4);
        assert_eq!(frames, [[0.0, 1.0], [2.0, 3.0], [4.0, 5.0], [6.0, 7.0]]);
        assert_eq!((underruns, ended), (0, false));
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn empty_pushed_ring_counts_underruns_until_data_arrives_then_drains() {
        let counter = Arc::new(AtomicU64::new(0));
        let (mut tx, mut rx) = pushed_stream(48_000, 1, 8, counter.clone());
        let (_, underruns, _) = pull_all(&mut rx, 3);
        assert_eq!(underruns, 3);
        assert_eq!(counter.load(Ordering::Relaxed), 3);
        tx.push(2, [0.5, 0.25].into_iter());
        tx.finish();
        let (frames, underruns, ended) = pull_all(&mut rx, 10);
        assert_eq!(frames, [[0.5, 0.5], [0.25, 0.25]]);
        assert_eq!(underruns, 0);
        assert!(ended);
    }

    #[test]
    fn resampling_doubles_frames_and_keeps_the_ramp() {
        let pcm = Pcm::mono(24_000, (0..100).map(|i| i as f32).collect());
        let up = pcm.resampled(48_000);
        assert_eq!(up.sample_rate, 48_000);
        assert_eq!(up.frames(), 200);
        assert!((up.samples[51] - 25.5).abs() < 1e-4);
        assert_eq!(pcm.resampled(24_000).frames(), 100);
    }
}
