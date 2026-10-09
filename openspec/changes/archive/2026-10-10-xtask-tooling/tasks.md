# Tasks

## 1. Scaffolding and layout

- [x] 1.1 Add `clap` (derive), `anyhow`, `ureq` (rustls), `serde_json`, `sha1`, `flate2`, `tar`, `zip` to `xtask/Cargo.toml` and verify `cargo check -p xtask` succeeds
- [x] 1.2 Implement repo-root and `.cache/` path layout helpers and unit-test that version and tool paths match the specified layout
- [x] 1.3 Implement the clap command tree (`fetch`, `jdk`, `datagen`, `decompile`, `server`, `setup`) with `--version`, `--port` and `--force` and verify `cargo xtask --help` lists every command

## 2. Fetch

- [x] 2.1 Implement piston-meta manifest parsing and latest-release lookup, unit-tested with a small JSON fixture including an unknown-id case
- [x] 2.2 Implement SHA-1 hex digest and verification, unit-tested with a known vector and a mismatch
- [x] 2.3 Implement the `fetch` command with skip-if-verified, `.part` downloads and checksum failure, verified by the helpers' unit tests (no network in tests)

## 3. JDK

- [x] 3.1 Implement system Java major-version detection from `java -version` output, unit-tested with sample outputs
- [x] 3.2 Implement the Adoptium URL builder per OS/arch and archive extraction with top-level stripping, unit-tested with generated in-memory tar.gz and zip fixtures
- [x] 3.3 Implement the `jdk` command and verify it skips download when a suitable system Java exists

## 4. Datagen and decompile

- [x] 4.1 Implement the `datagen` command (bundler main class, arguments, cwd, `datagen.log`, exit-code check) and unit-test the constructed command line
- [x] 4.2 Implement the `decompile` command (Vineflower download-if-absent, `-Xmx6g` and `--only` arguments) and unit-test the constructed command line

## 5. Server

- [x] 5.1 Implement `server.properties` rendering (online-mode, port, view distance) and unit-test the output lines
- [x] 5.2 Implement EULA handling (`MCR_ACCEPT_EULA`, `eula.txt`, exit message) and unit-test both paths with a temp directory
- [x] 5.3 Implement the `server` command with stdin forwarding and `--nogui`, and verify it exits with the EULA message when unaccepted

## 6. Setup and integration

- [x] 6.1 Implement `setup` as the ordered pipeline that stops at the first failure, and unit-test the step order with an injected step runner
- [x] 6.2 Run `cargo xtask --help` and `cargo xtask server` without `MCR_ACCEPT_EULA` to confirm the user-facing messages

## Workflow follow-up

- Run `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- Archive the change with `openspec archive xtask-tooling -y` and run `openspec validate --all --strict --no-interactive`.
