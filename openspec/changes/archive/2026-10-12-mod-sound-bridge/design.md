## Context

The vanilla sound stack in 26.3:

- `net.minecraft.client.sounds.SoundManager` is a thin delegate to a private `SoundEngine`.
- `SoundEngine.play(SoundInstance)`:
  1. Resolves the sound.
  2. Computes the final gain (`calculateVolume`: instance × category × master) and pitch.
  3. Gets a `ChannelAccess.ChannelHandle` from the STATIC or STREAMING pool.
  4. Configures the `Channel` (pitch, volume, attenuation, loop, position, relative) on the "Sound engine"
     executor thread.
  5. Attaches a `SoundBuffer` (`SoundBufferLibrary.getCompleteBuffer`) or an `AudioStream`
     (`SoundBufferLibrary.getStream`, `JOrbisAudioStream`/`LoopingAudioStream`), then plays.
- `tick(boolean)` drives ticking sounds, delays and `Channel.updateStream`. `updateSource(Camera)` sets the
  `Listener` transform.
- `loadLibrary()` calls `Library.init(device, DeviceList, hrtf)`, which opens the OpenAL device and builds
  `CountingChannelPool`s. When it fails, `loaded` stays false and every call becomes a no-op.
- `Channel` (private constructor, `static create()` via `alGenSources`) has `play`, `pause`, `unpause`, `stop`,
  `playing`, `stopped`, `setSelfPosition`, `setPitch`, `setLooping`, `setVolume`, `disableAttenuation`,
  `linearAttenuation`, `setRelative`, `attachStaticBuffer`, `attachBufferStream`, `updateStream`, `destroy`.
- `SoundBuffer(ByteBuffer pcm, AudioFormat)` uploads lazily to an AL buffer (`releaseAlBuffer`, `discardAlBuffer`).
- Fabric sound-api mixes into `SoundEngine` (`getStream` hook) and `SoundInstance`. Nothing in Fabric API touches
  `Library`/`Channel`, so bridging below `SoundEngine` keeps every mod hook above it.

`mcr-audio` has an rtrb command queue (`AudioHandle::send`) with these commands:
- `Play(VoiceId, PlayRequest{source Static(Arc<Pcm>) | Stream, volume, pitch, category, spatial, looping})`
- `Stop`, `StopCategory`, `StopAll`, `SetListener`, `SetCategoryVolume`
- `SetVoicePosition`, `SetVoiceVolume`, `SetVoicePitch`

It uses the OpenAL linear distance model.

## Decisions

**Bridge at the OpenAL layer.**

| Target | Hook |
|---|---|
| `Library.init(String, DeviceList, boolean)` | cancel; set the private `staticChannels`/`streamingChannels` fields to `Library.CountingChannelPool`s through accessors, sized as `Library.init` would size them for a 255-source device |
| `Library.cleanup()`, `isCurrentDeviceDisconnected()`, `currentDeviceName()`, `getChannelDebugString()` | bridge versions |
| `Library.createDeviceTracker()` | a `DeviceTracker` reporting one device named `mcr` that never changes |
| `Channel.create()` | return a `Channel` built through an `@Invoker` of the private constructor with a bridge voice id (a negative int space, never an AL name) |
| every `Channel` method listed above | HEAD cancel → `BridgeAudio` command |
| `Listener.setTransform`/`reset` | ignored (mcr owns the listener) |
| `SoundBuffer.releaseAlBuffer`/`discardAlBuffer` | free the Rust PCM for that buffer |

`mod-fabric-boot`'s `SoundEngine.loadLibrary` cancel is removed, so `loaded` becomes true through the normal path.

**Commands and state.**
- `BridgeAudio` writes fixed-size commands into a dedicated SPSC ring. The Java "Sound engine" executor thread is
  its only producer, since vanilla runs all channel work there. Uploads are the exception: a mutex guards them, and
  they may come from the background executor.
- A Rust bridge thread does not exist. The ring is a second command producer that the `mcr-audio` mixer polls
  alongside mcr's own queue, so latency matches native sounds.
- Voice state (playing, stopped, paused) is a byte per bridge voice in a shared table that the mixer updates.
  `Channel.playing()`/`stopped()` read it directly from a `MemorySegment`, with no downcall.

**Static buffers.** On `attachStaticBuffer(SoundBuffer)`, Java looks up the buffer's Rust id in an identity
`WeakHashMap`. On a miss, it uploads `(ptr, len, AudioFormat sampleRate/channels/bits)` with one downcall. Rust
copies the data into an `Arc<Pcm>` (converted to its mixer format, resampled once) and returns the id. Cache
eviction follows `releaseAlBuffer` and the weak reference.

**Streams.** `attachBufferStream(AudioStream)` creates a pushed-PCM voice with a 1 s ring. `updateStream()` (on the
sound thread, called each sound tick) reads from the `AudioStream` until the ring is three-quarters full. That is
the same pull vanilla does into AL buffers. End of stream marks the voice as draining. Looping streams loop on the
Java side (`LoopingAudioStream`), as in vanilla.

**Gain and spatialisation.**
- Java computes the final gain (instance × category × master), so bridge voices use a `PreMixed` category that
  skips mcr's category scaling.
- `linearAttenuation(max)` maps to `Spatial::Positioned{max_distance}`, `disableAttenuation()` to `Global`, and
  `setRelative(true)` to listener-relative positioning.
- Pitch maps directly, with vanilla's clamp to 0.5–2.0 already applied by Java.

**Ownership in mod mode.**

| Java plays | mcr plays |
|---|---|
| clientbound sound packets (`ClientPacketListener.handleSoundEvent`/`handleSoundEntityEvent`), music (`MusicManager`), ambient handlers in `LocalPlayer`, GUI clicks, mod sounds, other entities' footsteps | local-player footsteps, land, swim and jump sounds; mcr-predicted block hit/break/place sounds |

mcr's sound-packet handler, music and ambience are skipped in mod mode. Both sides use mcr's listener (Rust camera).
Volumes come from Java's `Options` sound sliders. At each `awaitFrame`, Java writes
`Options.getFinalSoundSourceVolume(source)` for every `SoundSource` into a shared float table, and Rust applies those
values to mcr-owned categories.

## Risks

- `Channel`'s private constructor and final fields: `@Invoker` covers construction. If a mod also mixes into
  `Channel` (direct-AL mods), it is incompatible and listed in the mod manager.
- Stream underruns if the sound thread stalls (GC): the 1 s ring absorbs it.
- Double sounds if a server sound is also triggered locally by mcr's prediction (for example block break). The
  ownership table is checked against a captured session in task 3.2.

## Measurements

`cargo bench -p mcr-audio --bench mixer` (Ryzen 7 5800X): rendering 1024 frames (21.3 ms at 48 kHz) with 64
positioned mono voices at 22.05 kHz and varied pitch costs 158.6 µs with static buffers and 154.7 µs with pushed
rings, about 0.7 % of one core either way. Pushed voices cost the same as static ones, because both pull one frame
per resampling step. The ring pop replaces the buffer index.
