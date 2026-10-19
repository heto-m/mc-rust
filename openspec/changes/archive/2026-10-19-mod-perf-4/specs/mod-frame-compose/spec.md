## ADDED Requirements

### Requirement: Loaded when the reload ends
In mod mode, Java SHALL report itself loaded once its game load finished and no resource reload is running, even
while a loading overlay is still fading out. mcr's `--connect` and the render report SHALL wait for that, not for
the overlay to be gone.

#### Scenario: Connect after the first load
- **WHEN** mcr starts with `--connect` and Java's first resource reload finishes
- **THEN** mcr connects in the same second, without waiting the two seconds the loading overlay takes to fade

#### Scenario: Reload in game
- **WHEN** a resource reload runs in game
- **THEN** Java is not reported loaded until that reload finished
