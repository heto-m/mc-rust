# Proposal

## Why

The merged mod-layer changes left small defects: the native "mcr" menu draws over Java's pause buttons, SDL
modifier state lacks the lock bits, text can arrive twice through winit's key text and IME commit, keys held while
a Java screen closes stop working, the crash prompt's "Try again" quits instead of retrying, and dependencies
provided two levels deep in nested jars do not resolve.

## What Changes

- The "mcr" menu draws an opaque panel with the title above the buttons.
- Exactly one resource-pack prompt shows in mod mode: mcr's native one. Java's push and pop handlers are
  cancelled while the net bridge is attached, because Java's `ServerboundResourcePackPacket` is not on the forward
  list and would be dropped.
- Caps Lock and Num Lock toggles set `SDL_KMOD_CAPS` and `SDL_KMOD_NUM` in the modifier state sent to Java.
- With an IME active, key press text is ignored and `Ime::Commit` is the only text source.
- Movement keys still held when a Java screen closes resume in mcr.
- "Try again" spawns mcr again with the same arguments and then quits.
- Nested jars are read two levels deep under a tree-wide byte and jar budget.
- Deferred: Mods screen strings as lang keys (mcr-ui loads lang only from the user's resource stack, with no
  mechanism for mcr's own native strings).
