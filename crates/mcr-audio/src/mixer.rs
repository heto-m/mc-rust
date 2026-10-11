use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::category::SoundSource;
use crate::decode::{Pcm, Pull, StreamSource};

pub const MAX_VOICES: usize = 247;
pub const CHUNK_FRAMES: usize = 256;
const QUEUE_CAPACITY: usize = 1024;
const ENDED_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: [f32; 3],
    pub yaw_degrees: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            yaw_degrees: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spatial {
    Global,
    Positioned {
        position: [f32; 3],
        max_distance: f32,
    },
}

#[derive(Debug)]
pub enum Source {
    Static(Arc<Pcm>),
    Stream(StreamSource),
}

#[derive(Debug)]
pub struct PlayRequest {
    pub source: Source,
    pub volume: f32,
    pub pitch: f32,
    pub category: SoundSource,
    pub spatial: Spatial,
    pub looping: bool,
}

#[derive(Debug)]
pub enum Command {
    Play(VoiceId, PlayRequest),
    Stop(VoiceId),
    StopCategory(SoundSource),
    StopAll,
    SetListener(Listener),
    SetCategoryVolume(SoundSource, f32),
    SetVoicePosition(VoiceId, [f32; 3]),
    SetVoiceVolume(VoiceId, f32),
    SetVoicePitch(VoiceId, f32),
}

/// OpenAL linear distance model with reference distance 0 and rolloff 1.
pub fn linear_attenuation(distance: f32, max_distance: f32) -> f32 {
    if max_distance <= 0.0 {
        return 0.0;
    }
    (1.0 - distance / max_distance).clamp(0.0, 1.0)
}

/// Equal-power left/right gains for `pan` in -1 (left) ..= 1 (right).
pub fn pan_gains(pan: f32) -> [f32; 2] {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    [angle.cos(), angle.sin()]
}

impl Listener {
    pub fn distance_and_pan(&self, position: [f32; 3]) -> (f32, f32) {
        let d = [
            position[0] - self.position[0],
            position[1] - self.position[1],
            position[2] - self.position[2],
        ];
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if dist < 1e-4 {
            return (dist, 0.0);
        }
        let (sin, cos) = self.yaw_degrees.to_radians().sin_cos();
        let right = [-cos, -sin];
        let pan = (d[0] * right[0] + d[2] * right[1]) / dist;
        (dist, pan)
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    active: AtomicUsize,
}

impl Stats {
    pub fn active_voices(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct AudioHandle {
    tx: rtrb::Producer<Command>,
    next_id: u32,
    stats: Arc<Stats>,
    ended: rtrb::Consumer<VoiceId>,
}

impl AudioHandle {
    /// Drains voices that finished on their own since the last call.
    pub fn take_ended(&mut self, out: &mut Vec<VoiceId>) {
        while let Ok(id) = self.ended.pop() {
            out.push(id);
        }
    }

    pub fn send(&mut self, command: Command) -> bool {
        self.tx.push(command).is_ok()
    }

    pub fn play(&mut self, request: PlayRequest) -> Option<VoiceId> {
        let id = VoiceId(self.next_id);
        if self.send(Command::Play(id, request)) {
            self.next_id = self.next_id.wrapping_add(1);
            Some(id)
        } else {
            None
        }
    }

    pub fn stop(&mut self, id: VoiceId) -> bool {
        self.send(Command::Stop(id))
    }

    pub fn set_listener(&mut self, listener: Listener) -> bool {
        self.send(Command::SetListener(listener))
    }

    pub fn set_category_volume(&mut self, category: SoundSource, volume: f32) -> bool {
        self.send(Command::SetCategoryVolume(category, volume))
    }

    pub fn active_voices(&self) -> usize {
        self.stats.active_voices()
    }
}

#[derive(Debug)]
struct Voice {
    id: VoiceId,
    source: Source,
    volume: f32,
    pitch: f32,
    category: SoundSource,
    spatial: Spatial,
    looping: bool,
    cursor: usize,
    prev: [f32; 2],
    next: [f32; 2],
    frac: f64,
    rate_ratio: f64,
    exhausted: bool,
    finished: bool,
    gains: [f32; 2],
    loudness: f32,
}

impl Voice {
    fn new(id: VoiceId, req: PlayRequest, out_rate: u32) -> Self {
        let rate = match &req.source {
            Source::Static(pcm) => pcm.sample_rate,
            Source::Stream(s) => s.sample_rate,
        };
        let mut voice = Self {
            id,
            source: req.source,
            volume: req.volume.clamp(0.0, 1.0),
            pitch: req.pitch.clamp(0.5, 2.0),
            category: req.category,
            spatial: req.spatial,
            looping: req.looping,
            cursor: 0,
            prev: [0.0; 2],
            next: [0.0; 2],
            frac: 0.0,
            rate_ratio: f64::from(rate) / f64::from(out_rate),
            exhausted: false,
            finished: false,
            gains: [0.0; 2],
            loudness: 0.0,
        };
        voice.advance_source();
        voice.advance_source();
        voice
    }

    fn pull(&mut self) -> Pull {
        match &mut self.source {
            Source::Static(pcm) => {
                let ch = pcm.channels as usize;
                let frames = pcm.frames();
                if self.cursor >= frames {
                    if !self.looping || frames == 0 {
                        return Pull::End;
                    }
                    self.cursor = 0;
                }
                let i = self.cursor * ch;
                self.cursor += 1;
                let l = pcm.samples[i];
                let r = if ch == 2 { pcm.samples[i + 1] } else { l };
                Pull::Frame([l, r])
            }
            Source::Stream(s) => s.pull_frame(),
        }
    }

    fn advance_source(&mut self) {
        self.prev = self.next;
        if self.exhausted {
            self.finished = true;
            return;
        }
        match self.pull() {
            Pull::Frame(f) => self.next = f,
            Pull::Underrun => self.next = [0.0; 2],
            Pull::End => {
                self.exhausted = true;
                self.next = [0.0; 2];
            }
        }
    }

    fn is_stereo(&self) -> bool {
        match &self.source {
            Source::Static(pcm) => pcm.channels == 2,
            Source::Stream(s) => s.channels == 2,
        }
    }

    fn target_gains(&self, listener: &Listener, volumes: &[f32; SoundSource::COUNT]) -> [f32; 2] {
        let mut gain = self.volume * volumes[0];
        if self.category != SoundSource::Master {
            gain *= volumes[self.category.index()];
        }
        let stereo = self.is_stereo();
        match self.spatial {
            Spatial::Global => [gain, gain],
            Spatial::Positioned {
                position,
                max_distance,
            } => {
                let (dist, pan) = listener.distance_and_pan(position);
                let gain = gain * linear_attenuation(dist, max_distance);
                if stereo {
                    [gain, gain]
                } else {
                    let p = pan_gains(pan);
                    [gain * p[0], gain * p[1]]
                }
            }
        }
    }

    fn mix(&mut self, out: &mut [f32], target: [f32; 2]) {
        let frames = out.len() / 2;
        let inv = 1.0 / frames as f32;
        let step = f64::from(self.pitch) * self.rate_ratio;
        let mut gl = self.gains[0];
        let mut gr = self.gains[1];
        let dl = (target[0] - gl) * inv;
        let dr = (target[1] - gr) * inv;
        for frame in out.as_chunks_mut::<2>().0 {
            let t = self.frac as f32;
            let l = self.prev[0] + (self.next[0] - self.prev[0]) * t;
            let r = self.prev[1] + (self.next[1] - self.prev[1]) * t;
            frame[0] += l * gl;
            frame[1] += r * gr;
            gl += dl;
            gr += dr;
            self.frac += step;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                self.advance_source();
                if self.finished {
                    break;
                }
            }
            if self.finished {
                break;
            }
        }
        self.gains = target;
    }
}

#[derive(Debug)]
pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Voice>,
    listener: Listener,
    volumes: [f32; SoundSource::COUNT],
    rx: Option<rtrb::Consumer<Command>>,
    ended: Option<rtrb::Producer<VoiceId>>,
    stats: Arc<Stats>,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            voices: Vec::with_capacity(MAX_VOICES),
            listener: Listener::default(),
            volumes: [1.0; SoundSource::COUNT],
            rx: None,
            ended: None,
            stats: Arc::default(),
        }
    }

    pub fn with_queue(sample_rate: u32) -> (Self, AudioHandle) {
        let (tx, rx) = rtrb::RingBuffer::new(QUEUE_CAPACITY);
        let mut mixer = Self::new(sample_rate);
        let (ended_tx, ended_rx) = rtrb::RingBuffer::new(ENDED_CAPACITY);
        mixer.rx = Some(rx);
        mixer.ended = Some(ended_tx);
        let handle = AudioHandle {
            tx,
            next_id: 0,
            stats: mixer.stats.clone(),
            ended: ended_rx,
        };
        (mixer, handle)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    pub fn apply(&mut self, command: Command) {
        match command {
            Command::Play(id, req) => self.start(id, req),
            Command::Stop(id) => self.voices.retain(|v| v.id != id),
            Command::StopCategory(c) => self.voices.retain(|v| v.category != c),
            Command::StopAll => self.voices.clear(),
            Command::SetListener(l) => self.listener = l,
            Command::SetCategoryVolume(c, v) => self.volumes[c.index()] = v.clamp(0.0, 1.0),
            Command::SetVoicePosition(id, p) => {
                if let Some(Spatial::Positioned { position, .. }) =
                    self.voice_mut(id).map(|v| &mut v.spatial)
                {
                    *position = p;
                }
            }
            Command::SetVoiceVolume(id, vol) => {
                if let Some(v) = self.voice_mut(id) {
                    v.volume = vol.clamp(0.0, 1.0);
                }
            }
            Command::SetVoicePitch(id, p) => {
                if let Some(v) = self.voice_mut(id) {
                    v.pitch = p.clamp(0.5, 2.0);
                }
            }
        }
    }

    fn voice_mut(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| v.id == id)
    }

    fn start(&mut self, id: VoiceId, req: PlayRequest) {
        let mut voice = Voice::new(id, req, self.sample_rate);
        if voice.finished {
            return;
        }
        if let Spatial::Positioned {
            position,
            max_distance,
        } = voice.spatial
            && self.listener.distance_and_pan(position).0 >= max_distance
        {
            return;
        }
        voice.gains = voice.target_gains(&self.listener, &self.volumes);
        voice.loudness = voice.gains[0].max(voice.gains[1]);
        if self.voices.len() < MAX_VOICES {
            self.voices.push(voice);
            return;
        }
        let quietest = self
            .voices
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.loudness.total_cmp(&b.1.loudness))
            .map(|(i, v)| (i, v.loudness));
        if let Some((i, loudness)) = quietest
            && voice.loudness > loudness
        {
            self.voices[i] = voice;
        }
    }

    fn drain_queue(&mut self) {
        while let Some(command) = self.rx.as_mut().and_then(|rx| rx.pop().ok()) {
            self.apply(command);
        }
    }

    /// Fills `out` with interleaved stereo; commands are drained every `CHUNK_FRAMES`.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for chunk in out.chunks_mut(CHUNK_FRAMES * 2) {
            self.drain_queue();
            self.mix_chunk(chunk);
        }
        self.stats
            .active
            .store(self.voices.len(), Ordering::Relaxed);
    }

    fn mix_chunk(&mut self, chunk: &mut [f32]) {
        let listener = self.listener;
        let volumes = self.volumes;
        for voice in &mut self.voices {
            let target = voice.target_gains(&listener, &volumes);
            voice.mix(chunk, target);
            voice.loudness = target[0].max(target[1]);
        }
        let ended = &mut self.ended;
        self.voices.retain(|v| {
            if v.finished {
                if let Some(tx) = ended {
                    let _ = tx.push(v.id);
                }
                return false;
            }
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn constant(frames: usize, level: f32) -> Arc<Pcm> {
        Arc::new(Pcm::mono(RATE, vec![level; frames]))
    }

    fn request(pcm: &Arc<Pcm>, spatial: Spatial) -> PlayRequest {
        PlayRequest {
            source: Source::Static(pcm.clone()),
            volume: 1.0,
            pitch: 1.0,
            category: SoundSource::Blocks,
            spatial,
            looping: false,
        }
    }

    fn at(x: f32, z: f32, max: f32) -> Spatial {
        Spatial::Positioned {
            position: [x, 0.0, z],
            max_distance: max,
        }
    }

    fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0; frames * 2];
        mixer.render(&mut buf);
        buf
    }

    fn last_frame(buf: &[f32]) -> (f32, f32) {
        (buf[buf.len() - 2], buf[buf.len() - 1])
    }

    #[test]
    fn attenuation_is_linear_and_clamped() {
        assert_eq!(linear_attenuation(0.0, 16.0), 1.0);
        assert!((linear_attenuation(8.0, 16.0) - 0.5).abs() < 1e-6);
        assert_eq!(linear_attenuation(16.0, 16.0), 0.0);
        assert_eq!(linear_attenuation(40.0, 16.0), 0.0);
    }

    #[test]
    fn attenuated_voice_renders_half_gain_at_half_range() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize, 1.0);
        m.apply(Command::Play(VoiceId(0), request(&pcm, at(0.0, 8.0, 16.0))));
        let (l, r) = last_frame(&render(&mut m, 1024));
        let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (l - expected).abs() < 1e-3 && (r - expected).abs() < 1e-3,
            "{l} {r}"
        );
    }

    #[test]
    fn panning_follows_listener_yaw() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize, 1.0);
        // yaw 0 faces +Z so the right-hand side is -X
        m.apply(Command::Play(
            VoiceId(0),
            request(&pcm, at(-1.0, 0.0, 16.0)),
        ));
        let (l, r) = last_frame(&render(&mut m, 1024));
        assert!(l < 1e-3 && r > 0.9, "{l} {r}");

        m.apply(Command::StopAll);
        m.apply(Command::SetListener(Listener {
            position: [0.0; 3],
            yaw_degrees: 180.0,
        }));
        m.apply(Command::Play(
            VoiceId(1),
            request(&pcm, at(-1.0, 0.0, 16.0)),
        ));
        let (l, r) = last_frame(&render(&mut m, 1024));
        assert!(r < 1e-3 && l > 0.9, "{l} {r}");
    }

    #[test]
    fn centre_pan_is_equal_power() {
        let g = pan_gains(0.0);
        assert!((g[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((g[0] * g[0] + g[1] * g[1] - 1.0).abs() < 1e-6);
        let hard = pan_gains(1.0);
        assert!(hard[0].abs() < 1e-6 && (hard[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn double_pitch_ends_in_half_the_time() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize, 0.5);
        let mut req = request(&pcm, Spatial::Global);
        req.pitch = 2.0;
        m.apply(Command::Play(VoiceId(0), req));
        render(&mut m, RATE as usize / 2 - 512);
        assert_eq!(m.active_voices(), 1);
        render(&mut m, 1024);
        assert_eq!(m.active_voices(), 0);
    }

    #[test]
    fn pitch_resamples_a_ramp() {
        let ramp: Vec<f32> = (0..4800).map(|i| i as f32 / 4800.0).collect();
        let pcm = Arc::new(Pcm::mono(RATE, ramp));
        let mut m = Mixer::new(RATE);
        let mut req = request(&pcm, Spatial::Global);
        req.pitch = 2.0;
        m.apply(Command::Play(VoiceId(0), req));
        let buf = render(&mut m, 100);
        let slope = (buf[2 * 60] - buf[2 * 40]) / 20.0;
        assert!((slope - 2.0 / 4800.0).abs() < 1e-5, "{slope}");
    }

    #[test]
    fn category_and_master_volumes_scale() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize, 1.0);
        m.apply(Command::SetCategoryVolume(SoundSource::Blocks, 0.5));
        m.apply(Command::SetCategoryVolume(SoundSource::Master, 0.5));
        m.apply(Command::Play(VoiceId(0), request(&pcm, Spatial::Global)));
        let (l, _) = last_frame(&render(&mut m, 1024));
        assert!((l - 0.25).abs() < 1e-3, "{l}");

        m.apply(Command::SetCategoryVolume(SoundSource::Blocks, 0.0));
        render(&mut m, 512);
        let (l, r) = last_frame(&render(&mut m, 512));
        assert_eq!((l, r), (0.0, 0.0));
    }

    #[test]
    fn full_mixer_caps_voices_and_steals_the_quietest() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize, 0.1);
        for i in 0..MAX_VOICES as u32 + 10 {
            let mut req = request(&pcm, Spatial::Global);
            req.volume = 0.5;
            m.apply(Command::Play(VoiceId(i), req));
        }
        assert_eq!(m.active_voices(), MAX_VOICES);

        let mut quiet = request(&pcm, Spatial::Global);
        quiet.volume = 0.1;
        m.apply(Command::Play(VoiceId(9000), quiet));
        assert!(m.voices.iter().all(|v| v.id != VoiceId(9000)));

        let mut loud = request(&pcm, Spatial::Global);
        loud.volume = 1.0;
        m.apply(Command::Play(VoiceId(9001), loud));
        assert_eq!(m.active_voices(), MAX_VOICES);
        assert!(m.voices.iter().any(|v| v.id == VoiceId(9001)));
    }

    #[test]
    fn out_of_range_voice_is_not_started() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(1000, 1.0);
        m.apply(Command::Play(
            VoiceId(0),
            request(&pcm, at(20.0, 0.0, 16.0)),
        ));
        assert_eq!(m.active_voices(), 0);
    }

    #[test]
    fn queue_commands_take_effect_during_render() {
        let (mut m, mut handle) = Mixer::with_queue(RATE);
        let pcm = constant(RATE as usize, 1.0);
        let id = handle.play(request(&pcm, Spatial::Global)).unwrap();
        render(&mut m, 512);
        assert_eq!(handle.active_voices(), 1);
        handle.stop(id);
        render(&mut m, 512);
        assert_eq!(handle.active_voices(), 0);
    }

    #[test]
    fn stereo_source_is_not_panned() {
        let pcm = Arc::new(Pcm {
            channels: 2,
            sample_rate: RATE,
            samples: [1.0, 0.25].repeat(RATE as usize).into(),
        });
        let mut m = Mixer::new(RATE);
        m.apply(Command::Play(
            VoiceId(0),
            request(&pcm, at(-5.0, 0.0, 10.0)),
        ));
        let (l, r) = last_frame(&render(&mut m, 1024));
        assert!(
            (l - 0.5).abs() < 1e-3 && (r - 0.125).abs() < 1e-3,
            "{l} {r}"
        );
    }

    #[test]
    fn looping_voice_keeps_playing() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(100, 1.0);
        let mut req = request(&pcm, Spatial::Global);
        req.looping = true;
        m.apply(Command::Play(VoiceId(0), req));
        render(&mut m, 10_000);
        assert_eq!(m.active_voices(), 1);
    }

    #[test]
    fn many_voices_render_in_bounded_time() {
        let mut m = Mixer::new(RATE);
        let pcm = constant(RATE as usize * 4, 0.01);
        for i in 0..MAX_VOICES as u32 {
            m.apply(Command::Play(
                VoiceId(i),
                request(&pcm, at(i as f32 * 0.05, 1.0, 64.0)),
            ));
        }
        let start = std::time::Instant::now();
        render(&mut m, RATE as usize / 10);
        let per_second = start.elapsed().as_secs_f64() * 10.0;
        eprintln!("247 voices: {per_second:.3} s CPU per second of audio");
        assert!(per_second < 1.0);
    }

    #[test]
    fn finished_voices_are_reported_but_stopped_ones_are_not() {
        let (mut mixer, mut handle) = Mixer::with_queue(RATE);
        let short = constant(300, 0.5);
        let a = handle.play(request(&short, Spatial::Global)).unwrap();
        let long = constant(RATE as usize, 0.5);
        let b = handle.play(request(&long, Spatial::Global)).unwrap();
        render(&mut mixer, 1024);
        let mut ended = Vec::new();
        handle.take_ended(&mut ended);
        assert_eq!(ended, [a]);
        handle.stop(b);
        render(&mut mixer, 256);
        handle.take_ended(&mut ended);
        assert_eq!(ended, [a]);
    }
}
