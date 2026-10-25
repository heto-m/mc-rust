## Why

`vanilla-parity-6` worked through 15 gaps in 4 batches. Its close-out re-audit left a short list: the F3 debug
entries are a subset of vanilla's, two small screen and cloud differences, and live checks that need a
managed singleplayer server, a window file drop, the Fabric JVM or an online account.

## What Changes

Each leftover is recorded in `design.md` with vanilla's behaviour, mcr's code location, impact and a
severity. The fixes are grouped into 3 small batches (`tasks.md`):

1. F3 debug entries
2. World Options scroll and cloud depth
3. Live checks

8 items: 1 visible, 7 minor, 0 gameplay-breaking.

## Out of scope

- The `visualize_*`, `chunk_section_*` and `3d_crosshair` debug entries (parity-4 deferred debug renderers).
- The F3+L zip and F3+1 pie formats, which follow this client's own profiler (see `vanilla-parity-6`).

## Impact

- Affected specs: `client-options`, `sky-rendering`.
- Code: `mcr-ui`, `mcr-app`, `mcr-render`. No protocol changes.
