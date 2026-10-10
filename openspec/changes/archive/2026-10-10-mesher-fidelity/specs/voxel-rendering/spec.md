## MODIFIED Requirements

### Requirement: Packed quad encoding
The renderer SHALL encode cube-face quads in 16 bytes and non-cube model quads in 40 bytes, both carrying four
corner light pairs and ambient occlusion, and the shader decoding SHALL yield exactly the fields the Rust
encoder was given.

#### Scenario: Encoder and shader agree
- **WHEN** quads with boundary field values are encoded and decoded on the GPU
- **THEN** every decoded field equals the encoded value

#### Scenario: Out-of-range field rejected
- **WHEN** a field exceeds its bit width
- **THEN** encoding fails instead of corrupting neighbouring fields

## ADDED Requirements

### Requirement: Dynamic lighting environment
Final surface brightness SHALL be computed on the GPU from per-vertex sky and block light through a lightmap
built from an environment (sky factor, sky light colour, ambient, block tint, brightness), so changing the
environment relights existing geometry without remeshing, and distance fog and the clear colour SHALL come
from the same environment.

#### Scenario: Night relight
- **WHEN** the sky factor drops and the same meshes are drawn
- **THEN** sky-lit surfaces become darker while block-lit surfaces keep their brightness

#### Scenario: Fog
- **WHEN** a surface lies beyond the fog end distance
- **THEN** it is drawn in the fog colour

### Requirement: Animated textures
Animated sprites SHALL advance on the GPU from a per-sprite frame table updated each tick, honouring frame
times and interpolation, without touching section geometry.

#### Scenario: Frame advance
- **WHEN** the animation clock advances past a frame's duration
- **THEN** quads using that sprite sample the next frame without re-uploading geometry

#### Scenario: Interpolation
- **WHEN** a sprite is interpolated and the clock is between frames
- **THEN** the sampled colour lies between the two frames
