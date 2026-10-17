## ADDED Requirements

### Requirement: Camera mods inside the alignment
Java's camera alignment SHALL run vanilla's own code with mcr's camera position and rotation substituted, rather than skipping it, so that mods hooking inside the alignment still change the camera. Their change SHALL reach mcr through the camera handoff.

#### Scenario: Rotation hook after the alignment's rotation
- **WHEN** a mod sets the camera's rotation right after vanilla's alignment set it, as Freelook does
- **THEN** Java reports the moved camera and mcr renders terrain from it

#### Scenario: No camera mod
- **WHEN** no mod hooks the camera alignment
- **THEN** Java's camera equals mcr's camera and nothing is reported

### Requirement: Mouse turn capture
Java SHALL report when a mod consumes the mouse turn of the local player before vanilla applies it. While it does and Java draws the world, mcr's player SHALL not turn with the mouse, and Java SHALL still receive the mouse motion.

#### Scenario: Freelook
- **WHEN** a mod cancels the local player's turn and turns only its camera, and the user moves the mouse
- **THEN** mcr's player keeps its rotation and the camera turns

#### Scenario: No turn mod
- **WHEN** no mod consumes the turn and the user moves the mouse
- **THEN** mcr's player turns as before

### Requirement: Camera roll handoff
When a mod rolls Java's camera about its view axis, Java SHALL report the roll with the handed-off camera, and mcr SHALL roll its view by it on top of its own tilts. Java SHALL draw its world with the same roll mcr renders, so entities stay aligned with mcr's terrain.

#### Scenario: Rolled camera
- **WHEN** a mod rolls the camera's rotation 15 degrees about its view axis
- **THEN** mcr's view rolls by the same angle and Java's entities stay on mcr's ground

#### Scenario: No roll
- **WHEN** no mod rolls the camera
- **THEN** the reported roll is 0 and nothing is handed off

### Requirement: Third-person pull-back from the eye
In third person, Java's camera alignment SHALL start from mcr's eye and pull back by mcr's camera distance, so that without a mod Java's camera equals mcr's, and a mod that replaces the pull-back offsets the camera from the eye as in vanilla.

#### Scenario: Shoulder camera
- **WHEN** a mod redirects the third-person pull-back to add a sideways offset
- **THEN** mcr renders from the offset camera

#### Scenario: Plain third person
- **WHEN** no mod changes the third-person camera
- **THEN** nothing is handed off and mcr renders its own third-person camera
