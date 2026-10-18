# Tasks

## 1. Foundations

- [x] 1.1 `mcr_protocol::version`: `GAME_VERSION`, `SUPPORTED_VERSIONS`, `is_supported`; test
- [x] 1.2 `mcr_singleplayer::provision::fetch_manifest` (existing size cap and HTTPS checks)
- [x] 1.3 `mcr-auth`: cancellable device sign-in (`AuthError::Cancelled`) and `refresh_cached`; mock HTTP tests

## 2. `mcr-launcher` crate

- [x] 2.1 Profile model, icon set, defaults, duplicate; tests
- [x] 2.2 Launcher file: bounded load (size, counts, strings, ids, icons, heap, resolution, names), `.bad` keep-aside, atomic save; hostile-input tests
- [x] 2.3 Version list: bounded manifest parse, release/snapshot filter, supported flag, latest supported release, cache read/write; tests
- [x] 2.4 Accounts: offline add with validation, Microsoft sign-in worker (device code event, cancel, save 0600), refresh at launch, sign out; scripted-HTTP tests (no network)
- [x] 2.5 `LaunchPlan` from a profile, version and account; tests for vanilla, mods, unsupported, explicit overrides

## 3. UI (`mcr-ui`)

- [x] 3.1 `LauncherModel` trait and row types
- [x] 3.2 `LauncherScreen`: profile list with icons, New/Edit/Copy/Delete (confirmation), account button, Play, Quit, status line
- [x] 3.3 `ProfileEditScreen`: name, icon, version, game dir, mods folder, heap, resolution
- [x] 3.4 `VersionSelectScreen` with the snapshot toggle and greyed unsupported rows
- [x] 3.5 `AccountsScreen`, offline name screen, Microsoft sign-in screen (code, URL, Open link, Copy code, Cancel)
- [x] 3.6 Screen unit tests with a fake model (selection, delete confirmation, disabled Play, invalid name)

## 4. App (`mcr-app`)

- [x] 4.1 CLI: `--launcher`, `--profile`, `--data-dir`, explicit-flag tracking; parse tests
- [x] 4.2 `LauncherApp` model over `mcr-launcher` with background manifest fetch, sign-in and refresh threads
- [x] 4.3 Launcher scene (windowed) and the hand-off to `mods::startup` → renderer → Shell
- [x] 4.4 Offscreen launcher loop in `run_shell` with the shared autopilot
- [x] 4.5 `--profile` direct launch; online auth through `ShellConfig` into `ClientConfig`; heap and vanilla flags into mod startup

## 5. Verification

- [x] 5.1 `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`
- [x] 5.2 Live offscreen: launcher renders; create a profile, pick it, Play into the title screen
- [x] 5.3 Live offscreen: a mod profile with `.cache/play-mods` reaches Java's title screen
- [x] 5.4 Live: an offline account name joins a vanilla server on port 25571, with `--connect`, through the launcher and with `--profile`
- [x] 5.5 Archive the change and fill any TBD purpose
