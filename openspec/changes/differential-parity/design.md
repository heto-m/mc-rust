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

### item_model (3000 cases, 2325 match: 77.5%)

Stacks are network bytes from our encoder (vanilla decodes them with `ItemStack.OPTIONAL_STREAM_CODEC` over
the jar's data-pack registries, so trims and enchantments are real holders; registry ids follow vanilla's
sorted load order). 40% carry an `item_model` pointing at a generated definition (select, condition,
range dispatch, composite, empty, every special type, every tint source, matrix and decomposed
transformations), served to us as a resource pack and to vanilla through `ClientItem.CODEC`. Vanilla bakes
with `ModelBakery` (leaves wrapped so each layer names its model or special) and updates with no level and
no owner; definitions that read the level or the clock (compass, clock) are left out. Ours:
`ItemModels::evaluate` through `StackView` for `gui`, `world_layers` (the `StackProps` path) for every
other display context. Compared per layer: model id or special (type and fields), base, tints, local
transform (1/1024, ±2) and glint. Special renderer arguments (patterns, profiles) are not compared. By cause
(cases; one case can have several):

- **Tint alpha** (≈200). `Dye` and `Firework` return their `default` as written, so an RGB default
  without alpha draws invisible, and `Constant` forces it opaque (`Constant` constructor,
  `ARGB.opaque`); `TintSource::tint` makes dye and firework defaults opaque and leaves constants as
  written. Case 3: dye default `9157281` is `#008BBAA1` in vanilla, `#FF8BBAA1` in ours; case 21: a
  constant `#BB90B35C` is `#FF90B35C` in vanilla.
- **Select or range with no fallback** (103). An unmatched `select` or a value below every threshold
  bakes to `context.missingItemModel` (`SelectItemModel.Unbaked.bake`, `RangeSelectItemModel.Unbaked
  .bake`), so the missing model draws; `Evaluator::node` draws nothing. Case 16.
- **Definitions vanilla rejects, ours loads** (≈130). `SelectItemModelProperty.Type.validateCases`
  rejects a `select` that repeats a value (76), `Transformation.CODEC` needs all four fields of a
  decomposed transformation (26), and `minecraft:bed` is no special model type (25,
  `SpecialModelRenderers`). Vanilla then draws the missing model. Case 22 (`rocket` twice), 14 (no
  `right_rotation`), 8 (bed).
- **Definitions that fail to load and unknown `item_model` ids** (81 + 21). `ModelManager.getItemModel`
  falls back to the missing model; `ItemModels::evaluate` falls back to the item's own definition.
  Unknown model types fail on both sides. Case 202: `item_model` `minecraft:mcr_none`.
- **Axis-angle rotations** (62). `ExtraCodecs.QUATERNIONF` also takes `{"axis", "angle"}`;
  `Parser::transformation` only takes `[x, y, z, w]`, rejects the definition and draws the item's own
  model. Case 168: a composite in vanilla, the item's `red_concrete_stairs` in ours.
- **Colours as float lists** (46). `ExtraCodecs.RGB_COLOR_CODEC` takes `[r, g, b]` floats;
  `Parser::int` reads only integers, so the default becomes 0 or -1. Case 44: a `custom_model_data`
  default `[0.25, 0.375, 0.875]` is `#FF3F5FDF` in vanilla, `#FFFFFFFF` in ours.
- **Prototype components** (≈70). `ItemStack.get` sees the item's default components; `StackView` and
  `StackProps` see only the patch. `has_component` on a default (every item has empty `enchantments`,
  case 1248) is true in vanilla, false in ours; `component` selects never match (`component_string` is
  not implemented; `rarity` is a default, case 241). `GLINT_BY_DEFAULT` lacks `written_book`'s
  default `enchantment_glint_override`, and the world path's `has_glint` reads only the patch (20,
  case 39: `enchanted_book` in hand).
- **Removed `item_model`** (21). `!minecraft:item_model` leaves `ItemModelResolver.appendItemLayers`
  with no id, so nothing draws; ours draws the item's own model. Case 47.
- **Damage on an undamageable item** (≤21). `Damage.get` with `normalize` divides by a max damage of
  0, the NaN picks the fallback (`RangeSelectItemModel.update`); ours reads 0. Case 79.
- **Charge type** (≤15). `Charge.get` is `ROCKET` when any projectile is a rocket; `StackView` looks at
  the first only and `StackProps` always says `ARROW`. Case 501: crossbow with arrow then rocket.
- **Special fields not kept** (≈35). `SpecialKind` drops `book`'s angles, `end_cube`'s `effect`,
  `chest`'s `openness` and `chest_type`, `shulker_box`'s `openness`, `banner`'s `attachment` and
  `head`'s `texture` and `animation`. Case 1 (book).
- **World path gaps** (≈15). `StackProps` has no `trim_material` (case 2116: a gold-trimmed diamond
  chestplate in `none`), no firework colours (case 1901) and only `custom_color` potions; the GUI path
  also ignores the effect colours `PotionContents.getColorOr` mixes (case 568: a splash potion by id).

### tooltip (3000 cases, 2805 match: 93.5%)

Generated stacks (half from 47 tooltip-heavy items, half any item) with up to 30 tooltip-bearing components (names, lore, rarity,
enchantments, trims, potions, containers, bundles, fireworks, attribute modifiers, books, banners,
songs, instruments, `tooltip_display`), removed components, advanced and creative flags each way.
Vanilla runs `ItemStack.getTooltipLines` with a fake player whose level enables the vanilla
features; ours runs `tooltip_lines` over `JarRegistries` (registry entries and enchantment tags from
the jar's data). Lines compare as draw runs; a plain space's colour and shadow are dropped on both
sides since they draw nothing.

- **Custom projectile names** (54). `ChargedProjectiles.addProjectileTooltip` uses
  `getDisplayName`: `[` + the italic custom name + `]` as separate styled parts; ours builds one plain
  `[name]` string. Case 66.
- **Enchantment attribute effects** (52). `LevelBasedValue` has `clamped`, `exponent`, `fraction`,
  `levels_squared` and `lookup` besides `linear` and `constant`; `level_value` returns `None` for
  them and the modifier line is lost. Case 264: Sweeping Edge II misses `+0.67 Sweeping Damage Ratio`.
- **Duplicate enchantment ids** (40). `ItemEnchantments.STREAM_CODEC` decodes into an
  `Object2IntOpenHashMap`, so a repeated id keeps the last level once; ours lists every entry and
  sums their modifiers. Case 64: Swift Sneak III in vanilla, I and III in ours.
- **Prototype pot decorations** (38). `decorated_pot`'s default `pot_decorations` is four bricks
  (`Items`, `PotDecorations.allBrick`) and `PotDecorations.addToTooltip` lists them; ours reads only
  the patch. Case 42.
- **Over-stacked templates** (6). `ItemStackTemplate.create` validates the count against the max
  stack and yields an empty stack (`0 x Air`, `[Air]`); ours shows the template as sent. Case 1082.
- **Nested tooltips have no player** (5). Projectiles call `addDetailsToTooltip` with a null player,
  so attack modifiers lose the player's base value (`+6 Attack Damage`); ours adds it (` 7`). Case 222.
- **Amplifier above 255** (2). `MobEffectInstance` clamps the amplifier to 255; ours keeps the sent
  value (`potion.potency.298`). Case 1740.
- **Durability on unbreakable items** (2). `ItemStack.isDamaged` needs `isDamageableItem`, which is
  false when unbreakable; ours shows `Durability:`. Case 2444.
- **Spawner hover text** (1). `Spawner.appendHoverText` adds `spawner.desc1`/`desc2` to `spawner` and
  `trial_spawner` without spawn data; ours adds nothing. Case 1506.

### options (3000 cases, 0 match: 0%)

Generated `options.txt` files: every key vanilla saves plus the two it reads without a field, values
in and out of range and in odd syntax, duplicate, unknown and legacy keys (`fancyGraphics`,
`graphicsMode`, LWJGL 2 key codes), data versions from none through the current 5023, CRLF and lone
CR line ends, a BOM, and missing files. Vanilla `new Options(client, dir)` loads it (with
`DataFixTypes.OPTIONS`) and `save()` writes it back; ours runs `OptionsFile::parse` then `to_text`.
Each saved key compares as a field (`version` left out: ours never writes one, so a vanilla client
reading our file runs every options fix from version 0), plus the key order, the key-binds screen's
collisions (`KeyBindsList.KeyEntry.refreshEntry`) and the key mapping table. Counts are cases; most
cases hit several classes. Ignoring the three universal classes, 106 cases match.

- **Key mapping order** (2462, every case vanilla loads). `Options.keyMappings` puts
  `key.debug.overlay` and `key.debug.modifier` before the hotbar slots and `key.debug.crash` later
  among the debug keys; `MAPPINGS` has them after `key.hotbar.9`. The table (names, defaults,
  categories, orders, toggles) otherwise matches.
- **Default render distance** (1605). The `Options` constructor's `renderDistance` default is 12;
  ours is 32. Case 3.
- **`debugGuiScale` and `glDebugVerbosity`** (1891). `processOptions` reads and writes both; ours has
  no field and writes them only when the file had them.
- **Out of range values** (1721). `OptionInstance` codecs (`UnitDouble`, `IntRange`,
  `Codec.doubleRange`) reject them and the option keeps its default; ours clamps. Case 3:
  `textBackgroundOpacity:1.477` is 0.5 in vanilla, 1.0 in ours. `-0.0` is out of `[0, 1]` too.
- **Unknown keys** (1117). Vanilla saves only what `processOptions` knows; ours keeps every other
  line (by design, for round trips), so legacy and mod keys stay. Case 5: `key_key.modded`.
- **Slider steps** (796). Sliders built with `IntRange.xmap` round what they load: `maxFps:117` is
  110, `chunkSectionFadeInTime:1.247` is 1.2, `mouseWheelSensitivity` snaps to its log scale and
  `fovEffectScale` goes through its square root. Ours keeps the value. Case 7.
- **Key names** (727). `InputConstants.getKey` also takes `key.keyboard.<code>`/`key.mouse.<n>`
  for codes without a name (and `key.mouse.3` is the right button again); ours unbinds them. Case 2:
  `key.keyboard.913`. Most collision differences (347) follow from these aliases.
- **Lenient booleans** (721). Plain fields use `isTrue` (anything but `true` is false) and
  `OptionInstance` booleans read lenient JSON (` true` and `"true"` are true); ours falls back to the
  default. Case 9: `invertYMouse:TRUE` is false in vanilla.
- **Enum ids out of range** (570). `ByIdMap` with `WRAP` wraps `particles`, `narrator`,
  `chatVisibility`, `attackIndicator`, `prioritizeChunkUpdates` and `textureFiltering`; ours uses
  the default. Case 7: `textureFiltering:-1` is 2.
- **Data fixes** (494 old, 88 versionless). `OptionsMenuBlurrinessFix`, the `graphicsMode` splits,
  `OptionsSetGraphicsPresetToCustomFix`, `OptionsAccessibilityOnboardFix`, `OptionsKeyLwjgl3Fix`,
  the `swapHands` rename and the rest; ours applies none. Case 12: `menuBackgroundBlurriness:0.25`
  from version 1935 is 3, `graphicsPreset` becomes `custom`.
- **Vanilla abandons the load** (538: 221 `NumberFormatException`, 214 JSON syntax, 75
  `StringIndexOutOfBoundsException`, 28 `IllegalStateException`). `Options.load` catches one
  exception for the whole file, so a key name in a pre-1344 file (`OptionsKeyLwjgl3Fix` parses it as
  an int), invalid JSON (`LenientJsonParser`) or `key.keyboard` with no code leaves every later
  option at its default; ours skips just that line. Case 6.
- **Float text** (159). Vanilla prints Java doubles (`1.0E-4`); ours prints `0.0001`. Same value.
- **Number syntax** (144 + 90). Lenient JSON takes ` 0.5` and integer options take `4.0`; it rejects
  `.5` and `5.`, which Rust's parser takes. Case 21, 31.
- **Booleans in unit sliders** (134). `UnitDouble`'s codec also takes `true`/`false` as 1/0; ours
  uses the default. Case 25.
- **Escapes in `soundDevice`** (98). Vanilla decodes the JSON string; `QuotedString::parse` only
  strips the quotes, so each save doubles the backslashes. Case 88: `"C:\\dir"`.
- **`tutorialStep`** (97). `TutorialSteps.getByName` turns unknown names into `none`; ours keeps them.
- **Pack lists** (88). Gson reads `[1,2]` as `["1","2"]`; ours drops the list. Case 56.
- **Lone CR line ends** (78). `BufferedReader.lines` splits on `\r`; `str::lines` does not, so the
  rest of the file becomes one value. Case 8.

### entity_pose (3000 cases, 2669 match: 89.0%)

35 adult models without keyframe animations (quadrupeds, spiders, golems, fish, flyers, humanoids,
slimes, the end crystal), cycled evenly, with generated walk position and speed, age, head yaw and
pitch, plus the state fields each model reads that our `EntityView` can carry (swing progress,
aggression, creepy, standing, attack and offer ticks, stun and roar, tentacle clock, in water, on
land, squish). Vanilla bakes the layer with `EntityModelSet.vanilla()`, builds the model class and
runs `setupAnim` on the render state; ours plans the entity through `base_anim` and
`Catalog::build`, then poses its first part like `EntityScene` (rig, hidden slots, rotation
overrides). Parts compare by path within 1/512 (`Mth.sin` is a 65536-entry table). Derived states
are fed the way our client assumes: guardian spikes out with a 0.45 per tick tail clock, the
phantom's flap clock is its age, a dolphin moves when it walks, a sheep not eating holds its head at
its pitch. Not covered: babies, keyframe-animated mobs (armadillo, camel, frog, sniffer, warden,
breeze, creaking...), arm poses and held items, variant layers.

- **Creeper legs** (74, every case that walks). `CreeperModel` binds `leftHindLeg` to
  `right_hind_leg` and so on, so each leg swings with its mirror's phase; `legs4` uses the names.
  Case 8: `right_front_leg` xRot 0.302 in vanilla, -0.302 in ours.
- **Turtle** (86). `TurtleModel` runs `QuadrupedModel.setupAnim` first (head look, leg xRot) and only
  then sets the land or swim yRot and zRot; `rig_mobs::turtle` never turns the head and leaves leg
  xRot at rest. Case 7.
- **Silverfish** (86). The `SIMPLE` table gives `minecraft:silverfish` `Rig::Endermite`, so its seven
  segments and three layers move with the endermite's amplitudes and the layers stay still;
  `SilverfishModel` scales segment `i` by `1 + |i - 2|` (yaw) and `|i - 2|` (sway). Case 15.
- **Tadpole tail** (85). `TadpoleModel` sets `tail.yRot = -k * 0.25 * sin(0.3 * age)` (k is 1 in
  water, 1.5 out); the tadpole runs `Rig::Fish`, whose cod-style wobble barely moves it. Case 25:
  0.25 in vanilla, 0.012 in ours.

### player_physics (1600 cases, 1598 match: 99.9%)

17 scenes (flat stone, oak stairs, slab stairs, a one-block step, ladders and vines on a wall, water
and lava pools, a 3x3 platform edge, soul sand, honey, cobweb, powder snow, ice, blue ice, a slime
floor dropped onto, elytra from y 90), each 40 ticks of generated held keys (forward biased, with
jump, sneak, sprint and strafes) and turning yaw and pitch. Vanilla runs a real `LocalPlayer`
(`KeyboardInput` over `Options` key mappings, `tick` then `sendChanges`) in a `ClientLevel`
subclass that serves the scene's blocks, fluids, chunks and collisions and nothing else: no
entities, sounds, particles, rain or dimension layers (environment attributes at their defaults,
the overworld's). Ours runs `PlayerPhysics::tick` over the same blocks. Each tick's position and
velocity compare within 1e-6, and the ground flag exactly. Survival defaults only: no effects,
attributes, abilities, vehicles, entities or server corrections; elytra cases wear one and glide
once a jump starts it.

- **First move after standing in powder snow** (2). After several still ticks inside powder snow,
  vanilla's first tick that moves (a jump in case 743, a sprint step in case 301)
  is not slowed: it rises the plain 0.42, or walks 0.12, and keeps its velocity, and the stuck
  multiplier only applies from the next tick. Ours applies `PowderSnowBlock.entityInside`'s
  `(0.9, 1.5, 0.9)` on that first tick too and zeroes the velocity, so every later tick drifts
  (case 301: z 2.618 in vanilla, 2.606 in ours). Not pinned down further; the likely seam is
  `Entity.applyEffectsFromBlocks` checking a zero-length movement after still ticks.
