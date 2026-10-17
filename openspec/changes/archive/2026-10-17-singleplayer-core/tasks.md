## 1. Library

- [x] 1.1 `mcr-singleplayer` crate: layout, folder names, level.dat summary and rename, world scan, icon decode, with tests on synthetic NBT
- [x] 1.2 `server.properties`, `ops.json` and launch arguments from creation settings or level.dat, with tests
- [x] 1.3 Provisioning: manifest, version JSON, server jar download with SHA-1 and size checks, progress, with parser tests
- [x] 1.4 Java: `HomeSearch` reuse and Mojang `java-runtime` install (path checks, SHA-1, bounds), with manifest tests
- [x] 1.5 `ManagedServer`: supervisor thread, bounded output reader, ready detection, stop/kill timeouts, pdeathsig, memory scope; tests with a fake shell-script server (ready, never ready, ignores stop, crashes)
- [x] 1.6 Stale servers: pid file and `session.lock` owner check, with tests

## 2. Screens

- [x] 2.1 World list, edit and create world screens with `WorldStore`, `PlayWorld` and `CreateWorld` actions; behaviour tests
- [x] 2.2 Title screen Singleplayer button; pause screen local variant and `ScreenStack::any_pauses_game`

## 3. App

- [x] 3.1 Shell singleplayer session: EULA prompt, provisioning thread, progress screen, join on ready, failure screens
- [x] 3.2 Pause freeze/unfreeze, Save and Quit flow with saving screen, server exit during play, drop on close
- [x] 3.3 `--singleplayer <folder>` and autopilot `wait_idle`

## 4. Verify

- [x] 4.1 Live: create a world, play, pause (tick state in the server log), save and quit, reopen, delete a throwaway world
- [x] 4.2 fmt, clippy, tests; archive
