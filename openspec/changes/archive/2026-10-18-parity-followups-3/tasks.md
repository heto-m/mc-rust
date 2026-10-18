## 1. Change

- [x] 1.1 Proposal, design, specs and tasks; `openspec validate parity-followups-3 --strict`

## 2. Built-in registry entries and chat decorations

- [x] 2.1 `mcr-nbt` JSON to NBT (typed lists, depth limit) and lenient numeric getters; unit tests
- [x] 2.2 `Registries` builtin provider: fill entries without data under the byte budgets, lookup cap, id checks; unit tests
- [x] 2.3 App provider from the client jar for every registry; drop per-consumer fallbacks; biome colours from registry NBT
- [x] 2.4 Chat decoration key, parameters and style from the entry; unit test against the real jar data
- [x] 2.5 Live: `/me`, `/msg`, `/say`, `/teammsg` against a vanilla server

## 2b. Secure chat trust

- [x] 2b.1 `TextRenderer` from mcr-ui's flattening and the GUI's shared language; `plain_with_args` removed
- [x] 2b.2 Fail closed without a renderer; unsigned font rule with Only Show Secure Chat; local server by port
- [x] 2b.3 Tests: no-slot template, skipping `%2$s`, language over fallback, chat type without content, other localhost port and transfer

## 3. Edit Game Rules in mod mode

- [x] 3.1 Live check of the button and screen in a local world with and without cheats
- [x] 3.2 `set_game_rule` on the forward list; filter test
- [x] 3.3 Live: change a rule from Java's screen and read it back on the server

## 4. Back to the launcher

- [x] 4.1 `ScreenAction::Launcher`, native title button, "mcr" menu entry on the title
- [x] 4.2 In-process return without a JVM (windowed and offscreen)
- [x] 4.3 Re-exec with `--launcher --data-dir` after `ModHost::finish` when a JVM was started; argument unit test
- [x] 4.4 Offscreen checks of both paths; `docs/mod-bridge.md`

## 5. net_bridge

- [x] 5.1 Run `MCR_JVM_TESTS=1` `net_bridge` against a vanilla server and fix failures

## 6. Finish

- [x] 6.1 Merge main, fmt, clippy, tests, archive
