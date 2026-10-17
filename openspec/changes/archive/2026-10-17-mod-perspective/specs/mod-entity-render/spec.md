## ADDED Requirements

### Requirement: Java leads the perspective
While Java draws the world, Java's camera type SHALL decide the perspective. Java SHALL report it each frame as first person, third person back or third person front, and mcr SHALL follow it for its world camera, its native player model and its hand. A mod perspective on top of a camera type SHALL reach mcr through that camera type and the camera handoff.

#### Scenario: Mod perspective cycle
- **WHEN** a mod consumes F5 and switches Java from first person straight to third person front
- **THEN** mcr's perspective becomes third person front and its camera looks back at the player

#### Scenario: Shoulder view
- **WHEN** a mod switches Java to third person back and offsets the pull-back sideways
- **THEN** mcr follows into third person and renders from the offset camera

#### Scenario: Without mods
- **WHEN** no mods are loaded and the user presses F5
- **THEN** mcr cycles first person, third person back and third person front itself, as before

### Requirement: Perspective switch frame
Java SHALL NOT force its camera type to mcr's perspective. On a frame where Java's camera type and mcr's perspective disagree, Java's alignment SHALL run vanilla's own code, and the resulting camera SHALL reach mcr through the camera handoff until mcr has followed.

#### Scenario: Switch to third person
- **WHEN** Java switches to third person back while mcr is still in first person
- **THEN** mcr renders from Java's pulled-back camera and then from its own third-person camera, never from the eye with the player model visible

### Requirement: Mod roll of the level pose
Java SHALL report the roll about the view axis that mods add to the level's bob pose, as the difference between the pose vanilla's bob steps built and the pose vanilla alone builds from the same state. mcr SHALL roll its view by it on top of its own tilts while Java draws the world.

#### Scenario: Camera Overhaul style roll
- **WHEN** a mod rotates the bob pose stack 10 degrees about the view axis at the start of `bobHurt`
- **THEN** mcr's view rolls by 10 degrees and Java's entities roll with mcr's terrain

#### Scenario: No pose mod
- **WHEN** no mod changes the bob pose, including while hurt or walking with view bobbing
- **THEN** the reported roll is 0
