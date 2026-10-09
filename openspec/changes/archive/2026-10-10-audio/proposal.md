# Proposal

## Why

The client has no sound. Vanilla sound files live in the launcher asset store, not the client jar, and
need a low-latency mixer plus the `sounds.json` event model to play them.

## What Changes

- `cargo xtask assets [--version]` downloads the asset index and all objects into `.cache/assets/{indexes,objects}` (launcher layout), in parallel, sha1 verified, skipping existing files.
- New crate `mcr-audio`: cpal output, OGG Vorbis decoding (short sounds fully decoded and cached, long ones streamed), a callback-thread mixer fed by a lock-free command queue, 3D linear attenuation, equal-power panning, pitch by resampling, per-category volumes, 247 voices.
- `sounds.json` parsing with weighted variants and event references; `SoundManager::play` picks variants with a seeded RNG.
- Offline mixer rendering for tests and an `examples/play.rs` for the real device.

Out of scope: HRTF, Doppler, reverb/EQ, subtitles, music scheduling logic, game wiring, resource-pack sound overrides, `.minecraft/assets` auto-detection beyond pointing the root path at it.

## Capabilities

### New Capabilities
- `asset-store-download`: xtask download of the asset index and objects.
- `audio-mixer`: voice mixing, attenuation, panning, pitch, categories, stealing, device output.
- `sound-events`: `sounds.json` model and seeded variant selection.

### Modified Capabilities

## Impact

New crate `crates/mcr-audio` (deps: cpal, lewton, rtrb, serde). `xtask` gains `assets.rs` and a command variant. Needs ALSA headers on Linux.
