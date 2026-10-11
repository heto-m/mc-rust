## 0. Spike
- [ ] 0.1 Extract vanilla core shaders from the local `client.jar`, compile them with `glslc`-equivalent settings (shaderc from LWJGL natives, via a tiny Java tool on the host), run the combined-sampler rewrite prototype, and validate them with naga; record pass/fail counts in design.md

## 1. Shader path
- [ ] 1.1 `spv_fixup`: combined-sampler split, binding renumbering, push-constant-to-uniform, with unit tests on hand-written SPIR-V
- [ ] 1.2 naga `spv-in` → wgpu shader module, plus pipeline creation from `CreateInfo` (state mapping table) with unit tests per mapping
- [ ] 1.3 Test over all vanilla shaders from the local jar (`#[ignore]` without `.cache`): every non-texel-buffer pipeline validates

## 2. Resources and recording (Rust)
- [ ] 2.1 Handle slabs (generation-checked), buffers, textures, views, samplers, destroy-after-use
- [ ] 2.2 Encoder state: staging belt, ordered copies/clears/writes, row repacking, readbacks, fences, completion posting
- [ ] 2.3 Pass stream decoder, bind group cache with dynamic offsets, all draw variants, fan conversion

## 3. Java backend
- [ ] 3.1 `McrDeviceBackend`, resource wrappers, `DeviceInfo`
- [ ] 3.2 `McrCommandEncoder`, `McrRenderPass` stream writer, `McrTransientMemory`, mapping
- [ ] 3.3 Offscreen `McrSurface`, and switch `McrGpuBackend` from null to this device

## 4. Verification
- [ ] 4.1 `xtask golden-gui`: render the test screen with vanilla's OpenGL backend (real window, run locally, alone) into `.cache/golden/`; integration test renders the same screen through the bridge offscreen and compares at SSIM ≥ 0.98
- [ ] 4.2 Bench: title screen and test screen frames, recording ops per frame, Java record time, Rust encode time, bind group cache hit rate; record in design.md
- [ ] 4.3 fmt, clippy, tests, archive
