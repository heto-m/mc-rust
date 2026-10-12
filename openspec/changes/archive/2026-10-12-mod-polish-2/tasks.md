## 1. ABI

- [x] 1.1 `BRIDGE_VERSION` 9: `FrameBegin` render distance and entity scale, `MSG_KEYS`, `MSG_PACKS`, `REPORT_KEYS`, with table tests

## 2. Items

- [x] 2.1 Key mappings sync both ways, with unit tests and a JVM test via the test mod
- [x] 2.2 Server packs loaded in Java from mcr's files, with a JVM test
- [x] 2.3 Render distance and entity scale drive Java's fog, with a JVM test
- [x] 2.4 Keep vanilla particles mods spawn outside mirrored scopes, with a live test via the test mod
- [x] 2.5 Full-screen backdrop behind the native menu panel, with a test
- [x] 2.6 Seed Caps Lock and Num Lock from the keyboard LEDs on Linux, with a test
- [x] 2.7 Quiet log for requests the proxy refuses, with a JVM test via the test mod

## 3. Finish

- [x] 3.1 Update `docs/mod-bridge.md` and `docs/mod-compat.md`
- [x] 3.2 Full gate: fmt, clippy, test, java-build, JVM tests
