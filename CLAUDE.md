# mc-rust

From-scratch Rust client for Minecraft: Java Edition. Joins vanilla servers; built for performance with
GPU-driven rendering on wgpu and a custom engine (no Bevy, no OpenGL).

## Fixed decisions

- **Milestone 1: full functionality against a vanilla server** (offline mode). Singleplayer and
  everything else come after it.

- First target: Minecraft **26.3** (protocol from the vanilla data generator). Older versions come later
  as translators onto the latest canonical packet set (ViaVersion-style), so never hard-code
  version-specific behavior outside `mcr-protocol`'s version layer.
- Renderer: wgpu. Drop to raw Vulkan only through wgpu-hal interop, never a second backend.
- Singleplayer: the vanilla server jar runs as a managed child process (offline mode, loopback,
  `/tick freeze` for pause, vanilla world format). Client code must not assume a remote server.
- Networking: dedicated I/O threads with blocking sockets + channels to the game thread. No tokio.
- **Never commit Mojang assets, jars, or decompiled code.** Assets load at runtime from the user's own
  client jar. Decompiled sources under `.cache/` are reference only; reimplement behavior, don't
  transliterate it.

## Layout

| Path | Purpose |
|---|---|
| `crates/mcr-buf` | `Encode`/`Decode<'a>` traits, VarInt, primitives (zero-copy decode) |
| `crates/mcr-*` | One crate per subsystem (protocol, net, auth, nbt, world, assets, render, client) |
| `crates/mcr-app` | The `mcr` binary |
| `java/` | Our Java sources for the embedded JVM (no Mojang code), built by `cargo xtask java-build` into `.cache/java/` |
| `xtask` | Dev tooling: `cargo xtask <cmd>` (download jars/JDK, data generator, codegen, decompile) |
| `.cache/` | Gitignored downloads and generated reference data |
| `openspec/` | Specs and change proposals |

## Workflow

- Every feature is an OpenSpec change (`openspec/changes/<name>/`: proposal, design, specs delta,
  tasks). Validate with `openspec validate <name> --strict`; archive it (`openspec archive <name> -y`)
  on the branch before the PR merges.
- Branch per change (`feat/<name>`, `fix/<name>`, `chore/<name>`), merged locally into `main`. Local
  only for now: no pushes, PRs or CI work until the client runs.
- Commits: Conventional Commits, short subject (`feat(protocol): add login packets`), no body unless
  needed.
- Before pushing: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings &&
  cargo test --workspace`.
- Run cargo builds and tests inside a memory-capped scope (`systemd-run --user --scope -q -p MemoryMax=8G
  -p MemorySwapMax=0 cargo …`): systemd-oomd otherwise kills the whole terminal, coordinating session
  included. Hostile-input test fixtures must stay small — `vec![tree; 64]` nested 12 deep is 64^12 nodes.
- Live checks: send `op <name>` only after the player has joined; before that the vanilla server looks the name up online and stores the wrong UUID.
- Long-running processes (vanilla server, client runs) from agent shells: start them with the Bash tool's
  `run_in_background`, or fully detached (`setsid nohup … > log 2>&1 &`), never inheriting the tool's
  pipes — otherwise the tool call hangs until the process exits. Never write to a FIFO without a
  reader; feed server consoles via a regular file + `tail -f`. Stop servers when done.
- `Cargo.lock` conflicts on rebase: take `main`'s copy, then run `cargo check` to regenerate.
- Add dependencies to the crate's own `Cargo.toml`; shared ones go in `[workspace.dependencies]`.

## Code standards

- Performance is a feature: avoid per-packet/per-block allocation in hot paths, prefer flat arrays and
  borrowed decoding, measure with `criterion` benches before optimizing.
- Everything from the server is hostile input: bound every loop, allocation, recursion and expansion
  driven by packet data (lengths, counts, nesting, health/absorption, text components).
- `unsafe` only with a `// SAFETY:` line and a benchmark proving it pays.
- Errors: `thiserror` in libraries, `anyhow` only in binaries and xtask.
- Tests: unit tests next to code; protocol codecs get round-trip tests plus byte fixtures captured
  from the real vanilla server/client.
