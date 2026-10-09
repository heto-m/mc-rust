# Design

## Context

See proposal.md. The game thread must never block on audio; the device callback must never allocate or lock.

## Goals / Non-Goals

**Goals:** sub-20 ms output latency, 247 concurrent voices, deterministic offline rendering, vanilla-compatible sound selection.

**Non-Goals:** HRTF, Doppler, effects, game integration.

## Decisions

- **cpal + lewton**: lewton is pure Rust and adequate; short effects are decoded once to mono/stereo `Arc<[f32]>` and cached, music is decoded incrementally on a worker thread into an SPSC ring (rtrb) consumed by the voice.
- **Mixer** (`Mixer`) is plain data with `render(&mut [f32])` (interleaved stereo); the cpal callback owns it and drains a lock-free `rtrb` command queue each callback. Tests drive it directly.
- **Attenuation**: OpenAL linear distance model with reference distance 0 as vanilla: `gain = 1 - rolloff*(d - ref)/(max - ref)` clamped, ref=0, max=attenuation distance, rolloff 1. Sources beyond max are silent and culled.
- **Panning**: listener-relative azimuth from yaw; equal-power `cos/sin`; mono sources panned, stereo sources not spatialised (as OpenAL does for stereo buffers); non-positioned (UI/music) voices bypass 3D.
- **Pitch**: linear-interpolation resampling with fractional read cursor; pitch clamped 0.5..2.
- **Voice stealing**: when 247 are in use, steal the quietest effective-gain voice if lower than the new one, otherwise drop the new sound. Vanilla instead refuses new sounds; stealing is the stated requirement.
- **Handles**: ids allocated on the game thread (atomic counter), so `play` returns a handle immediately for later `stop`.
- **sounds.json**: serde model; entries are sound files or `type: event` references; weighted pick by cumulative weight using a seeded xorshift RNG; sound names resolve to `minecraft/sounds/<name>.ogg` via the asset index.
- **Asset store**: `AssetStore` reads `indexes/<id>.json` and `objects/<hh>/<hash>`; the root may be a real `.minecraft/assets`.
- **xtask assets**: ureq with a small worker pool (std threads over a shared queue), sha1 check, `.part` rename.

## Risks / Trade-offs

- lewton is slower than native decoders, mitigated by decode caching and streaming on a worker.
- Streaming underruns produce silence for that voice rather than glitching others.
