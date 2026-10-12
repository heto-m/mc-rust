# audio-mixer Specification

## Purpose
Real-time stereo mixer for positioned and global sounds: attenuation, panning, pitch, category volumes, voice limits, driven by a lock-free command queue.

## Requirements

### Requirement: Linear distance attenuation
Positioned voices SHALL use gain `clamp(1 - d / max, 0, 1)` where `max` is the voice's attenuation distance, and are silent at or beyond it.

#### Scenario: Curve
- **WHEN** a voice with max distance 16 is 8 blocks from the listener
- **THEN** its distance gain is 0.5, and 0 at 16 blocks

### Requirement: Equal-power panning
Positioned mono voices SHALL be panned by listener-relative azimuth with equal-power gains.

#### Scenario: Side and centre
- **WHEN** a source is directly to the listener's right
- **THEN** the left channel is silent and the right carries full gain; straight ahead gives equal channels at 1/sqrt(2)

### Requirement: Pitch by resampling
A voice with pitch `p` SHALL consume source frames at rate `p`.

#### Scenario: Double pitch
- **WHEN** pitch is 2.0
- **THEN** a one-second sound ends after half a second of output

### Requirement: Category volumes
The mixer SHALL scale each voice by its master and `SoundSource` category volume (master, music, record, weather, block, hostile, neutral, player, ambient, voice, ui) set at runtime.

#### Scenario: Muted category
- **WHEN** a category volume is 0
- **THEN** its voices render silence

### Requirement: Voice limit and stealing
The mixer SHALL support 247 concurrent voices and, when full, replace the quietest voice if the new voice is louder, otherwise discard the new voice.

#### Scenario: Full mixer
- **WHEN** 248 voices are started
- **THEN** at most 247 are active

### Requirement: Real-time safe command queue
The game thread SHALL control the mixer only through a lock-free queue drained by the audio callback, and the mixer SHALL be renderable offline into a buffer.

#### Scenario: Offline render
- **WHEN** a sound is played and N frames rendered without a device
- **THEN** the buffer contains the mixed output

### Requirement: Voice end reports
The mixer SHALL report voices that finish playing by themselves to the game thread, and SHALL NOT report voices removed by a stop command.

#### Scenario: Finished and stopped
- **WHEN** one voice runs out and another is stopped
- **THEN** only the first is reported

### Requirement: Pushed PCM sources
The mixer SHALL support voices fed by a producer-filled PCM ring with a declared sample rate and channel count, SHALL resample them to the output rate, and SHALL count underruns instead of blocking.

#### Scenario: Underrun
- **WHEN** a pushed voice's ring runs empty while the voice is playing
- **THEN** the mixer outputs silence for that voice, increments its underrun counter and resumes when data arrives

### Requirement: Voice pause and pre-mixed gain
The mixer SHALL support pausing and resuming individual voices, and a pre-mixed category whose voices are not scaled by any category volume.

#### Scenario: Pause keeps position
- **WHEN** a voice is paused for one second and resumed
- **THEN** playback continues from the sample where it paused

### Requirement: Additional command producer and voice state
The mixer SHALL accept commands from a second independent producer queue and SHALL publish per-voice state (playing, paused, stopped) in shared memory readable without locking.

#### Scenario: Two producers
- **WHEN** both producers start voices in the same audio callback period
- **THEN** both voices start in that period
