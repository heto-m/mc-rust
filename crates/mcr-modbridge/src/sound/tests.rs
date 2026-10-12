use std::path::Path;

use mcr_audio::{Mixer, VOICE_PLAYING, VOICE_STOPPED};
use mcr_jvm::RingWriter;

use super::*;

const RATE: u32 = 48_000;
const S16LE: i32 = 16 | SOUND_FORMAT_SIGNED;

struct Java {
    link: SoundLink,
    ring: RingWriter,
}

impl Java {
    fn new() -> Self {
        let shared = Arc::new(SoundShared::new().unwrap());
        let ring = shared.ring.writer().unwrap();
        Self {
            link: SoundLink { shared },
            ring,
        }
    }

    fn shared(&self) -> &SoundShared {
        &self.link.shared
    }

    fn send(&mut self, kind: u16, slot: u32, arg: u32, v: [f32; 3]) {
        let mut p = [0u8; SND_SIZE];
        p[SND_SLOT..SND_SLOT + 4].copy_from_slice(&slot.to_ne_bytes());
        p[SND_ARG..SND_ARG + 4].copy_from_slice(&arg.to_ne_bytes());
        for (i, c) in v.iter().enumerate() {
            let o = SND_X + 4 * i;
            p[o..o + 4].copy_from_slice(&c.to_bits().to_ne_bytes());
        }
        self.ring.try_push(kind, &p).unwrap();
    }

    fn cmd(&mut self, kind: u16, slot: u32, arg: u32) {
        self.send(kind, slot, arg, [0.0; 3]);
    }

    fn value(&mut self, kind: u16, slot: u32, x: f32) {
        self.send(kind, slot, 0, [x, 0.0, 0.0]);
    }

    /// What `Channel.play()` does on the Java side.
    fn play(&mut self, slot: u32, generation: u32) {
        self.shared()
            .states
            .store(slot as usize, generation, VOICE_PLAYING);
        self.cmd(SND_PLAY, slot, generation);
    }

    fn global(&mut self, slot: u32) {
        self.cmd(SND_CREATE, slot, 0);
        self.value(SND_ATTENUATION, slot, 0.0);
        self.cmd(SND_RELATIVE, slot, 1);
    }
}

fn s16(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| ((s * 32767.0) as i16).to_le_bytes())
        .collect()
}

fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    mixer.render(&mut out);
    out
}

fn mixer(java: &Java) -> Mixer {
    let mut m = Mixer::new(RATE);
    m.attach_producer(Box::new(java.link.voices(RATE).unwrap()));
    m
}

#[test]
fn static_buffer_plays_once_and_its_slot_reads_stopped_after() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let id = java.shared().upload(&s16(&[0.5; 2400]), fmt);
    assert_ne!(id, 0);
    java.global(4);
    java.value(SND_VOLUME, 4, 0.5);
    java.cmd(SND_BUFFER, 4, id);
    java.play(4, 9);
    let out = render(&mut m, 1024);
    let expected = 0.5 * 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!((out[1000] - expected).abs() < 1e-3, "{}", out[1000]);
    assert_eq!(m.active_voices(), 1);
    assert_eq!(java.link.slot_state(4), Some((9, VOICE_PLAYING)));
    render(&mut m, 2048);
    assert_eq!(m.active_voices(), 0);
    assert_eq!(java.link.slot_state(4), Some((9, VOICE_STOPPED)));
    let mut started = Vec::new();
    java.link.take_started(&mut started);
    assert_eq!(
        started,
        [VoiceStart {
            slot: 4,
            generation: 9,
            buffer: Some(id),
            channels: 1,
            sample_rate: RATE,
        }]
    );
    assert_eq!(java.link.stats().plays, 1);
}

#[test]
fn uploads_resample_to_the_mixer_rate() {
    let java = Java::new();
    let _m = mixer(&java);
    let fmt = PcmFormat::parse(24_000, 2, S16LE).unwrap();
    let id = java.shared().upload(&s16(&[0.1; 200]), fmt);
    let pcm = lock(&java.shared().buffers).get(id).unwrap();
    assert_eq!(
        (pcm.sample_rate, pcm.channels, pcm.frames()),
        (RATE, 2, 200)
    );
}

#[test]
fn stream_plays_pushed_pcm_and_ends_after_draining() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let s = java.shared();
    assert!(s.stream_open(2, 7, fmt));
    let wanted = s.stream_push(2, 7, None);
    assert_eq!(wanted, (RATE as i32 * 3 / 4) * 2);
    assert_eq!(s.stream_push(2, 7, Some(&s16(&[0.25; 4800]))), 9600);
    assert_eq!(s.stream_push(2, 8, Some(&[0; 4])), -1);
    java.global(2);
    java.cmd(SND_STREAM, 2, 7);
    java.play(2, 1);
    let out = render(&mut m, 2048);
    assert!((out[2000] - 0.25 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    let s = java.shared();
    assert_eq!(s.stream_push(2, 7, Some(&s16(&[0.25; 4800]))), 9600);
    s.stream_end(2, 7);
    assert_eq!(s.stream_push(2, 7, None), -1);
    render(&mut m, 4800 * 2);
    assert_eq!(m.active_voices(), 0);
    assert_eq!(java.link.slot_state(2), Some((1, VOICE_STOPPED)));
    assert_eq!(java.link.stats().underruns, 0);
}

#[test]
fn pause_resume_and_restart_follow_generations() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let id = java.shared().upload(&s16(&[0.5; 48_000]), fmt);
    java.global(0);
    java.cmd(SND_BUFFER, 0, id);
    java.play(0, 1);
    render(&mut m, 256);
    java.shared().states.store(0, 1, mcr_audio::VOICE_PAUSED);
    java.cmd(SND_PAUSE, 0, 0);
    let out = render(&mut m, 512);
    assert!(out[512..].iter().all(|s| *s == 0.0));
    java.cmd(SND_RESUME, 0, 0);
    render(&mut m, 256);
    assert_eq!(java.link.slot_state(0), Some((1, VOICE_PLAYING)));
    java.play(0, 2);
    render(&mut m, 256);
    assert_eq!(m.active_voices(), 1);
    assert_eq!(java.link.slot_state(0), Some((2, VOICE_PLAYING)));
    java.shared().states.store(0, 2, VOICE_STOPPED);
    java.cmd(SND_STOP, 0, 0);
    render(&mut m, 256);
    assert_eq!(m.active_voices(), 0);
}

#[test]
fn looping_and_parameter_changes_reach_a_playing_voice() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let id = java.shared().upload(&s16(&[0.5; 100]), fmt);
    java.cmd(SND_CREATE, 1, 0);
    java.value(SND_ATTENUATION, 1, 16.0);
    java.send(SND_POSITION, 1, 0, [0.0, 0.0, 8.0]);
    java.cmd(SND_LOOPING, 1, 1);
    java.cmd(SND_BUFFER, 1, id);
    java.play(1, 1);
    let out = render(&mut m, 4096);
    let half = 0.5 * 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!((out[8000] - half).abs() < 1e-3, "{}", out[8000]);
    java.value(SND_VOLUME, 1, 0.0);
    let out = render(&mut m, 512);
    assert_eq!(out[1022], 0.0);
    assert_eq!(m.active_voices(), 1);
}

#[test]
fn missing_sources_and_hostile_commands_are_contained() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    java.cmd(SND_BUFFER, 5, 0xDEAD_BEEF);
    java.play(5, 3);
    java.cmd(SND_STREAM, 6, 1);
    java.play(6, 1);
    java.play(9999, 1);
    java.cmd(SND_PLAY, u32::MAX, 1);
    java.cmd(77, 0, 0);
    java.send(SND_POSITION, 0, 0, [f32::NAN, f32::INFINITY, -1e30]);
    java.value(SND_PITCH, 0, f32::NAN);
    java.value(SND_ATTENUATION, 0, f32::NEG_INFINITY);
    java.ring.try_push(SND_PLAY, &[1, 2, 3]).unwrap();
    render(&mut m, 256);
    assert_eq!(m.active_voices(), 0);
    assert_eq!(java.link.slot_state(5), Some((3, VOICE_STOPPED)));
    assert_eq!(java.link.slot_state(6), Some((1, VOICE_STOPPED)));
    let stats = java.link.stats();
    assert_eq!(stats.missing_sources, 2);
    assert_eq!(stats.bad_commands, 4);
}

#[test]
fn a_busy_table_defers_play_to_the_next_chunk() {
    let mut java = Java::new();
    let mut m = mixer(&java);
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let id = java.shared().upload(&s16(&[0.5; 4800]), fmt);
    java.global(0);
    java.cmd(SND_BUFFER, 0, id);
    java.play(0, 1);
    {
        let _held = lock(&java.shared().buffers);
        render(&mut m, 256);
        assert_eq!(m.active_voices(), 0);
    }
    render(&mut m, 256);
    assert_eq!(m.active_voices(), 1);
    assert_eq!(java.link.stats().missing_sources, 0);
}

#[test]
fn uploads_are_bounded() {
    let java = Java::new();
    let s = java.shared();
    let fmt = PcmFormat::parse(RATE as i32, 2, S16LE).unwrap();
    assert_eq!(s.upload(&[0; 3], fmt), 0);
    assert_eq!(s.upload(&vec![0; MAX_UPLOAD_BYTES + 4], fmt), 0);
    assert_eq!(java.link.stats().rejected_uploads, 2);
    assert_eq!(sound_upload(0, 100, 48_000, 1, S16LE), 0);
    assert_eq!(sound_stream_push(-1, 0, 0, 0), -1);
    assert_eq!(
        sound_stream_open(SOUND_SLOTS as i32, 0, 48_000, 1, S16LE),
        0
    );
}

#[test]
fn upload_cost_counts_the_resampled_copy_and_never_overflows() {
    let fmt = PcmFormat::parse(8_000, 2, S16LE).unwrap();
    assert_eq!(upload_cost(400, fmt, 0), Some(100 * 2 * 4));
    assert_eq!(upload_cost(400, fmt, 8_000), Some(100 * 2 * 4));
    assert_eq!(
        upload_cost(400, fmt, 48_000),
        Some(100 * 2 * 4 + 600 * 2 * 4)
    );
    assert_eq!(upload_cost(usize::MAX, fmt, u32::MAX), None);
}

#[test]
fn the_byte_budget_rejects_uploads_before_decoding_them() {
    let java = Java::new();
    let s = java.shared();
    lock(&s.buffers).budget = 4096;
    let fmt = PcmFormat::parse(RATE as i32, 1, S16LE).unwrap();
    let a = s.upload(&[0; 1024], fmt);
    assert_ne!(a, 0);
    assert_eq!(s.upload(&[0; 2048], fmt), 0);
    assert_eq!(java.link.stats().buffered_bytes, 2048);
    s.free(a);
    assert_ne!(s.upload(&[0; 2048], fmt), 0);
    s.out_rate.store(192_000, Ordering::Relaxed);
    let low = PcmFormat::parse(8_000, 1, 8).unwrap();
    assert_eq!(
        s.upload(&[0; 64], low),
        0,
        "resampling would pass the budget"
    );
    assert_eq!(java.link.stats().rejected_uploads, 2);
}

#[test]
fn open_streams_are_capped() {
    let java = Java::new();
    let s = java.shared();
    let fmt = PcmFormat::parse(8_000, 1, S16LE).unwrap();
    for slot in 0..MAX_STREAMS {
        assert!(s.stream_open(slot, 1, fmt));
    }
    assert!(!s.stream_open(MAX_STREAMS, 1, fmt));
    assert!(
        s.stream_open(0, 2, fmt),
        "reopening a slot replaces its stream"
    );
    s.stream_end(1, 1);
    assert!(s.stream_open(MAX_STREAMS, 1, fmt));
}

#[test]
fn buffer_ids_are_generational() {
    let mut t = BufferTable::default();
    let pcm = Arc::new(Pcm::mono(RATE, vec![0.0; 10]));
    let a = t.insert(pcm.clone()).unwrap();
    assert!(t.get(a).is_some());
    assert_eq!(t.bytes, 40);
    assert!(t.remove(a).is_some());
    assert_eq!(t.bytes, 0);
    let b = t.insert(pcm).unwrap();
    assert_eq!(a & 0xFFFF, b & 0xFFFF);
    assert_ne!(a, b);
    assert!(t.get(a).is_none());
    assert!(t.remove(a).is_none());
    assert!(t.get(u32::MAX).is_none());
}

#[test]
fn java_volumes_are_clamped_and_sequenced() {
    let java = Java::new();
    assert!(java.link.java_volumes().is_none());
    let c = &java.shared().control;
    c.volumes[0].store(0.5f32.to_bits(), Ordering::Relaxed);
    c.volumes[4].store(f32::NAN.to_bits(), Ordering::Relaxed);
    c.volumes[5].store(7.0f32.to_bits(), Ordering::Relaxed);
    c.volume_seq.store(1, Ordering::Release);
    let (seq, v) = java.link.java_volumes().unwrap();
    assert_eq!((seq, v[0], v[4], v[5]), (1, 0.5, 0.0, 1.0));
}

#[test]
fn attaching_is_exclusive_and_detaching_stops_java_voices() {
    let java = Java::new();
    let voices = java.link.voices(RATE).unwrap();
    assert!(java.link.attached());
    assert!(java.link.voices(RATE).is_none());
    java.shared().states.store(3, 2, VOICE_PLAYING);
    drop(voices);
    assert!(!java.link.attached());
    assert_eq!(java.link.slot_state(3), Some((2, VOICE_STOPPED)));
    assert!(java.link.voices(RATE).is_some());
}

fn sound_abi_java() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../java/mcr-bridge/src/dev/mcr/bridge/sound/SoundAbi.java");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn int_const(src: &str, name: &str) -> i64 {
    let decl = format!("static final int {name} = ");
    let start = src
        .find(&decl)
        .unwrap_or_else(|| panic!("{name} missing in SoundAbi.java"));
    let rest = &src[start + decl.len()..];
    let text = rest[..rest.find(';').unwrap()].trim();
    match text.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).unwrap(),
        None => text.parse().unwrap(),
    }
}

#[test]
fn java_constants_match_rust() {
    let src = sound_abi_java();
    let ints: &[(&str, i64)] = &[
        ("SND_SLOT", SND_SLOT as i64),
        ("SND_ARG", SND_ARG as i64),
        ("SND_X", SND_X as i64),
        ("SND_Y", SND_Y as i64),
        ("SND_Z", SND_Z as i64),
        ("SND_SIZE", SND_SIZE as i64),
        ("SND_CREATE", SND_CREATE.into()),
        ("SND_DESTROY", SND_DESTROY.into()),
        ("SND_PLAY", SND_PLAY.into()),
        ("SND_PAUSE", SND_PAUSE.into()),
        ("SND_RESUME", SND_RESUME.into()),
        ("SND_STOP", SND_STOP.into()),
        ("SND_POSITION", SND_POSITION.into()),
        ("SND_PITCH", SND_PITCH.into()),
        ("SND_VOLUME", SND_VOLUME.into()),
        ("SND_LOOPING", SND_LOOPING.into()),
        ("SND_ATTENUATION", SND_ATTENUATION.into()),
        ("SND_RELATIVE", SND_RELATIVE.into()),
        ("SND_BUFFER", SND_BUFFER.into()),
        ("SND_STREAM", SND_STREAM.into()),
        ("CTL_ATTACHED", CTL_ATTACHED as i64),
        ("CTL_VOLUME_SEQ", CTL_VOLUME_SEQ as i64),
        ("CTL_VOLUMES", CTL_VOLUMES as i64),
        ("CTL_DROPPED", CTL_DROPPED as i64),
        ("CTL_SIZE", CTL_SIZE as i64),
        ("SOURCES", SOURCES as i64),
        ("SOUND_SLOTS", SOUND_SLOTS as i64),
        ("STREAM_END", STREAM_END.into()),
        ("FORMAT_BITS", SOUND_FORMAT_BITS.into()),
        ("FORMAT_SIGNED", SOUND_FORMAT_SIGNED.into()),
        ("FORMAT_BIG_ENDIAN", SOUND_FORMAT_BIG_ENDIAN.into()),
        ("STATE_INITIAL", mcr_audio::VOICE_INITIAL.into()),
        ("STATE_PLAYING", VOICE_PLAYING.into()),
        ("STATE_PAUSED", mcr_audio::VOICE_PAUSED.into()),
        ("STATE_STOPPED", VOICE_STOPPED.into()),
        ("GENERATION_MASK", mcr_audio::GENERATION_MASK.into()),
    ];
    for (name, value) in ints {
        assert_eq!(int_const(&src, name), *value, "{name}");
    }
}
