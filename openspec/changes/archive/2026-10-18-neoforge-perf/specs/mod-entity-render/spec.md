## MODIFIED Requirements

### Requirement: Camera mods inside the alignment
Java's camera alignment SHALL run vanilla's own code with mcr's camera position and rotation substituted, rather than skipping it, so that mods hooking inside the alignment still change the camera. Their change SHALL reach mcr through the camera handoff. Under NeoForge, `ViewportEvent.ComputeCameraAngles` SHALL start from mcr's rotation, so its listeners change the camera on top of mcr's, and NeoForge's own rotation calls in the alignment SHALL take mcr's rotation wherever vanilla's do.

#### Scenario: Rotation hook after the alignment's rotation
- **WHEN** a mod sets the camera's rotation right after vanilla's alignment set it, as Freelook does
- **THEN** Java reports the moved camera and mcr renders terrain from it

#### Scenario: No camera mod
- **WHEN** no mod hooks the camera alignment
- **THEN** Java's camera equals mcr's camera and nothing is reported

#### Scenario: Turning under NeoForge without a camera mod
- **WHEN** mcr's camera turns every frame under NeoForge and no mod changes the camera
- **THEN** Java's camera equals mcr's in every frame, Java hands no camera back, and mcr renders terrain from its own camera
