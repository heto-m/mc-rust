//! Java's sound engine played by mcr-audio: `Channel` operations arrive on a command ring, PCM by downcall.

mod owner;
mod pcm;
mod voices;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use mcr_audio::{Pcm, PcmProducer, StreamSource, VoiceStates, pushed_stream};
use mcr_jvm::{Ring, RingError};

pub use owner::{SoundOwner, event_owner};
pub use pcm::PcmFormat;
pub use voices::{BridgeVoices, VoiceStart};

use crate::BridgeError;

pub const SND_SLOT: usize = 0;
pub const SND_ARG: usize = 4;
pub const SND_X: usize = 8;
pub const SND_Y: usize = 12;
pub const SND_Z: usize = 16;
pub const SND_SIZE: usize = 20;

pub const SND_CREATE: u16 = 1;
pub const SND_DESTROY: u16 = 2;
pub const SND_PLAY: u16 = 3;
pub const SND_PAUSE: u16 = 4;
pub const SND_RESUME: u16 = 5;
pub const SND_STOP: u16 = 6;
pub const SND_POSITION: u16 = 7;
pub const SND_PITCH: u16 = 8;
pub const SND_VOLUME: u16 = 9;
pub const SND_LOOPING: u16 = 10;
pub const SND_ATTENUATION: u16 = 11;
pub const SND_RELATIVE: u16 = 12;
pub const SND_BUFFER: u16 = 13;
pub const SND_STREAM: u16 = 14;

pub const CTL_ATTACHED: usize = 0;
pub const CTL_VOLUME_SEQ: usize = 4;
pub const CTL_VOLUMES: usize = 8;
pub const CTL_DROPPED: usize = 52;
pub const CTL_SIZE: usize = 64;

/// Java's `SoundSource` count; the volume table holds one slider value per ordinal.
pub const SOURCES: usize = 11;
/// Bridge voice slots: vanilla sizes its pools for a 255-source device (247 static, 8 streaming).
pub const SOUND_SLOTS: usize = 256;
pub const SOUND_RING_CAPACITY: usize = 128 << 10;
pub const STREAM_END: i32 = -1;

pub const SOUND_FORMAT_BITS: i32 = 0xFF;
pub const SOUND_FORMAT_SIGNED: i32 = 0x100;
pub const SOUND_FORMAT_BIG_ENDIAN: i32 = 0x200;

pub const MAX_BUFFERS: usize = 16_384;
/// Decoded static PCM kept for Java, in bytes of f32 samples.
pub const BUFFER_BUDGET: usize = 256 << 20;
pub const MAX_UPLOAD_BYTES: usize = 64 << 20;
/// Open pushed streams; vanilla's streaming pool has 8 channels.
pub const MAX_STREAMS: usize = 16;
const STREAM_RING_SECONDS: usize = 1;
const STARTED_LOG: usize = 1024;

#[repr(C, align(64))]
#[derive(Debug, Default)]
struct Control {
    attached: AtomicU32,
    volume_seq: AtomicU32,
    volumes: [AtomicU32; SOURCES],
    dropped: AtomicU32,
    _pad: [u32; 2],
}

const _: () = {
    assert!(size_of::<Control>() == CTL_SIZE);
    assert!(std::mem::offset_of!(Control, attached) == CTL_ATTACHED);
    assert!(std::mem::offset_of!(Control, volume_seq) == CTL_VOLUME_SEQ);
    assert!(std::mem::offset_of!(Control, volumes) == CTL_VOLUMES);
    assert!(std::mem::offset_of!(Control, dropped) == CTL_DROPPED);
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the audio thread got from a table it may only `try_lock`.
#[derive(Debug)]
enum Taken<T> {
    Found(T),
    Missing,
    Busy,
}

fn try_with<T, R>(m: &Mutex<T>, f: impl FnOnce(&mut T) -> Option<R>) -> Taken<R> {
    let mut guard = match m.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return Taken::Busy,
    };
    f(&mut guard).map_or(Taken::Missing, Taken::Found)
}

#[derive(Debug)]
struct BufferTable {
    entries: Vec<Option<Arc<Pcm>>>,
    generations: Vec<u16>,
    free: Vec<u16>,
    bytes: usize,
    budget: usize,
}

impl Default for BufferTable {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
            bytes: 0,
            budget: BUFFER_BUDGET,
        }
    }
}

/// Bytes of f32 PCM an upload decodes to, the resampled copy included; `None` on overflow.
fn upload_cost(len: usize, format: PcmFormat, out_rate: u32) -> Option<usize> {
    let channels = usize::from(format.channels);
    let frames = format.frames_in(len);
    let decoded = frames
        .checked_mul(channels)?
        .checked_mul(size_of::<f32>())?;
    if out_rate == 0 || out_rate == format.rate {
        return Some(decoded);
    }
    let out_frames = frames
        .checked_mul(out_rate as usize)?
        .div_ceil(format.rate as usize);
    out_frames
        .checked_mul(channels)?
        .checked_mul(size_of::<f32>())?
        .checked_add(decoded)
}

impl BufferTable {
    fn room(&self) -> usize {
        self.budget.saturating_sub(self.bytes)
    }

    fn split(id: u32) -> (usize, u16) {
        ((id & 0xFFFF) as usize, (id >> 16) as u16)
    }

    fn insert(&mut self, pcm: Arc<Pcm>) -> Option<u32> {
        let size = pcm.samples.len().checked_mul(size_of::<f32>())?;
        if size > self.room() {
            return None;
        }
        let index = match self.free.pop() {
            Some(i) => usize::from(i),
            None if self.entries.len() < MAX_BUFFERS => {
                self.entries.push(None);
                self.generations.push(1);
                self.entries.len() - 1
            }
            None => return None,
        };
        self.entries[index] = Some(pcm);
        self.bytes += size;
        Some((u32::from(self.generations[index]) << 16) | index as u32)
    }

    fn get(&self, id: u32) -> Option<Arc<Pcm>> {
        let (index, generation) = Self::split(id);
        (self.generations.get(index) == Some(&generation))
            .then(|| self.entries[index].clone())
            .flatten()
    }

    fn remove(&mut self, id: u32) -> Option<Arc<Pcm>> {
        let (index, generation) = Self::split(id);
        if self.generations.get(index) != Some(&generation) {
            return None;
        }
        let pcm = self.entries[index].take()?;
        self.bytes -= pcm.samples.len() * size_of::<f32>();
        self.generations[index] = generation.checked_add(1).unwrap_or(1);
        self.free.push(index as u16);
        Some(pcm)
    }
}

#[derive(Debug)]
struct StreamEntry {
    serial: u32,
    format: PcmFormat,
    producer: PcmProducer,
    source: Option<StreamSource>,
}

#[derive(Debug, Default)]
struct Counters {
    uploads: AtomicU64,
    rejected_uploads: AtomicU64,
    plays: AtomicU64,
    missing_sources: AtomicU64,
    bad_commands: AtomicU64,
}

/// A snapshot of the sound bridge's counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundStats {
    pub uploads: u64,
    pub rejected_uploads: u64,
    pub buffered_bytes: u64,
    /// Voices started by Java's `Channel.play()`.
    pub plays: u64,
    pub missing_sources: u64,
    pub bad_commands: u64,
    pub underruns: u64,
    /// Commands Java dropped because the ring stayed full.
    pub dropped: u64,
}

/// Process-wide because the JVM is: Java keeps these addresses for as long as it lives.
#[derive(Debug)]
pub(crate) struct SoundShared {
    ring: Arc<Ring>,
    states: Arc<VoiceStates>,
    control: Box<Control>,
    buffers: Mutex<BufferTable>,
    streams: Mutex<Vec<Option<StreamEntry>>>,
    out_rate: AtomicU32,
    underruns: Arc<AtomicU64>,
    counters: Counters,
    started: Mutex<Option<rtrb::Consumer<VoiceStart>>>,
}

impl SoundShared {
    pub(crate) fn new() -> Result<Self, RingError> {
        Ok(Self {
            ring: Ring::new(SOUND_RING_CAPACITY)?,
            states: VoiceStates::new(SOUND_SLOTS),
            control: Box::default(),
            buffers: Mutex::default(),
            streams: Mutex::new((0..SOUND_SLOTS).map(|_| None).collect()),
            out_rate: AtomicU32::new(0),
            underruns: Arc::default(),
            counters: Counters::default(),
            started: Mutex::new(None),
        })
    }

    pub(crate) fn ring_addr(&self) -> i64 {
        self.ring.addr()
    }

    pub(crate) fn states_addr(&self) -> i64 {
        self.states.as_ptr() as i64
    }

    pub(crate) fn control_addr(&self) -> i64 {
        std::ptr::from_ref::<Control>(&self.control) as i64
    }

    fn upload(&self, bytes: &[u8], format: PcmFormat) -> u32 {
        let out = self.out_rate.load(Ordering::Relaxed);
        let fits = format.frames_in(bytes.len()) > 0
            && bytes.len() <= MAX_UPLOAD_BYTES
            && upload_cost(bytes.len(), format, out)
                .is_some_and(|cost| cost <= lock(&self.buffers).room());
        if !fits {
            self.counters
                .rejected_uploads
                .fetch_add(1, Ordering::Relaxed);
            return 0;
        }
        let mut pcm = Pcm {
            channels: format.channels,
            sample_rate: format.rate,
            samples: format.samples(bytes).collect(),
        };
        if out != 0 && out != format.rate {
            pcm = pcm.resampled(out);
        }
        match lock(&self.buffers).insert(Arc::new(pcm)) {
            Some(id) => {
                self.counters.uploads.fetch_add(1, Ordering::Relaxed);
                id
            }
            None => {
                self.counters
                    .rejected_uploads
                    .fetch_add(1, Ordering::Relaxed);
                0
            }
        }
    }

    fn free(&self, id: u32) {
        let released = lock(&self.buffers).remove(id);
        drop(released);
    }

    fn stream_open(&self, slot: usize, serial: u32, format: PcmFormat) -> bool {
        let Some(capacity) = (format.rate as usize).checked_mul(STREAM_RING_SECONDS) else {
            return false;
        };
        if slot >= SOUND_SLOTS {
            return false;
        }
        let (producer, source) = pushed_stream(
            format.rate,
            format.channels,
            capacity,
            self.underruns.clone(),
        );
        let entry = StreamEntry {
            serial,
            format,
            producer,
            source: Some(source),
        };
        let mut streams = lock(&self.streams);
        let others = streams
            .iter()
            .enumerate()
            .filter(|(i, e)| *i != slot && e.is_some())
            .count();
        if others >= MAX_STREAMS {
            return false;
        }
        let old = streams[slot].replace(entry);
        drop(streams);
        drop(old);
        true
    }

    /// `None` asks how many bytes would fill the ring to three quarters; `-1` means no such stream.
    fn stream_push(&self, slot: usize, serial: u32, bytes: Option<&[u8]>) -> i32 {
        let mut streams = lock(&self.streams);
        let Some(entry) = streams
            .get_mut(slot)
            .and_then(Option::as_mut)
            .filter(|e| e.serial == serial)
        else {
            return -1;
        };
        if entry.source.is_none() && entry.producer.is_abandoned() {
            return -1;
        }
        let fb = entry.format.frame_bytes();
        let accepted = match bytes {
            None => {
                let quarter = entry.producer.capacity_frames() / 4;
                entry.producer.free_frames().saturating_sub(quarter) * fb
            }
            Some(bytes) => {
                let format = entry.format;
                let frames = format.frames_in(bytes.len());
                entry.producer.push(frames, format.samples(bytes)) * fb
            }
        };
        i32::try_from(accepted).unwrap_or(i32::MAX)
    }

    fn stream_end(&self, slot: usize, serial: u32) {
        let mut streams = lock(&self.streams);
        if let Some(e) = streams.get_mut(slot)
            && e.as_ref().is_some_and(|e| e.serial == serial)
        {
            let ended = e.take();
            drop(streams);
            drop(ended);
        }
    }

    fn take_stream(&self, slot: usize, serial: u32) -> Taken<StreamSource> {
        try_with(&self.streams, |streams| {
            streams
                .get_mut(slot)?
                .as_mut()
                .filter(|e| e.serial == serial)?
                .source
                .take()
        })
    }

    fn buffer(&self, id: u32) -> Taken<Arc<Pcm>> {
        try_with(&self.buffers, |buffers| buffers.get(id))
    }

    fn volumes(&self) -> Option<(u32, [f32; SOURCES])> {
        let seq = self.control.volume_seq.load(Ordering::Acquire);
        if seq == 0 {
            return None;
        }
        let volumes = std::array::from_fn(|i| {
            let v = f32::from_bits(self.control.volumes[i].load(Ordering::Relaxed));
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        Some((seq, volumes))
    }

    fn stats(&self) -> SoundStats {
        let c = &self.counters;
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        SoundStats {
            uploads: get(&c.uploads),
            rejected_uploads: get(&c.rejected_uploads),
            buffered_bytes: lock(&self.buffers).bytes as u64,
            plays: get(&c.plays),
            missing_sources: get(&c.missing_sources),
            bad_commands: get(&c.bad_commands),
            underruns: self.underruns.load(Ordering::Relaxed),
            dropped: u64::from(self.control.dropped.load(Ordering::Relaxed)),
        }
    }
}

/// Rust's handle on Java's sound output; cheap to clone.
#[derive(Debug, Clone)]
pub struct SoundLink {
    shared: Arc<SoundShared>,
}

impl SoundLink {
    /// The process-wide link Java's bridge mod talks to.
    pub fn get() -> Result<Self, BridgeError> {
        Ok(Self {
            shared: crate::abi::shared()?.sound.clone(),
        })
    }

    /// The command source to hand to the mixer; `None` while another one is attached.
    pub fn voices(&self, out_rate: u32) -> Option<BridgeVoices> {
        BridgeVoices::new(self.shared.clone(), out_rate)
    }

    /// A mixer is polling Java's commands, so Java may create channels.
    pub fn attached(&self) -> bool {
        self.shared.control.attached.load(Ordering::Acquire) != 0
    }

    /// Java's sound sliders by `SoundSource` ordinal, with a sequence that changes on every write.
    pub fn java_volumes(&self) -> Option<(u32, [f32; SOURCES])> {
        self.shared.volumes()
    }

    pub fn stats(&self) -> SoundStats {
        self.shared.stats()
    }

    /// `(generation, state)` of a bridge voice slot.
    pub fn slot_state(&self, slot: usize) -> Option<(u32, u8)> {
        self.shared.states.load(slot)
    }

    /// Voices Java started since the last call, oldest first; up to 1024 wait unread.
    pub fn take_started(&self, out: &mut Vec<VoiceStart>) {
        if let Some(rx) = lock(&self.shared.started).as_mut() {
            while let Ok(v) = rx.pop() {
                out.push(v);
            }
        }
    }
}

/// # Safety
/// `ptr` must be null or point to `len` readable bytes for the rest of the current downcall.
unsafe fn java_bytes<'a>(ptr: i64, len: i32, max: usize) -> Option<&'a [u8]> {
    let len = usize::try_from(len).ok().filter(|l| *l <= max)?;
    if ptr == 0 || len == 0 {
        return None;
    }
    // SAFETY: guaranteed by the caller; the length is bounded above.
    Some(unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len) })
}

fn sound() -> Option<&'static SoundShared> {
    crate::abi::shared_now().map(|s| &*s.sound)
}

fn guarded<R>(fallback: R, f: impl FnOnce(&SoundShared) -> R) -> R {
    catch_unwind(AssertUnwindSafe(|| sound().map(f)))
        .ok()
        .flatten()
        .unwrap_or(fallback)
}

pub(crate) extern "C" fn sound_upload(
    ptr: i64,
    len: i32,
    rate: i32,
    channels: i32,
    format: i32,
) -> i32 {
    guarded(0, |s| {
        let Some(format) = PcmFormat::parse(rate, channels, format) else {
            return 0;
        };
        // SAFETY: Java passes a native segment that outlives this downcall; `len` is bounded.
        let Some(bytes) = (unsafe { java_bytes(ptr, len, MAX_UPLOAD_BYTES) }) else {
            s.counters.rejected_uploads.fetch_add(1, Ordering::Relaxed);
            return 0;
        };
        s.upload(bytes, format) as i32
    })
}

pub(crate) extern "C" fn sound_free(id: i32) {
    guarded((), |s| s.free(id as u32));
}

pub(crate) extern "C" fn sound_stream_open(
    slot: i32,
    serial: i32,
    rate: i32,
    channels: i32,
    format: i32,
) -> i32 {
    guarded(0, |s| {
        let (Ok(slot), Some(format)) = (
            usize::try_from(slot),
            PcmFormat::parse(rate, channels, format),
        ) else {
            return 0;
        };
        i32::from(s.stream_open(slot, serial as u32, format))
    })
}

/// `len` 0 asks for the wanted byte count, [`STREAM_END`] closes the stream.
pub(crate) extern "C" fn sound_stream_push(slot: i32, serial: i32, ptr: i64, len: i32) -> i32 {
    guarded(-1, |s| {
        let Ok(slot) = usize::try_from(slot) else {
            return -1;
        };
        let serial = serial as u32;
        if len == STREAM_END {
            s.stream_end(slot, serial);
            return 0;
        }
        if len == 0 {
            return s.stream_push(slot, serial, None);
        }
        // SAFETY: Java passes a native segment that outlives this downcall; `len` is bounded.
        match unsafe { java_bytes(ptr, len, MAX_UPLOAD_BYTES) } {
            Some(bytes) => s.stream_push(slot, serial, Some(bytes)),
            None => -1,
        }
    })
}

#[cfg(test)]
mod tests;
