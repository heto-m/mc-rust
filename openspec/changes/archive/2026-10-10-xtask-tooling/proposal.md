# Proposal

## Why

The client needs the vanilla Minecraft 26.3 jars, a portable JDK, the data generator output and decompiled
sources as reference material. Today these are set up by hand on one machine, which makes the workflow
irreproducible. `xtask` is the single entry point for these steps and must produce the exact `.cache/` layout
the developers already use.

## What Changes

- Turn `xtask` into a `clap`-based dev CLI invoked as `cargo xtask <command>`.
- `fetch [--version]`: download `version.json`, `client.jar` and `server.jar` from piston-meta, verifying SHA-1.
- `jdk`: provide a Java 25 runtime (system `java` if >= 25, otherwise a portable Eclipse Temurin JDK).
- `datagen [--version]`: run the vanilla data generator in the `datagen/` working directory.
- `decompile [--version]`: run Vineflower 1.12.0 over the client jar.
- `server [--version] [--port]`: run the vanilla server in `run/server-<ver>/` with EULA handling.
- `setup`: `fetch`, `jdk`, `datagen`, `decompile` in order.
- Every command is idempotent and accepts `--force` to redo its work.
- All outputs live under `<repo>/.cache/` (gitignored); server runs live under `<repo>/run/` (gitignored).

## Capabilities

### New Capabilities
- `dev-tooling`: The `cargo xtask` developer commands that fetch, verify, generate and run the reference
  Minecraft artifacts under `.cache/` and `run/`.

### Modified Capabilities

## Impact

- `xtask/` only: new dependencies (`clap`, `anyhow`, `ureq` with rustls, `serde_json`, `sha1`, `flate2`, `tar`, `zip`).
- Network access is required only for the commands that download; unit tests make no network calls.
- Requires a Java runtime at runtime for `datagen`, `decompile` and `server`.

Out of scope: codegen from generated reports (separate change), Fabric/Forge or snapshot-specific handling,
Windows-specific server launch flags, and any committed binary or Mojang file.
