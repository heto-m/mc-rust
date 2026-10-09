# Design

## Context

See proposal.md for motivation. `xtask` is a binary-only workspace member with no dependencies yet. The
`.cargo/config.toml` alias `xtask = "run --package xtask --"` already exists. `.cache/` and `run/` are gitignored.

## Goals / Non-Goals

**Goals:**
- Reproduce the existing `.cache/` layout exactly.
- Keep every command idempotent and verifiable.
- Keep pure logic (manifest parsing, SHA-1 checks, path layout, properties rendering, command lines) unit-testable without network.

**Non-Goals:**
- Codegen from generated reports (separate change).
- Platform-specific server launch tuning beyond `java -jar`.

## Decisions

- **Blocking HTTP with `ureq` (rustls)**: matches the no-async stance; no tokio.
- **Repo root from `CARGO_MANIFEST_DIR`'s parent**: stable regardless of the cwd `cargo xtask` runs from.
- **Pure helpers split from I/O**: manifest lookup, SHA-1 hex, path layout, properties rendering and process
  argument construction are plain functions with unit tests. Command functions only wire them to the filesystem
  and child processes.
- **Verify-then-skip**: a file is reused only if its SHA-1 matches the hash declared in `version.json`.
  Alternative considered: existence-only skip, rejected because a truncated jar would be silently reused.
- **JDK via the Adoptium binary redirect**: `ureq` follows the redirect. Archives are extracted with `flate2`+`tar`
  (unix) or `zip` (windows), stripping the top directory. On macOS the `bin/java` lives under `Contents/Home`.
- **Extract into a sibling temp directory, then rename**: a failed extraction never leaves a half-populated `jdk25/`.
- **Vineflower by existence**: downloaded once into `.cache/tools/`. No pinned hash, since the version is pinned by URL.
- **EULA via environment variable**: non-interactive; the EULA is accepted only by an explicit `MCR_ACCEPT_EULA=1`.

## Risks / Trade-offs

- [Piston-meta or Adoptium response shape changes] → parsing is isolated and covered by fixture tests; errors name the URL.
- [Interrupted large download] → downloads go to a `.part` file and are renamed only after verification.
- [Vineflower needs about 6 GB heap] → fixed `-Xmx6g` as specified; a failing exit status is reported as-is.
