# launcher Specification

## Purpose
The launcher before the title screen: profiles (installations), the version list, offline and Microsoft accounts, Play, and the command-line flags that skip or force it.

## Requirements

### Requirement: Launcher profiles
The launcher SHALL keep profiles with a name, an icon from a fixed set, a game version (latest release or a version id), a game dir (default `<data dir>/client`), a mods folder or none (vanilla), a JVM heap, an optional resolution and the last played time, in `<data dir>/launcher_profiles.json`. It SHALL offer create, edit, duplicate and delete, ask for confirmation before deleting, and always keep a default "Latest release" profile that cannot be deleted.

#### Scenario: Create and select
- **WHEN** the user creates a profile named "Survival" and selects it
- **THEN** the file lists it and it is still selected after a restart

#### Scenario: Delete needs confirmation
- **WHEN** the user clicks Delete on a profile
- **THEN** a confirmation screen opens and the profile is removed only after confirming

#### Scenario: Default profile
- **WHEN** the file is missing or lists no "latest-release" profile
- **THEN** the launcher shows a "Latest release" profile with the default game dir and no mods

### Requirement: Bounded launcher file
The profiles file SHALL be written through a synced temp file and a rename, and SHALL be read as hostile input: at most 1 MiB, at most 64 profiles and 32 accounts, names cut at 64 characters, paths at 1024 bytes, control characters removed, unknown icons replaced, heap and resolution clamped, invalid account names and duplicate ids handled. A file that is too large or not JSON SHALL leave the launcher on defaults and be kept aside as `.bad` before anything is saved.

#### Scenario: Oversized and hostile file
- **WHEN** the file holds 10,000 profiles with 1 MB names
- **THEN** the launcher loads at most 64 profiles with names of at most 64 characters

#### Scenario: Garbage file
- **WHEN** the file is not JSON
- **THEN** the launcher shows the default profile and the original is kept as `launcher_profiles.json.bad`

### Requirement: Version list
The launcher SHALL list versions from Mojang's version manifest, fetched in the background and cached at `<data dir>/cache/version_manifest_v2.json` for offline starts, showing releases and, behind a toggle, snapshots. Versions not in `mcr_protocol::version::SUPPORTED_VERSIONS` SHALL be greyed out with "not supported yet" and SHALL NOT be selectable. The manifest SHALL be capped at 8 MiB and 4096 versions with validated ids. "Latest release" SHALL resolve to the newest supported release.

#### Scenario: Unsupported version
- **WHEN** the manifest lists 26.1.2, 26.2 and 26.3
- **THEN** 26.3 and 26.2 can be chosen and 26.1.2 is greyed with "not supported yet"

#### Scenario: Offline start
- **WHEN** the network is down and a cached manifest exists
- **THEN** the version list comes from the cache

#### Scenario: Snapshots hidden
- **WHEN** the snapshot toggle is off
- **THEN** no snapshot is listed

### Requirement: Canonical assets for every version
Every supported version SHALL load its assets from the canonical version's client jar.

#### Scenario: Older version's assets
- **WHEN** a profile plays 26.2
- **THEN** its assets come from the 26.3 client jar

### Requirement: Launcher accounts
The launcher SHALL manage offline accounts, whose names are validated like vanilla, and Microsoft accounts, let the user pick the active account, and sign an account out after a confirmation.

#### Scenario: Invalid offline name
- **WHEN** the user enters `bad name!` as an offline account
- **THEN** Add stays inactive with the reason shown

#### Scenario: Sign out
- **WHEN** the user signs out a Microsoft account and confirms
- **THEN** its entry and its account file are removed

### Requirement: Microsoft sign-in
Microsoft sign-in SHALL use the device-code flow with the code and URL shown on screen, polling until approved, cancelled, declined or expired. Tokens SHALL be stored only in the account's 0600 file, never in the profiles file or logs.

#### Scenario: Device code shown
- **WHEN** the user starts a Microsoft sign-in and the device code request succeeds
- **THEN** the screen shows the user code and verification URL with Open link and Copy code

#### Scenario: Cancel sign-in
- **WHEN** the user cancels while polling
- **THEN** polling stops at its next wait and no account is added

### Requirement: Account refresh at launch
At launch a Microsoft account SHALL be refreshed without prompting and its token and UUID SHALL be used for online-mode servers; a rejected refresh SHALL ask the user to sign in again. The launch SHALL also pass the account's profile keys, Mojang's services keys and a key refresher to the client; failing to fetch keys or services keys SHALL NOT fail the launch. Offline accounts SHALL pass none.

#### Scenario: Refresh at launch
- **WHEN** Play is pressed with a Microsoft account whose Minecraft token expired
- **THEN** the token is refreshed from the refresh token and the game joins online-mode servers with it

#### Scenario: Keys reach the client
- **WHEN** Play is pressed with a Microsoft account whose certificates are fetched
- **THEN** the client config holds those keys and the services keys, and their debug output hides the private key

### Requirement: Launcher view
The launcher view SHALL show the profile list, the active account and a Play button, drawn with mcr's UI. Play SHALL apply the selected profile (game dir, version jar, mods folder or vanilla, heap, resolution, account), record its last played time, and continue to the title screen, or in mod mode to Java's boot. Profiles SHALL be switched only in the launcher; the title screen SHALL lead back to it.

#### Scenario: Play a vanilla profile
- **WHEN** the user selects a profile without mods and clicks Play
- **THEN** the title screen shows and Java is not started, even if the game dir's mods folder has enabled mods

#### Scenario: Play a mod profile
- **WHEN** the selected profile names a mods folder with enabled mods and the user clicks Play
- **THEN** Java boots with those mods and its title screen is shown

#### Scenario: Unsupported profile
- **WHEN** the selected profile names a version mcr does not support
- **THEN** Play is inactive and the profile shows "not supported yet"

### Requirement: Launcher command line
`--connect`, `--singleplayer`, `--mods`, `--world`, `--synthetic`, `--soak` and offscreen `--screenshot` SHALL skip the launcher. `--launcher` SHALL force it (also offscreen and with `--connect`, joining after Play), `--profile <name>` SHALL apply that profile and start without the launcher, and `--data-dir <dir>` SHALL move the launcher files. Explicit `--game-dir`, `--mods`, `--name`, `--jar` and `--size` SHALL override the profile.

#### Scenario: Direct profile
- **WHEN** `mcr --profile Survival` is run
- **THEN** the Survival profile's game dir and account are used and the title screen shows without the launcher

#### Scenario: Unknown profile
- **WHEN** `--profile Nope` names no profile
- **THEN** mcr exits with a usage error listing the profile names

#### Scenario: Scripted launcher
- **WHEN** `--launcher --connect host --autopilot s.txt` runs and the script clicks Play
- **THEN** the launcher's account name joins `host`, and the remaining steps run in the game

### Requirement: Back to the launcher
The native title screen SHALL have a Launcher button, and in mod mode the "mcr" menu SHALL offer Launcher while no world is open. Without a JVM in the process, mcr SHALL return to the launcher view in the same process. When a JVM was started, mcr SHALL shut Java down cleanly and start itself again with `--launcher` and the same `--data-dir`, keeping an offscreen run offscreen.

#### Scenario: Native round trip
- **WHEN** the user clicks Launcher on the native title screen
- **THEN** the launcher view shows in the same process and Play starts the selected profile again

#### Scenario: Mod mode
- **WHEN** the user picks Launcher in the "mcr" menu on Java's title screen
- **THEN** Java shuts down cleanly and a new mcr process shows the launcher with the same data dir
