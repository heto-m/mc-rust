## ADDED Requirements

### Requirement: Footsteps a mod replaced
Java SHALL periodically check, without playing anything, whether vanilla's footstep path of the local player still reaches its step sound. When a mod has taken it over, Java SHALL report it, and mcr SHALL stop playing the local player's footsteps while the mod's own sounds play through Java.

#### Scenario: Footstep mod
- **WHEN** a mod cancels the player's vanilla step sound and plays its own, and the player walks
- **THEN** only the mod's footsteps are heard

#### Scenario: No footstep mod
- **WHEN** no mod touches footsteps and the player walks
- **THEN** mcr plays its footsteps as before
