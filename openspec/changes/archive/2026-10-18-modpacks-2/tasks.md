# Tasks

## 1. Wire format

- [x] 1.1 `McrCodecs` wire shapes for modded component, particle and entity data serializer codecs in the registry report
- [x] 1.2 `mcr_protocol::types::modded`: bounded shape parser and skipper, active table installed with the registries; tests
- [x] 1.3 `DataComponent::Modded`, `Particle::Modded`, modded entity data skipped; hostile-input tests

## 2. Java models and fluids (ABI 22)

- [x] 2.1 `McrRenderExport`: fluid models at the title, block quads on `MSG_MODELS`, selected packs; `RENDER_REPORT` downcall
- [x] 2.2 `mcr_mesh::java`: bounded report parser and merge; tests
- [x] 2.3 Model cache: Java quads for states Rust cannot bake, modded fluid families, constant tint palette kinds; tests
- [x] 2.4 Fluid mesher: per-family sprites, layer, overlay and tint; test
- [x] 2.5 Invisible render shapes export no quads

## 3. Join

- [x] 3.1 `FSTATE_WAITING` for loading screens; compose keeps the last frame
- [x] 3.2 Title prebake, `ModelCache::remap` to the server's tables, in-game rebake when Java answers; tests
- [x] 3.3 Java's selected resource packs inside the game dir join Rust's resource stack

## 4. Memory and textures

- [x] 4.1 Identical compiled quad runs stored once
- [x] 4.2 Sprite sizes beyond the renderer's groups resampled; test
- [x] 4.3 Automatic JVM heap from the client mod count, RAM-aware ceiling, `heap_explicit` in settings and profiles, shown in the Mods screen, launcher and log; tests

## 5. Verify

- [x] 5.1 Screenshots: Medieval's Easy Anvils, Visual Workbench, Tavern blocks; Echoes' ichor
- [x] 5.2 Medieval singleplayer join without a detach
- [x] 5.3 Both packs measured against the baseline
- [x] 5.4 Docs: `docs/mod-bridge.md`
