## ADDED Requirements

### Requirement: Improved transparency
With `improvedTransparency` on, translucent terrain and translucent particles SHALL be composited with
order-independent transparency as vanilla's wavelet transparency does, opaque particles SHALL be covered by
the translucent world in front of them, and with it off the renderer SHALL allocate and draw nothing for it.

#### Scenario: Water behind glass
- **WHEN** water is seen through stained glass with improved transparency on
- **THEN** both blend in depth order, whatever order they are drawn in

#### Scenario: Option off
- **WHEN** improved transparency is turned off
- **THEN** the frame and GPU time match the classic path

### Requirement: Leaf shading cost
Cutout terrain SHALL not be shaded behind solid terrain more often than vanilla's SOLID-then-CUTOUT order
allows, without changing the picture.

#### Scenario: Forest
- **WHEN** a forest is drawn at render distance 16
- **THEN** the frame is pixel-identical and GPU time is no higher than with one shared pass
