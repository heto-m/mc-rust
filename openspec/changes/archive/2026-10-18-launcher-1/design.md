# Design

## Shape

```
mcr-launcher (no GPU, no UI)        mcr-ui                        mcr-app
  store: profiles + accounts JSON     LauncherScreen               launcher::Launcher (scene)
  versions: manifest parse/cache      ProfileEditScreen              owns Renderer + Gui + ScreenStack
  accounts: offline + Microsoft       VersionSelectScreen          launcher::LauncherApp
  launch: profile -> LaunchPlan       AccountsScreen, sign-in        implements LauncherModel
                                      LauncherModel trait          main: Scene::Launcher -> Shell
```

`mcr-launcher` holds every rule that can be unit tested without a GPU: parsing and bounding the files, the
version list, account bookkeeping, and turning a profile into a `LaunchPlan`. `mcr-ui` gets the screens, which
talk to the app through a `LauncherModel` trait object (the same pattern as `ModsModel`). `mcr-app` implements the
model, runs the background work (manifest fetch, Microsoft sign-in and refresh) on threads with channels, and
hosts the screens in a small launcher scene.

## The launcher is its own scene, before the Shell

Mod mode decides before the renderer exists: `mods::startup` plans the JVM boot and the renderer is created with
`scene_texture` and the bridge's optional features, then Java boots and only then the `Shell` loads the game
assets. A profile picks the mods folder, so the choice must come before all of that. The launcher therefore runs
as `Scene::Launcher` with its own `Renderer` (small arenas) and a `Gui` loaded from the client jar, nothing else:
no world assets, audio or Java. Play produces a `LaunchPlan`; the app applies it to `Options`, drops the
launcher (and its renderer, so its surface is released), then runs the existing path unchanged:
`mods::startup` → renderer → `BootPlan::start` → `Shell`. The offscreen path (`screenshot::run_shell`) does the
same in a loop before `run_shell`'s body.

Consequences, documented for the user:

- Profiles are switched only in the launcher, before the first game start. Changing mod sets needs a JVM
  restart, so there is no "back to launcher" from the title screen in this change; quitting and starting `mcr`
  again shows the launcher.
- The launcher uses the 26.3 client jar for fonts and widgets (`--jar`, default `.cache/versions/26.3/client.jar`).
  Downloading jars for other versions is future work.

## When the launcher shows

| Run | Launcher |
|---|---|
| `mcr` (windowed, no session flags) | shown |
| `--launcher` | shown, also offscreen with `--screenshot` and together with `--connect` (Play then joins) |
| `--connect`, `--singleplayer`, `--mods` | skipped |
| `--profile <name>` | skipped: the profile is applied directly |
| `--world`, `--synthetic`, `--soak`, offscreen `--screenshot` without `--launcher` | skipped |

`--launcher` together with `--world`, `--synthetic`, `--soak` or `--profile` is a usage error. Flags given
explicitly still win over the profile: `--game-dir`, `--mods`, `--name`, `--jar`, `--size`. That keeps every
existing script working, and lets autopilot runs drive the launcher with `--launcher`.

## Files

Data dir: `--data-dir`, default `run` (relative to the working directory like the game dir). The default game dir
is `<data dir>/client`, which is today's `run/client`, so the default profile reuses existing worlds and options.

- `<data dir>/launcher_profiles.json` — profiles, selected profile, account list (no secrets), active account,
  snapshot toggle. User-editable.
- `<data dir>/accounts/<uuid>.json` — one `mcr_auth::Account` per Microsoft account: refresh token, Minecraft
  token, expiry, profile, keys. Written by `Account::save` (temp file, mode 0600, fsync, rename).
- `<data dir>/cache/version_manifest_v2.json` — the last manifest fetched.

Writes go to a sibling temp file, are synced and renamed. Reading the profiles file is hostile-input parsing:

- the file is read only up to 1 MiB; anything larger, not JSON or not an object is set aside as
  `launcher_profiles.json.bad` on open and the launcher starts with defaults;
- at most 64 profiles and 32 accounts; extra entries are dropped;
- names are cut at 64 characters, paths at 1024 bytes, version ids at 64, control characters removed;
- the icon must be in the fixed set, otherwise the default icon; heap clamped to the Mods screen's 1024..=4096 MiB; resolution
  each side 320..=7680 or none; ids must be 1..=32 ASCII alphanumerics/`-`, duplicates are re-numbered;
- offline account names must pass `mcr_auth::offline::validate_name`; Microsoft entries need a 32-hex UUID and a
  valid name; others are dropped.

The manifest is capped at 8 MiB (the existing singleplayer limit), at most 4096 versions are kept, ids must be
1..=64 characters of `[A-Za-z0-9._+ -]`, and unknown types are dropped. A cached manifest is used at once; a
fresh one is fetched in the background on every launcher start and replaces it only if it parses.

## Profiles

```
Profile { id, name, icon, version: Latest | Id(String), game_dir: Option<path>, mods: Option<path>,
          heap_mib, resolution: Option<(w, h)>, last_played: unix seconds }
```

- `game_dir: None` means the default `<data dir>/client`. Relative paths are taken from the working directory.
- `mods: None` is vanilla: mod mode stays off even if the game dir's `mods` folder has enabled mods. A folder
  turns on mod mode with that folder, like `--mods`.
- The default profile has id `latest-release`, version `Latest`, the default game dir and no mods. It can be
  edited but not deleted, and it is recreated if missing.
- `Latest` resolves to the newest **release** in the manifest that mcr supports, falling back to the newest
  supported version without a manifest. The launcher notes when a newer release exists that mcr cannot play.
- Icons are item ids from a fixed list (grass block, crafting table, furnace, ...) drawn with the item renderer.
- Duplicate copies everything but the id and last played, naming the copy "<name> (copy)".

## Per profile vs. global settings

Kept simple: a profile chooses **where** the game keeps its settings; it does not hold game options itself.

- **Per profile**: version, game dir (and through it `options.txt`, key bindings, `servers.dat`, worlds,
  resource packs, mod enable state), mods folder, JVM heap, window resolution.
- **Global** (launcher file): accounts and the active account, selected profile, snapshot toggle.

Two profiles that share a game dir therefore share their options, like the official launcher's default. The heap
in a profile overrides the Mods screen's heap setting for that launch only.

## Versions

`mcr_protocol::version::{GAME_VERSION, SUPPORTED_VERSIONS}` is the single source of the playable set; the
translators of later changes add to it. The launcher lists releases, plus snapshots when "Show snapshots" is on;
old alpha and beta are not listed. Unsupported rows are greyed with "not supported yet" and cannot be chosen. A
profile file naming an unsupported version shows the profile greyed and Play inactive. The version picks the
client jar `.cache/versions/<id>/client.jar` unless `--jar` was given.

## Accounts

- **Offline**: name checked with `validate_name` (1..=16 printable ASCII without spaces, as vanilla).
- **Microsoft**: `MCR_MSA_CLIENT_ID` is required (no default is embedded, as `account-auth` requires); without it
  the button explains that. Sign-in runs on a thread: `request_device_code` → the screen shows the code and the
  URL with Open link and Copy code → `poll_for_tokens` with a wait callback that returns `false` once the user
  cancels (new `AuthError::Cancelled`) → Xbox/XSTS/Minecraft chain, entitlements, profile, keys → saved to
  `accounts/<uuid>.json`. Only the profile name and UUID go into the launcher file.
- **Refresh at launch**: `Authenticator::refresh_cached` never prompts: a valid token is used, an expired one is
  refreshed with the refresh token, a rejected refresh fails with "sign in again". Play runs it on a thread and
  shows "Signing in…"; the plan carries `OnlineAuth { access_token, profile_id }`, which goes through
  `ShellConfig` into every `ClientConfig`. `--profile` refreshes synchronously before the window opens.
- **Sign out** removes the account file and its entry, after a confirmation.
- Tokens are `Secret` (Debug prints `***`); nothing logs a token, a device code is shown only on the screen.
- Tests use a scripted `HttpClient` inside `mcr-launcher`; no test talks to Microsoft.

## Autopilot

One `--autopilot` script drives both: steps run against the launcher until Play has produced a plan, then the
remaining steps go to the Shell. Only UI steps (`click`, `type`, `ui`, `tap`, `hover`, `wheel`, `wait`,
`sleep`, `shot`, `quit`) run in the launcher; any other step waits for the Shell.

## Screens

`LauncherScreen` (profile list with item icons, New / Edit / Copy / Delete, the active account's name as the
Accounts button, a wide Play, Quit, and a status line for "Signing in…", errors and version-list notices),
`ProfileEditScreen` (two columns: name, icon cycled by left/right click, version, resolution, game dir, mods
folder, heap slider), `VersionSelectScreen` (Latest release first, snapshot toggle), `AccountsScreen`,
`OfflineAccountScreen` and `SignInScreen`. Delete and Sign out go through `ConfirmScreen`. Double-clicking a
playable profile plays it.

## Risks

- Two renderers are created per run when the launcher shows (the launcher's, then the game's). The launcher's
  uses small arenas and is dropped first; startup cost is a device creation (well under a second).
- A user-edited path in a profile is used as given; it only ever names the user's own folders.
