## ADDED Requirements

### Requirement: Translucent draws follow uploaded geometry
A section's translucent draw (quad range, tint table and slot) SHALL switch to a new mesh only in the frame whose uploads write that mesh, and until then SHALL keep drawing the previous mesh. A section that has never been uploaded SHALL draw no translucent quads. Ranges freed by a replaced mesh SHALL NOT be overwritten before the frame that switches away from them.

#### Scenario: Upload budget exceeded
- **WHEN** a section with water is replaced while earlier uploads fill the frame's upload budget
- **THEN** that frame still draws the section's previous water, and a later frame draws the new water

#### Scenario: Removed before upload
- **WHEN** a section is replaced and then removed before its upload is written
- **THEN** no translucent quads are drawn for it afterwards
