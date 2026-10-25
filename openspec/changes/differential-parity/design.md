## Context

Horizon (a public Rust rewrite of 26.3) reached a verifiable parity claim with 281 differential harnesses:
Java programs call vanilla's classes, Rust probes call theirs, and a runner diffs the outputs over
generated cases. We take the same idea, with two advantages:

- we already boot the real client jar in-process (`mcr-jvm`, `java/mcr-boot`);
- the real client already renders through our GPU backend (`java/mcr-bridge/.../gpu`,
  `McrRenderPass`, `NullDeviceBackend`), so its draws can be recorded without patching vanilla.

## Goals / Non-goals

- Goals:
  - every client subsystem has at least one harness;
  - every harness is deterministic and runs offline;
  - a failing case is minimal and readable;
  - a rerun after a Rust-only change does not rerun the JVM.
- Non-goals:
  - harnesses for server logic;
  - proving equality outside the generated cases. A score is the share of cases that match, not a proof.

## Decisions

### D1 Oracle / probe / case

- **Shape:** a harness is `name`, a seeded case generator, a Java oracle and a Rust probe.
- **Cases:**
  - Cases are JSON lines written by the Rust generator in `crates/mcr-parity`, seeded by name.
  - Both sides read the same file, which avoids two random generators drifting apart.
  - Each case carries an id.
- **Outputs:** each side writes `{"id", "out"}` lines in a normalized form defined per harness, with
  floats rounded per harness and lists in a canonical order where vanilla's order is not observable.
- **Diff:** the runner diffs per id and prints the first N mismatches as `case`, `vanilla` and `mcr`, with
  a JSON path to the first differing field.

### D2 Oracles run vanilla, not a copy of it

- `java/mcr-oracle` is our own Java code, compiled against the user's client jar and libraries. The
  vanilla classes stay in the jar. It calls the classes it compares, for example:
  - `Font` / `StringSplitter`;
  - `Language`;
  - `ModelBakery` / `BlockModelShaper`;
  - `SectionCompiler`;
  - `ItemModelResolver`;
  - `ItemStack.getTooltipLines`;
  - `Options`;
  - `KeyMapping`;
  - `EntityModel.setupAnim`;
  - `LocalPlayer` movement through `Entity.move`.
- **Bootstrapping:** `SharedConstants` plus `Bootstrap.bootStrap()` covers registries.
  - Harnesses that need resources load the jar's default pack through `ResourceManager`.
  - Harnesses that need a `Minecraft` instance use the draw-stream boot (D4).
- **Isolation:** every JVM run is `unshare -rn` under the 5G memory scope.

### D3 Caching and selection

- **Oracle output** is cached under `.cache/parity/<name>/` by a hash of:
  - the client jar sha;
  - the oracle's class files;
  - the case file.
- **Probe output** is not cached: the probes are cheap.
- **Selection:** `cargo xtask parity` runs the harnesses whose probe crate scope or oracle changed:
  - with no arguments: harnesses whose inputs changed or that never passed;
  - `cargo xtask parity <names>`: the named harnesses;
  - `--all`: every harness;
  - `--failed`: the ones that failed last time.
- **Status:** the last result per harness is kept in `.cache/parity/status.json`.

### D4 Draw-stream harness

- **Boot:** the vanilla client boots headless in `mcr-jvm` with no mods, on the recording variant of
  our GPU backend.
- **Scenes:** a scene script puts it into a state and the backend records the frame as a normalized
  stream. Example states:
  - a screen at a GUI scale and window size;
  - the HUD with a given player state;
  - the F3 overlay;
  - a world frame at a fixed camera on a local server.
- **Stream, per draw:**
  - pipeline name;
  - texture identity, as an atlas sprite or resource path plus a UV rect in that texture's pixels;
  - scissor;
  - blend;
  - vertices as screen rect plus colour for GUI quads, or positions plus UV plus colour plus normal
    for world geometry;
  - order.
- **Our side:** the same scene from our client. `mcr-ui` batches map back to sprite or glyph identity
  through the atlas slots, and `mcr-render` exports section and entity draws in the same form.
- **Comparison:**
  - GUI streams compare draw by draw in order;
  - world streams compare as multisets per render layer, since draw order within a layer is not
    observable except for translucency, which compares sorted.

### D5 Frame harness

- **Setup:** the same local vanilla server, world, seed and camera path.
- **Comparison:** the vanilla client's frame, replayed by our wgpu backend, against our native frame.
  Both go through the same GPU and driver, so the difference is logic, not rasterizer.
- **Scoring:** per-pixel diff with a tolerance of 1 per channel, an image of the differing pixels, and a
  score per frame.
- **Role:** a catch-all for what the other harnesses do not reach, such as sky, fog, weather, particles
  and post effects. Particles and weather are seeded or frozen with `/tick freeze` and `/weather clear`
  where randomness makes a pixel diff meaningless.

### D6 Gate and score

- **Gate:** `merge.sh` runs `cargo xtask parity` (the changed harnesses) after the tests. A harness that
  already fails on `main` may keep failing (`--known-failures`). A branch that adds a failure fails the
  gate.
- **Score:** the `differential-parity` spec keeps a subsystem table with each harness's case count and
  pass rate. The client's parity number is the average.
- **Filing:** new mismatches go into the next `vanilla-parity-N` change.

## Risks / Trade-offs

- **Headless `Minecraft` boot** is the hardest part. The mod-mode boot already does it with Fabric. The
  harness boots plain vanilla through `java/mcr-boot` without a loader.
- **Wrong normalisation hides bugs.** Each harness's normalisation is reviewed against what the player
  can see. Rounding is never coarser than one GUI pixel or 1/256 block.
- **Long runs:** oracle caching keeps the inner loop to the Rust side. Harnesses shard by case range
  when large.
- **Legal:**
  - oracles are our code;
  - case files are generated by us;
  - oracle outputs are derived from Mojang's code and assets, so they never leave `.cache/`.

## Adding a harness

A harness is one name on both sides; `cargo xtask parity <name>` runs it.

1. **Rust:** add `crates/mcr-parity/src/<name>.rs` with a type implementing `Harness`:
   - `case(id, rng)` builds case `id` from the seeded `Rng`; `count()` is how many (an exhaustive
     harness such as `block_model` maps the id straight to a block state);
   - `probe(env, cases)` answers every case with our code, in case order; wrap each answer in
     `guarded` so a panic in our code becomes that case's `{"panic"}` output, not a dead run;
   - `tolerance(field)` is the absolute slack for numbers under an object key (default 0), for
     values whose rounding differs by design (see `section_mesh`'s colour);
   - `scope()` lists the repository paths our answer depends on (selection reruns the harness when they
     change).

   Register it in `HARNESSES` in `lib.rs`.
2. **Java:** add `java/mcr-oracle/src/dev/mcr/oracle/<name>/…Harness.java` implementing
   `dev.mcr.oracle.Harness`, with a constructor taking `Boot` (registries are up; `clientResources()`
   gives the jar's default pack plus the asset index; `Boot.tags()` binds the jar's data-pack tags, which
   vanilla code asking `state.is(tag)` needs). Register it in `Oracle.HARNESSES` under the same name.
3. **Normalize** both sides to the same JSON: floats through `round` with a power-of-two step, lists in
   a canonical order where vanilla's order is not visible. Call vanilla's classes; when one needs a GPU
   or `Minecraft`, construct around that piece (see `VanillaFonts`) rather than copying its logic.
4. Run it. Cases, oracle output, probe output and `summary.json` land in `.cache/parity/<name>/`; none of
   it is committed.

Exit codes of `cargo xtask parity`: 0 every selected harness passed, 1 mismatches, 2 setup or oracle
failure. `--fresh` reruns the oracle even when its cache key holds; `--max N` sets how many mismatches
are printed in full.

## Findings

### font (3000 cases, 1156 match: 38.5%)

Glyph advances (normal and bold) match for every glyph of the four known fonts, across ASCII, Latin,
Greek, Cyrillic, CJK, Hangul, emoji and unifont. `Font.width` of legacy strings matches except one
surrogate edge. The mismatches, by cause:

- **Chat continuation indent** (~1550 cases, every wrapped chat message). `ComponentRenderUtils
  .wrapComponents` prefixes each wrapped (not `\n`-started) continuation line with a space; `ChatHud`
  uses `Ui::wrap` without it.
- **`§r` after a split** (~255 cases). Vanilla's `StringSplitter.splitLines` measures with `§r`
  resetting to `Style.EMPTY` (dropping the component's bold and font), and each wrapped line's
  component takes the style active at its break, so `§r` on a later line resets to that (e.g. still
  bold after an earlier `§l`). `wrap` and the drawn cells always reset to the component's own style,
  so breaks and line widths differ.
- **Unknown font ids** (89 cases). A style font with no font definition (`minecraft:nonexistent`) draws
  every glyph as the missing glyph (advance 6, bold 7) in vanilla; `FontId::from_name` falls back to
  `minecraft:default`. Fonts other than the four known (`include/*`) also fall back.
- **Trailing invisible codes** (10 cases). When only formatting codes (or a lone `§`) follow the last
  break, vanilla still emits a final empty line (in chat, an indented empty line); `wrap` emits none.
- **Empty chat message** (2 cases). Vanilla adds one empty line; `ChatHud::add` adds none.
- **`§` before a supplementary character** (2 cases). Vanilla takes the high surrogate as the code and
  draws the orphaned low surrogate as U+FFFD; `push_legacy` swallows the whole code point.

### text (4000 cases, 2341 match: 58.5%)

Cases are typed JSON components (`ComponentSerialization.CODEC` over NBT on the Java side,
`Component::from_nbt` on ours): literal text with `§` codes, translatables over real `en_us` keys and
missing keys with generated fallbacks (`%s`, `%1$s`, `%%`, `%d`, `%0$s`, `%5$s`, stray `%`), string,
number, boolean and component arguments, score, selector and nbt contents, every style field and
`extra`. Each side prints the plain string (`getString` / `to_plain_string`) and the draw runs
(`StringDecomposer.iterateFormatted` / `flatten_chat` with `Lang` for `en_us`), adjacent runs of equal
style merged. No case differs in style alone. By cause (cases; one case can have several):

- **Plain string has no language** (1204, 385 alone). `Component.getString` resolves translatables
  through `Language.getInstance()`; `Component::to_plain_string` prints the fallback or the key. Case 0:
  `{"translate":"options.on"}` is `ON` in vanilla, `options.on` in ours.
- **Missing or out-of-range arguments** (609). `TranslatableContents.decompose` catches
  `TranslatableFormatException` and renders the whole template as literal text; ours drops each
  unfilled placeholder. Case 2: the fallback `y§z %s %s` with no arguments draws `%s %s` in vanilla,
  ` ` in ours.
- **Score contents** (355). `ScoreContents` has no `visit` (it only resolves on the server), so it draws
  nothing; ours draws the holder name. Case 59: `{"score":{"name":"@s",…}}` is empty vs `@s`.
- **NBT contents** (291). Same for `NbtContents`; ours draws the path. Case 18: `Items[0]`.
- **Unsupported format specifiers** (`%d` 36, other `%x` / `%.2f` / stray `%` 258). `FORMAT_PATTERN`
  only takes `%s`, `%n$s` and `%%`; anything else makes `decompose` fall back to the raw template. Ours
  substitutes `%d` and drops unknown specifiers. Case 808: fallback `%d HK…` stays `%d HK…` in vanilla,
  ours ` HK…`. (`%d` in lang files works in vanilla only because `Language.loadFromJson` rewrites it.)
- **`%0$s`** (160). Vanilla: index 0 is out of range, so the raw template. Ours: `to_plain_string`
  panics (`attempt to subtract with overflow`, mcr-protocol `text.rs`; 126 cases) and `flatten_chat`
  uses argument 0 (34). Case 7.
- **Double arguments** (26). Arguments render through `Object.toString`, so Java's `Double.toString`
  (`2.63E9`, `1.0E-4`); ours prints `2630000000.0`. Case 134 (`multiplayer.player.joined`).
- **`%%` is its own piece** (5). `decompose` emits `%` as a separate literal, so legacy codes active in
  the template do not carry into it and the run splits; ours keeps it in the surrounding literal.
  Case 1886.

### block_model (35723 cases, every state: 100%)

Vanilla's `ModelManager.loadBlockModels` → `ModelDiscovery` → `ModelBakery.bakeModels` on a fake
`blocks` atlas (one 2×2 cell per sprite, so atlas UVs decode to sprite and local UV) against
`ModelBaker::bake_block_state`. Compared per multipart part and weighted variant: weight, AO flag,
particle, and each cull bucket's quads in draw order with face, sprite, tint index, shade direction,
emission and the four vertices (position and UV at 1/1024). Every state matches: 468k quads including
67,926 off-grid positions, 14,553 multipart states and 1,625 weighted groups. The render layer is not
compared here; `section_mesh` covers it.

### section_mesh (400 cases, 11 match: 2.75%)

Generated sections (18³ with the neighbour shell; sparse scatter, terrain floors, dense boxes) over a
weighted mix of full cubes, slabs, stairs, fences, panes, glass, leaves, plants, torches, lanterns,
cauldrons, hoppers, scaffolding, water and lava at every level including falling, waterlogged blocks,
random or uniform light (an emitter's own cell at least its emission) and random biome tints. Vanilla:
`ModelBlockRenderer.tesselateBlock` (AO on, cutout leaves) and `FluidRenderer.tesselate` on a fake
`BlockAndTintGetter`, recorded per `ChunkSectionLayer`. Ours: `Mesher` without greedy merging, quads
decoded the way `terrain.wgsl` draws them, merged fluid tops split per block; `ModelCache` gained
`layer_sprites` so a texture layer can be named. Compared as multisets per layer, bucketed by centroid:
sprite, winding normal (in eighths, ±1; the vertex format has no normal), vertices at 1/1024 with UVs
sprite-local, colour after shade, AO and tint (±5/255: vanilla floors at each `ARGB` step) and packed
block and sky light. By cause (quads, cases):

- **Fluid quad quantisation** (124k quads, 354 cases, 47 alone). Fluid quads go through the model quad
  format: positions on 1/128, which drops `FluidRenderer`'s 0.001 insets on sides, bottoms and
  heights, and UVs on 1/16 tile. Case 0: a water side at x 14.999 in vanilla, 15.0 in ours, v 0.1299
  vs 0.125.
- **Per-face render layer** (48.9k, 199). `FaceBakery.computeMaterialTransparency` gives each face the
  transparency of the sprite region its UVs cover (`SpriteContents.computeTransparency`), so an opaque
  region of a cutout sprite bakes as `SOLID`; ours picks the layer by the whole sprite. Hits cauldrons,
  hoppers, iron bars, lanterns, torches, scaffolding, trapdoors, doors, dirt path sides and pink petal
  stems. Case 2: `cauldron_side` faces are `solid` in vanilla, `cutout` in ours.
- **Model quad quantisation** (23.6k, 279, 14 alone). `ModelQuad` positions are on 1/128, so off-grid
  geometry (random plant offsets, rotated elements, the petals at 2.99/16) moves by up to 1/256.
  Case 30: a `pink_petals_stem` vertex at x 6.0723 vs 6.0703.
- **Lava layer** (7.6k, 75). `FluidModel.Unbaked.bake` takes the layer from the still and flowing
  sprites' transparency, so lava (opaque sprites) is `SOLID`; ours draws it `cutout`. Case 4.
- **Flat light interpolated and truncated** (2.5k, 41). Flat-lit models (AO off, or an emitter:
  `ModelBlockRenderer.tesselateBlock` uses AO only at emission 0) take one light value per quad
  (`BlockModelLighter.prepareQuadFlat`). `Mesher::push_model_quad` blends the four corner values by
  vertex position in `f32` and casts to `u8`, so an off-grid vertex lands one unit low. Case 30: a petal
  stem vertex sky 95 vs 96; torches the same.
- **Faces against stairs never cull** (335, 57). `Block.shouldRenderFace` culls against the
  neighbour's face occlusion shape, which is full on a stair's back, its bottom (bottom half) or its
  top (top half); ours keeps every face touching a stair, whatever its shape or facing. Case 42: stone's
  top under `oak_stairs[facing=west,half=bottom]`.
- **Fluid heights beside forced-solidity blocks** (322, 21). `FluidRenderer.getHeight` weighs a
  non-fluid neighbour as 0 when `!isSolid()`, and `BlockBehaviour.calculateSolid` honours
  `forceSolidOn` / `forceSolidOff` and is false for `dynamicShape` blocks (no shape cache); ours
  derives legacy solidity from the collision shape alone. Lanterns (forced on), ladders and snow
  (forced off) and scaffolding (dynamic) change corner heights and flow. Case 10: water in waterlogged
  leaves beside scaffolding has corners 0.807 in vanilla, 0.891 / 0.805 in ours.
- **One smooth-light vertex** (4, 1). A snow layer face where vanilla's smooth light gives one vertex
  143/159 and ours 144/160 (`BlockModelLighter.prepareQuadAmbientOcclusion`); not traced further.
  Case 78.

Without the two quantisation classes 144 of 400 cases match; without the layer classes as well, 289.

Harness bugs found and fixed on the way: vanilla's `FlowingFluid.getFlow` asks
`is(BlockTags.BLOCKS_FLUID_FLOW)`, so the oracle binds data-pack tags (`Boot.tags()`); unbound tags had
turned flowing tops still in ~50 cases. Random light ignoring emitters made neighbour emission
(`LightCoordsUtil.getLightCoords` takes the drawn block's own) look like a mismatch.
