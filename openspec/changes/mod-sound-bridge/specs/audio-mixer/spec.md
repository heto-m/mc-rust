## ADDED Requirements

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
