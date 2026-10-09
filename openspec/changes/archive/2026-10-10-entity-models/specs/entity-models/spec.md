# Spec Delta

## Purpose
Provide vanilla-accurate entity model geometry, poses, animations and instanced GPU rendering, built from data extracted from the user's own client.

## ADDED Requirements

### Requirement: Model extraction
The extractor SHALL dump every model layer location of the client with its part tree (pose, cubes with origin, size, UV offset, inflate, mirror, texture size), every keyframe animation definition, and per entity type the statically derivable renderer textures, layers, shadow radius and scale, as JSON under the version's extracted directory.

#### Scenario: Pig layer
- **WHEN** the dump is loaded
- **THEN** layer `pig#main` has a head, a body and four legs

### Requirement: Cube geometry
The system SHALL turn each cube into six quads with vanilla UV layout, honoring inflate and mirror.

#### Scenario: Known cube
- **WHEN** a cube of size 8x8x8 with UV offset (0,0) on a 64x32 texture is unwrapped
- **THEN** its north face covers texels (8,8)-(16,16)

### Requirement: Pose and animation evaluation
The system SHALL compose bone transforms down the parent chain and SHALL sample keyframe animations with linear and Catmull-Rom interpolation, looping where the definition loops.

#### Scenario: Keyframe boundary
- **WHEN** an animation is sampled exactly at a keyframe timestamp
- **THEN** the channel value equals that keyframe's value

### Requirement: Procedural animation
The system SHALL provide walk-cycle limb swing for biped and quadruped models and head look from yaw and pitch, and SHALL fall back to the static pose for other models.

#### Scenario: Standing still
- **WHEN** limb swing amount is zero
- **THEN** limbs keep their rest pose

### Requirement: Instanced rendering
The system SHALL draw all entities of one model with a single instanced draw using per-instance bone matrices from a storage buffer and entity textures from a texture array.

#### Scenario: Row of mobs
- **WHEN** a pig, zombie, creeper, cow, villager and armor stand are rendered offscreen
- **THEN** each appears textured with vanilla proportions
