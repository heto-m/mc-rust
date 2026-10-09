# Tasks

## 1. Crate and formats

- [x] 1.1 Create `mcr-render` with deps; `cargo check -p mcr-render` passes
- [x] 1.2 Implement `PackedQuad`/`ModelQuad` encoders with bit-layout docs; round-trip unit tests pass
- [x] 1.3 Implement arena offset allocator; fuzz test shows no overlaps and full coalescing
- [x] 1.4 Camera (f64, reverse-Z, frustum planes); unit tests for projection and plane extraction

## 2. GPU pipeline

- [x] 2.1 Device setup, buffers, staging ring, texture array with procedural placeholder; offscreen test creates renderer or skips
- [x] 2.2 WGSL decoder, cull compute and draw shaders; GPU test compares decoder to Rust
- [x] 2.3 Section upload/removal API, indirect(+count) draw with fallback, timestamps
- [x] 2.4 Synthetic terrain and test mesher; offscreen render test saves PNG and asserts pixels
- [x] 2.5 Culling and large-coordinate tests (behind camera, back faces, 30M offset identical)

## 3. App and measurement

- [x] 3.1 `mcr-app` window with fly camera, vsync option, stats; builds and runs
- [x] 3.2 Bench example for 48-chunk render distance; record numbers in design.md
- [x] 3.3 fmt, clippy, tests pass; archive and validate all
