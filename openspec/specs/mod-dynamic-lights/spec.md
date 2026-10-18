# mod-dynamic-lights Specification

## Purpose
Dynamic light sources a Java mod computes (LambDynamicLights' held items, burning and glowing entities, beams)
reach mcr over the bridge and light mcr's own terrain in the shader, bounded and at no cost without such a mod.

## Requirements

### Requirement: Mod light sources reach the bridge
While Java draws the world and LambDynamicLights is loaded, the bridge SHALL publish the mod's active dynamic light sources after every Java frame, at most 256, nearest to the camera first. Entity and particle sources SHALL be points and line, beacon and end gateway behaviours segments, each with its light level and its world position at Java's partial tick.

#### Scenario: Held torch
- **WHEN** the player holds a torch with LambDynamicLights loaded and its mode on
- **THEN** the next composed frame carries one source at the player's eye with level 14

### Requirement: Mod toggles and failures
The bridge SHALL publish no sources while the mod's dynamic lighting mode is off. A failure to read the mod SHALL disable the channel for the session without affecting the game.

#### Scenario: Mode off
- **WHEN** the mod's dynamic lighting mode is set to off
- **THEN** the bridge publishes no sources and still reports the provider

#### Scenario: Unknown mod version
- **WHEN** a field or method the bridge looks up is missing
- **THEN** the bridge logs it once, publishes no sources and the game continues

### Requirement: Untrusted light data is bounded
Rust SHALL read at most 256 sources from the shared light block, drop entries with non-finite values, clamp levels to 0–15 and drop level 0, clamp each segment to 4096 blocks per axis, and keep the previous list when the block was being written during the read.

#### Scenario: Hostile block
- **WHEN** the block claims 100000 sources, with a NaN position and a level of 99 among them
- **THEN** Rust returns at most 256 sources, without the NaN one, and the level 99 source as level 15

### Requirement: Dynamic light term in the terrain shader
mcr SHALL light opaque, cutout and translucent terrain with up to 64 dynamic light sources per frame, nearest to the camera among those whose reach touches the view frustum. A fragment's block light SHALL become `max(block, level − distance · 15 / 7.75)` before the lightmap lookup, with the distance measured from a point half a block out along the face normal to the source's point or segment. No section SHALL be remeshed for it.

#### Scenario: Torch at night
- **WHEN** the player holds a torch at midnight on open ground with no block light
- **THEN** the ground within about seven blocks is lit, brightest under the player, and the frame looks unchanged elsewhere

#### Scenario: Dropped glowstone and burning mob
- **WHEN** a glowstone item lies on the ground and a zombie burns nearby at night
- **THEN** the terrain around each is lit

### Requirement: No cost without a provider
Without mods the renderer SHALL create no dynamic light pipelines or buffers, and in any frame without dynamic light sources it SHALL draw terrain with the same pipelines as before.

#### Scenario: Vanilla session
- **WHEN** mcr runs without mods
- **THEN** no dynamic light pipeline exists and the frame's terrain draws are unchanged

### Requirement: Dynamic lights setting
Video Settings SHALL offer "Mod Dynamic Lights" while Java reports a dynamic light provider, stored as `mcrDynamicLights` in `options.txt` and on by default. With it off mcr SHALL ignore the sources.

#### Scenario: Turned off
- **WHEN** the user turns Mod Dynamic Lights off while holding a torch at night
- **THEN** the terrain is no longer lit by the torch, and Java's entities still are
