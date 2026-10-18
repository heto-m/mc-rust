# Proposal

## Why

The end goal is a launcher like the official one, where the player picks a game version and an installation and
presses Play. mcr can only be configured through CLI flags today (`--game-dir`, `--mods`, `--name`), it has no
saved installations and no account management, and the Microsoft login in `mcr-auth` has no UI.

## What Changes

- **Launcher view** inside the `mcr` binary, drawn with mcr's own wgpu UI (`mcr-ui`), shown before the title screen
  in an interactive run: profile list, account picker, a big Play button, no news.
- **Profiles (installations)**: name, icon from a fixed set, game version, game dir, mods folder or "no mods",
  JVM heap, resolution, last played. Create, edit, duplicate, delete (with confirmation). A default
  "Latest release" profile always exists. Stored in `<data dir>/launcher_profiles.json`, written atomically and
  bounded on read.
- **Version list** from Mojang's version manifest (fetched through `mcr-singleplayer`, cached for offline start):
  releases, snapshots behind a toggle; versions mcr cannot play are greyed out with "not supported yet". The
  supported set lives in `mcr-protocol`'s version layer (26.3 today).
- **Accounts**: offline names validated like vanilla; Microsoft accounts through the existing device-code flow with
  the code and URL shown in the UI, cancellable polling, the refresh token kept in `mcr-auth`'s 0600 account file,
  refresh at launch, sign out.
- **Play** applies the profile (game dir, version jar, mods folder → mod mode, heap, resolution, account) and
  continues into the existing title screen or the JVM boot.
- **CLI**: `--connect`, `--singleplayer`, `--mods` (and offscreen/world runs) skip the launcher as before;
  `--launcher` forces it, `--profile <name>` launches a profile directly, `--data-dir <dir>` moves the launcher
  files.
- Online auth reaches the client: a Microsoft account's token and UUID go into `ClientConfig.online`.

## Out of scope

Downloading client jars or assets for other versions, version translators, news, skins, multiple simultaneous
game instances, switching mod sets without a restart (profiles are only switchable in the launcher before the
first game start), a "Back to launcher" from the title screen, Java argument editing beyond the heap, and a live
Microsoft login in automated tests.

## Capabilities

### New Capabilities
- `launcher`: profiles store, version list, launcher accounts, the launcher view and its CLI.

### Modified Capabilities
- `account-auth`: cancellable device sign-in and a non-interactive refresh for launch.
- `app-screens`: the interactive app starts on the launcher instead of the title screen.

## Impact

New crate `mcr-launcher`; `mcr-auth`, `mcr-protocol` (version constants), `mcr-singleplayer` (manifest fetch),
`mcr-ui` (launcher screens), `mcr-app` (launcher scene, CLI, online auth wiring). No bridge ABI change.
