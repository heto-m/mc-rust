use std::sync::Arc;
use std::sync::atomic::Ordering;

use mcr_audio::{
    Command, CommandProducer, EXTERNAL_VOICE_BIT, PlayRequest, SoundSource, Source, Spatial,
    VoiceId,
};
use mcr_jvm::RingReader;

use super::{
    SND_ARG, SND_ATTENUATION, SND_BUFFER, SND_CREATE, SND_DESTROY, SND_LOOPING, SND_PAUSE,
    SND_PITCH, SND_PLAY, SND_POSITION, SND_RELATIVE, SND_RESUME, SND_SIZE, SND_SLOT, SND_STOP,
    SND_STREAM, SND_VOLUME, SND_X, SND_Y, SND_Z, SOUND_SLOTS, STARTED_LOG, SoundShared, Taken,
    lock,
};

/// Commands decoded per mixed chunk; the rest wait for the next chunk.
const MAX_PER_POLL: usize = 1024;
/// Beyond the 30M-block world border.
const MAX_COORD: f32 = 1.0e8;
const MAX_ATTENUATION: f32 = 1.0e6;

/// A voice Java started, logged for diagnostics and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceStart {
    pub slot: u16,
    pub generation: u32,
    /// The static buffer id, or `None` for a pushed stream.
    pub buffer: Option<u32>,
    pub channels: u8,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SlotSource {
    None,
    Buffer(u32),
    Stream(u32),
}

#[derive(Debug, Clone, Copy)]
struct SlotParams {
    volume: f32,
    pitch: f32,
    position: [f32; 3],
    looping: bool,
    attenuation: Option<f32>,
    relative: bool,
    source: SlotSource,
    playing: bool,
}

impl Default for SlotParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch: 1.0,
            position: [0.0; 3],
            looping: false,
            attenuation: None,
            relative: false,
            source: SlotSource::None,
            playing: false,
        }
    }
}

impl SlotParams {
    fn spatial(&self) -> Spatial {
        let max_distance = self.attenuation.unwrap_or(f32::INFINITY);
        if self.relative {
            Spatial::Relative {
                offset: self.position,
                max_distance,
            }
        } else {
            Spatial::Positioned {
                position: self.position,
                max_distance,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Raw {
    kind: u16,
    slot: usize,
    arg: u32,
    v: [f32; 3],
}

impl Raw {
    fn decode(kind: u16, p: &[u8]) -> Option<Self> {
        if p.len() < SND_SIZE {
            return None;
        }
        let u32_at = |o: usize| u32::from_ne_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
        let f32_at = |o: usize| f32::from_bits(u32_at(o));
        Some(Self {
            kind,
            slot: u32_at(SND_SLOT) as usize,
            arg: u32_at(SND_ARG),
            v: [f32_at(SND_X), f32_at(SND_Y), f32_at(SND_Z)],
        })
    }
}

fn voice(slot: usize) -> VoiceId {
    VoiceId(EXTERNAL_VOICE_BIT | slot as u32)
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

/// Decodes Java's channel commands into mixer commands on the audio thread.
#[derive(Debug)]
pub struct BridgeVoices {
    shared: Arc<SoundShared>,
    reader: RingReader,
    slots: Box<[SlotParams]>,
    pending: Option<Raw>,
    broken: bool,
    started: rtrb::Producer<VoiceStart>,
}

impl BridgeVoices {
    pub(super) fn new(shared: Arc<SoundShared>, out_rate: u32) -> Option<Self> {
        let reader = shared.ring.reader()?;
        let (started, log) = rtrb::RingBuffer::new(STARTED_LOG);
        *lock(&shared.started) = Some(log);
        let out_rate = if (super::pcm::MIN_RATE..=super::pcm::MAX_RATE).contains(&out_rate) {
            out_rate
        } else {
            0
        };
        shared.out_rate.store(out_rate, Ordering::Relaxed);
        shared.control.attached.store(1, Ordering::Release);
        Some(Self {
            shared,
            reader,
            slots: vec![SlotParams::default(); SOUND_SLOTS].into_boxed_slice(),
            pending: None,
            broken: false,
            started,
        })
    }

    /// `false` leaves the command for the next poll: a table was busy.
    fn process(&mut self, raw: Raw, apply: &mut dyn FnMut(Command)) -> bool {
        let Some(params) = self.slots.get_mut(raw.slot) else {
            self.shared
                .counters
                .bad_commands
                .fetch_add(1, Ordering::Relaxed);
            return true;
        };
        let id = voice(raw.slot);
        let [x, y, z] = raw.v;
        match raw.kind {
            SND_CREATE | SND_DESTROY => {
                apply(Command::Stop(id));
                *params = SlotParams::default();
            }
            SND_STOP => {
                apply(Command::Stop(id));
                params.playing = false;
            }
            SND_PAUSE if params.playing => apply(Command::Pause(id)),
            SND_RESUME if params.playing => apply(Command::Resume(id)),
            SND_PAUSE | SND_RESUME => {}
            SND_POSITION => {
                params.position = [x, y, z].map(|c| finite_or(c, 0.0).clamp(-MAX_COORD, MAX_COORD));
                if params.playing {
                    apply(Command::SetVoicePosition(id, params.position));
                }
            }
            SND_PITCH => {
                params.pitch = finite_or(x, 1.0).clamp(0.5, 2.0);
                if params.playing {
                    apply(Command::SetVoicePitch(id, params.pitch));
                }
            }
            SND_VOLUME => {
                params.volume = finite_or(x, 0.0).clamp(0.0, 1.0);
                if params.playing {
                    apply(Command::SetVoiceVolume(id, params.volume));
                }
            }
            SND_LOOPING => {
                params.looping = raw.arg != 0;
                if params.playing {
                    apply(Command::SetVoiceLooping(id, params.looping));
                }
            }
            SND_ATTENUATION | SND_RELATIVE => {
                if raw.kind == SND_RELATIVE {
                    params.relative = raw.arg != 0;
                } else {
                    params.attenuation = (x.is_finite() && x > 0.0).then(|| x.min(MAX_ATTENUATION));
                }
                if params.playing {
                    apply(Command::SetVoiceSpatial(id, params.spatial()));
                }
            }
            SND_BUFFER => params.source = SlotSource::Buffer(raw.arg),
            SND_STREAM => params.source = SlotSource::Stream(raw.arg),
            SND_PLAY => return self.play(raw, apply),
            _ => {
                self.shared
                    .counters
                    .bad_commands
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        true
    }

    fn play(&mut self, raw: Raw, apply: &mut dyn FnMut(Command)) -> bool {
        let params = self.slots[raw.slot];
        let source = match params.source {
            SlotSource::None => Taken::Missing,
            SlotSource::Buffer(b) => match self.shared.buffer(b) {
                Taken::Found(pcm) => Taken::Found(Source::Static(pcm)),
                Taken::Missing => Taken::Missing,
                Taken::Busy => Taken::Busy,
            },
            SlotSource::Stream(serial) => match self.shared.take_stream(raw.slot, serial) {
                Taken::Found(s) => Taken::Found(Source::Stream(s)),
                Taken::Missing => Taken::Missing,
                Taken::Busy => Taken::Busy,
            },
        };
        let id = voice(raw.slot);
        let source = match source {
            Taken::Found(s) => s,
            Taken::Busy => return false,
            Taken::Missing => {
                self.shared
                    .counters
                    .missing_sources
                    .fetch_add(1, Ordering::Relaxed);
                apply(Command::Stop(id));
                self.slots[raw.slot].playing = false;
                drop(self.shared.states.link(raw.slot, raw.arg));
                return true;
            }
        };
        apply(Command::Stop(id));
        let link = self.shared.states.link(raw.slot, raw.arg);
        let (channels, sample_rate, buffer) = match (&source, params.source) {
            (Source::Static(pcm), SlotSource::Buffer(b)) => {
                (pcm.channels, pcm.sample_rate, Some(b))
            }
            (Source::Stream(s), _) => (s.channels(), s.sample_rate(), None),
            (Source::Static(pcm), _) => (pcm.channels, pcm.sample_rate, None),
        };
        let looping = params.looping && buffer.is_some();
        apply(Command::Play(
            id,
            PlayRequest {
                volume: params.volume,
                pitch: params.pitch,
                spatial: params.spatial(),
                looping,
                premixed: true,
                state: link,
                ..PlayRequest::new(source, SoundSource::Master)
            },
        ));
        self.slots[raw.slot].playing = true;
        self.shared.counters.plays.fetch_add(1, Ordering::Relaxed);
        let _ = self.started.push(VoiceStart {
            slot: raw.slot as u16,
            generation: raw.arg,
            buffer,
            channels,
            sample_rate,
        });
        true
    }

    fn next_raw(&mut self) -> Result<Option<Option<Raw>>, ()> {
        match self.reader.try_pop() {
            Ok(Some(msg)) => Ok(Some(Raw::decode(msg.kind(), msg.payload()))),
            Ok(None) => Ok(None),
            Err(_) => Err(()),
        }
    }
}

impl CommandProducer for BridgeVoices {
    fn poll(&mut self, apply: &mut dyn FnMut(Command)) {
        if self.broken {
            return;
        }
        if let Some(raw) = self.pending.take()
            && !self.process(raw, apply)
        {
            self.pending = Some(raw);
            return;
        }
        for _ in 0..MAX_PER_POLL {
            let raw = match self.next_raw() {
                Ok(Some(Some(raw))) => raw,
                Ok(Some(None)) => {
                    self.shared
                        .counters
                        .bad_commands
                        .fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Ok(None) => return,
                Err(()) => {
                    tracing::error!("Java's sound ring is corrupt; Java sounds stop");
                    self.broken = true;
                    for slot in 0..SOUND_SLOTS {
                        apply(Command::Stop(voice(slot)));
                    }
                    self.shared.states.stop_all();
                    return;
                }
            };
            if !self.process(raw, apply) {
                self.pending = Some(raw);
                return;
            }
        }
    }
}

impl Drop for BridgeVoices {
    fn drop(&mut self) {
        self.shared.control.attached.store(0, Ordering::Release);
        self.shared.states.stop_all();
    }
}
