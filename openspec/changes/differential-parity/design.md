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
