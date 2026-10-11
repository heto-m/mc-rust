## 1. mcr-audio
- [ ] 1.1 Pushed-PCM source with ring, drain-on-end and underrun counter; unit tests with the offline mixer
- [ ] 1.2 Pause/resume per voice, `PreMixed` category, second command producer, shared voice-state table; unit tests
- [ ] 1.3 Bench: mixer cost with 64 pushed voices versus static voices; record in design.md

## 2. Bridge
- [ ] 2.1 Rust: command ring consumer wiring, PCM upload with resampling, voice id mapping
- [ ] 2.2 Java mixins: `Library`, `DeviceTracker`, `Channel`, `Listener`, `SoundBuffer`; remove the `loadLibrary` cancel
- [ ] 2.3 Mod-mode ownership in mcr (skip packet sounds, music, ambience) and the Java options volume sync

## 3. Verification
- [ ] 3.1 Test mod with `sounds.json` and an OGG test tone; integration test with offline mixer capture as in the proposal
- [ ] 3.2 Captured session against `xtask server` (walk, break blocks, mobs, music): count voices per sound event and assert no duplicates
- [ ] 3.3 fmt, clippy, tests, archive
