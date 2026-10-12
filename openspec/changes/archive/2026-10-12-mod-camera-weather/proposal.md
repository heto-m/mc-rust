# Proposal

## Why

The compatibility sweep left Freecam broken and Particle Rain partial. mcr owns the camera, so a Java mod that
detaches or moves the camera changes nothing on screen and the player keeps walking. mcr also always draws its own
rain and snow, so a mod that replaces vanilla weather adds to it instead of replacing it. Two costs remain in the
frame composition: Java's GUI still renders bottom-up into its own main target, which costs two fullscreen flip
copies every frame. Java also runs no cave culling over its meshless sections, so it extracts block entities and
entities that mcr's own section graph has already ruled out.

## What Changes

- **Camera handoff.** Java reports its final camera (position, rotation, detached, input capture) whenever a mod
  made it differ from mcr's. While Java draws the world, mcr renders terrain from that camera, sanitised and kept
  inside the loaded world. When a mod takes the movement input, as Freecam does, the player stops moving and
  turning, and Java gets the movement keys and mouse look. Vanilla behaviour is unchanged when no mod moves the
  camera.
- **Weather replacement.** Java detects when a mod cancels or redirects vanilla's weather extraction and reports
  it. mcr then hides its own rain and snow.
- **Top-down GUI.** In the GUI slot, Java's main target resolves to mcr's scene. Projected pipelines get a vertex
  variant without naga's Y adjustment, and scissors flip, so both fullscreen flip copies go away.
- **Section visibility.** mcr's section graph result goes to Java through a shared bitset. Java skips block
  entities and entities that lie only in sections the graph did not reach.
- ABI: `BRIDGE_VERSION` 10. It adds `FrameBegin` render-camera fields and a handoff flag, frame-state camera and
  weather fields, and a shared section-visibility block.
