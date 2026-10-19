## ADDED Requirements

### Requirement: Client-side sounds
The client SHALL play the sounds vanilla plays locally: lightning thunder and impact, the portal trigger,
the user's own eating, drinking, bucket, flint and steel and spyglass sounds, air bubble pops and bubble
column inside sounds. `stop_sound` SHALL stop matching sounds, and entity-bound sounds SHALL follow their
entity.

#### Scenario: Stop sound
- **WHEN** the server sends stop_sound for the music source
- **THEN** the music stops
