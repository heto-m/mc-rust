## 1. ABI and frame state

- [x] 1.1 `FSTATE_PERSPECTIVE` and `FSTATE_VIEW_ROLL`, `BRIDGE_VERSION` 14, Java constants, decoding with unit tests

## 2. Java leads the perspective

- [x] 2.1 Java reports its camera type; the camera mixin stops forcing it and lets vanilla align on a switch frame
- [x] 2.2 `GameplayKeys` leaves the toggle-perspective key to Java while mcr's camera paces it
- [x] 2.3 mcr follows Java's perspective while Java draws the world and skips its own F5 then; autopilot report
- [x] 2.4 Test mod perspective cycle and live test

## 3. Mod pose roll

- [x] 3.1 Java diffs the level bob pose against vanilla's and reports the roll
- [x] 3.2 mcr rolls its view by it; test mod pose roll and live test

## 4. Real mods

- [x] 4.1 Shoulder Surfing (default config), Freelook and Camera Overhaul offscreen against the vanilla server

## 5. Finish

- [x] 5.1 Update `docs/mod-bridge.md` and `docs/mod-compat.md`
- [x] 5.2 Merge main, full gate: fmt, clippy, test, java-build, touched JVM tests
