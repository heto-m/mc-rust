## 1. State

- [x] 1.1 `mcr-client`: bounded server post effect list, cleared on respawn and join; configuration-phase packet ignored; tests
- [x] 1.2 `/posteffect` suggestions list vanilla chain ids

## 2. Render

- [x] 2.1 `mcr-render` post module: chain ids, pass graph, WGSL programs, pass-graph unit test
- [x] 2.2 Post stack: pooled targets, cached bind groups and uniforms, lazy pipelines
- [x] 2.3 Renderer: run chains before UI in offscreen, window (intermediate image) and scene modes
- [x] 2.4 `ExternalSubmitter::before_ui`; mod-bridge GUI compose moved there
- [x] 2.5 Offscreen image tests: invert, creeper, spider, blur, and renderer wiring

## 3. App and Java

- [x] 3.1 `mcr-app`: spectated chain (camera change, perspective, F4 toggle), menu blur radius, renderer feed
- [x] 3.2 Java bridge: skip mcr-owned vanilla chains in `applyPostEffects`
- [x] 3.3 Glowing entity outline: silhouettes in team colours, `entity_outline` chain, blend; GPU test

## 4. Verification

- [x] 4.1 Live: `/posteffect` and `/spectate` creeper, spider and enderman captured offscreen against a vanilla server
- [x] 4.2 Visual check of parity-followups-1 features, fixes or follow-ups recorded
- [x] 4.3 Gate: fmt, clippy, tests, openspec validate
