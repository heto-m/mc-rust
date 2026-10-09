## 1. Shared types

- [x] 1.1 Text component model with NBT codec and lenient decode; `cargo test -p mcr-protocol text` passes.
- [x] 1.2 `GameProfile`, `KnownPack`, registry entries and tag payload; round-trip tests pass.

## 2. Packets

- [x] 2.1 Handshake and status packets with byte fixtures; tests pass.
- [x] 2.2 Login packets with round-trip tests.
- [x] 2.3 Common packets shared by configuration and play with round-trip tests.
- [x] 2.4 Configuration packets with round-trip tests.

## 3. Verification

- [x] 3.1 `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass.
