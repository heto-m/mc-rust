use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
            return if self.finished.load(Ordering::Acquire) && self.consumer.slots() < ch {
                Pull::End
            } else {
                Pull::Underrun
            };
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
    })
}
