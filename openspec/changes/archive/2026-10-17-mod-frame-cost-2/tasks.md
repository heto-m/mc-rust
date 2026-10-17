## 1. Measure

- [x] 1.1 Cost buckets per downcall kind, `finish`, staging and the render thread's queue writes and submits, in the profile line
- [x] 1.2 Baseline numbers: no mods in game, play-mods title, play-mods in game (warm), with the breakdown

## 2. Fewer submits

- [x] 2.1 `FrameBatch` in `mcr-render`: hooks batch instead of submitting; one submit per frame; `submitted` callback
- [x] 2.2 Batched slot drains: epochs, flush on a write of another submit, projection patch as a copy, readback maps and fences after the submit
- [x] 2.3 Upload ring reused only per submit; failed recordings dropped at `finish`; rejected batches fall back to separate submits
- [x] 2.4 Unit tests for the ring and the batch conflict rule

## 3. Downcall hot path

- [x] 3.1 Measure and cut per-call costs in pass replay and encoder ops; keep only what pays

## 4. Finish

- [x] 4.1 After numbers and breakdown in design.md; `docs/mod-bridge.md`
- [x] 4.2 Merge main, archive, full gate: fmt, clippy, test, java-build, JVM compose tests
