## 1. ABI

- [x] 1.1 `BRIDGE_VERSION` 12: frame-state phase counters (`FSTATE_PROF`, `PROF_*`) and `D_LEVEL_VIEW`, with table tests

## 2. Measure

- [x] 2.1 Java frame phase times (`McrFrameTimes` and hooks), published with each frame
- [x] 2.2 Render-thread profile (`FrameProfiler`: period, wait, submit, Java phases, backend counters), plus downcall timing behind `MCR_MOD_PROFILE`, with unit tests
- [x] 2.3 Per-mod render-thread sampler (`McrProfiler`, JFR) behind `MCR_MOD_PROFILE`
- [x] 2.4 Shell bench: `--screenshot … --bench N` in the menu and connect flow prints fps and the profile
- [x] 2.5 Baseline numbers on the title screen and in game, without mods and with `.cache/play-mods`, recorded in design.md

## 3. Pipelining

- [x] 3.1 Frame-bounded, FIFO slot drains and frame-tagged queue, with ordering tests; no queue lock held while submitting
- [x] 3.2 Ahead and lockstep posting in `FrameLink`; late frames composed in the next mcr frame
- [x] 3.3 Late-frame level projection rewrite (`D_LEVEL_VIEW`, `level_projection`, patch before the first world slot), with a projection unit test
- [x] 3.4 `Pipelining` modes, `AheadPolicy` hysteresis, `MCR_MOD_PIPELINE`, the uncapped hint from the app, with unit tests
- [x] 3.5 JVM test: 600 unpaced frames ahead on the title screen, each composed once, GUI intact, ticks equal

## 4. Other costs

- [x] 4.1 Fence waits block only for readbacks; zero-timeout polls of the open recording skip the downcall; readback poll skips `device.poll` when idle
- [x] 4.2 One command buffer per submit point (solid+translucent, after+post-world); empty queued items skip `queue.submit`
- [x] 4.3 `MCR_BENCH_TURN` bench check that a frame composed ahead matches lockstep while the camera turns

## 5. Finish

- [x] 5.1 After numbers for no mods, play-mods on the title screen and play-mods in game; update `docs/mod-bridge.md`
- [x] 5.2 Merge main, full gate: fmt, clippy, test, java-build, JVM tests
