# Real Minecraft vs mcr: modpack benchmark

Measured 2026-10-18 on a Ryzen 7 5800X, RX 6800 XT, 15 GiB RAM and Linux (Mesa 26.2.4). The clients were real
Minecraft 26.3 with Fabric Loader 0.19.5, launched by Prism Launcher 11.1.1, and mcr built from the modpacks-2 branch
(85f9c4a). Each run joined the same Fabric server (`measure_pack.py serve`) on loopback.

## Results

All runs are at 2560x1440 with render distance 12 (server view distance 12, simulation distance 10), vsync off and
fps uncapped. The player stands still at the same spot and rotation. Frame times cover the 60 s that start 30 s after
the server logged the join. Each cell lists every run, separated by `/`.

**Echoes of the Sift.** 19 jars in the full pack; mcr loads 12.

| | Real MC, full pack | Real MC, mcr's mod set | mcr windowed | mcr offscreen |
|---|---|---|---|---|
| Title (s) | 12.4 / 12.9 / 12.1 ¹ | 8.6 / 8.6 / 9.2 ¹ | not logged ² | 10.5 / 11.1 ³ |
| Joined (s) | 13.4 / 13.9 / 13.1 | 9.6 / 9.6 / 9.2 | 11.6 / 11.4 | 11.1 / 11.7 |
| fps (mean) | 1494 / 1520 / 1597 | 1669 / 1502 / 1875 | 1356 / 1341 | 2330 / 2387 ⁴ |
| Frame mean (ms) | 0.67 / 0.66 / 0.63 | 0.60 / 0.67 / 0.53 | 0.74 / 0.75 | 0.43 / 0.42 ⁴ |
| Frame p50 (ms) | 0.63 / 0.62 / 0.60 | 0.57 / 0.63 / 0.51 | 0.70 / 0.71 | n/a ⁴ |
| Frame p99 (ms) | 1.50 / 1.48 / 1.36 | 1.44 / 1.47 / 1.23 | 1.73 / 1.80 | n/a ⁴ |
| Frame max (ms) | 22.4 / 4.5 / 4.3 | 16.2 / 7.2 / 7.8 | 4.3 / 6.8 | n/a ⁴ |
| Peak RSS (MB) ⁵ | 3144 / 3015 / 3138 | 2865 / 2884 / 3061 | 3316 / 3378 | 3422 / 3435 |
| GPU | RX 6800 XT (radeonsi GL 4.6) | RX 6800 XT (radeonsi GL 4.6) | RX 6800 XT (RADV Vulkan) | RX 6800 XT (RADV Vulkan) |

**Medieval Immersed.** 113 jars in the full pack; mcr loads 102.

| | Real MC, full pack | Real MC, mcr's mod set | mcr windowed | mcr offscreen |
|---|---|---|---|---|
| Title (s) | 27.3 / 26.9 ¹ | 25.1 / 25.9 ¹ | not logged ² | 29.4 / 28.7 ³ |
| Joined (s) | 29.3 / 27.9 | 26.1 / 25.9 | 30.4 / 30.7 | 33.0 / 32.0 |
| fps (mean) | 432 / 573 | 730 / 644 | 588 / 609 | 553 / 581 ⁴ |
| Frame mean (ms) | 2.32 / 1.74 | 1.37 / 1.55 | 1.70 / 1.64 | 1.81 / 1.72 ⁴ |
| Frame p50 (ms) | 2.21 / 1.66 | 1.29 / 1.45 | 1.57 / 1.49 | n/a ⁴ |
| Frame p99 (ms) | 3.90 / 3.10 | 3.01 / 3.02 | 4.75 / 4.74 | n/a ⁴ |
| Frame max (ms) | 27.8 / 32.7 | 131.5 / 92.5 | 84.1 / 97.7 | n/a ⁴ |
| Peak RSS (MB) ⁵ | 3891 / 3781 | 3598 / 3588 | 4626 / 4555 | 4495 / 4686 |
| GPU | RX 6800 XT (radeonsi GL 4.6) | RX 6800 XT (radeonsi GL 4.6) | RX 6800 XT (RADV Vulkan) | RX 6800 XT (RADV Vulkan) |

1. Quick play skips the title screen, so "title" is the game's `Connecting to 127.0.0.1` log line, and log
   lines have whole-second resolution. Real Minecraft's times count from the start of the Java process, which leaves
   out the time Prism spends before it launches the game. mcr's times count from the start of the mcr process, so
   they include the JVM boot inside it.
2. Windowed mcr with `--connect` connects without printing a title time. Its join time comes from the same server
   log line as every other client.
3. The `title: Java's GUI shown` line of the shell flow. Offscreen mode waits about 3 s for Java's title widgets to
   fade in before it connects, so its join comes later than the windowed one.
4. The offscreen figure is mcr's `--bench-timeline` count of unpaced frames in each second, averaged over seconds 31
   to 89 after the join. The mean frame time is 1000/fps. This mode records no per-frame times in the steady window.
   Its paced 30 s up to that window, chunk loading included, gave a mean of 0.94/0.90 ms, p50 0.81/0.78, p99
   3.27/3.14 and max 13.7/13.6 for Echoes, and a mean of 3.14/3.13 ms, p50 2.81/2.78, p99 7.65/7.97 and max
   55.7/55.1 for Medieval.
5. VmHWM of the game process. For real Minecraft this is the `java` process alone, without Prism.
   For mcr it is the whole process, including the JVM inside it. The Java heap was 2048 MiB (Echoes) and 2304 MiB
   (Medieval) in every client, with ZGC.

## Reading

- **Same mods, windowed.** mcr reaches about 70 to 95 % of real Minecraft's frame rate in both packs: Echoes 1340 to 1356
  fps against 1502 to 1875, and Medieval 588 to 609 fps against 644 to 730. Its p99 is higher in Medieval (4.75 ms
  against 3.0 ms) and close in Echoes (1.75 ms against 1.4 ms). Medieval's worst frames are about 100 ms in both
  clients.
- **Against the full pack** (Sodium, Iris, Xaero's minimap and the other mods that mcr blocks), mcr is faster in
  Medieval (about 600 fps against 432 to 573) and slower in Echoes (about 1350 against about 1540). Real Minecraft
  itself runs faster with mcr's mod set than with the full pack, clearly in Medieval and slightly in Echoes. The full
  pack's extra mods, such as the minimap, apparently cost more than Sodium saves at this view distance.
- **Offscreen numbers overstate light scenes.** Echoes renders 2330 to 2390 fps offscreen and 1340 to 1356 in a
  window. The difference, about 0.3 ms per frame, goes to presenting through gamescope and to MangoHud. Medieval is
  bound by Java's frame (1.4 to 1.8 ms per frame in the profile), so it shows the same rate both ways.
- **Startup.** mcr joins 2 s later than real Minecraft with the same mods in Echoes, and 4.5 s later in Medieval.
  Against the full pack it is 2 s faster in Echoes and 2 s slower in Medieval.
- **Memory.** mcr's process uses 0.2 to 0.5 GB more than the Minecraft `java` process in Echoes, and 0.7 to 1.0 GB
  more in Medieval.

## Method

- **Display.** Every graphical run used gamescope's headless backend at 2560x1440 (`run/bench-real/hl.sh`), started
  with DISPLAY, WAYLAND_DISPLAY and WAYLAND_SOCKET unset and a private XDG_RUNTIME_DIR. A Wayland client therefore had
  no route to the desktop compositor (`wayland-info` failed to connect), and the game saw only gamescope's Xwayland
  (`:1`). The game processes mapped `/dev/dri/renderD128` and libdrm_amdgpu. Minecraft's log, MangoHud and mcr's
  adapter line all reported the RX 6800 XT, never llvmpipe.
- **Real Minecraft.** Launched with `prismlauncher -l "<instance>" -o Bench` on the four `mcr bench` instances. Their
  window was set to 2560x1440 and options.txt had `renderDistance:12`, `maxFps:260` (uncapped), vsync off and
  `inactivityFpsLimit:"minimized"`. The default `afk` limit drops the game to 30 fps about a minute after the last
  input, which invalidated a first attempt. Each instance had one discarded warm-up launch.
- **Frame times, windowed.** MangoHud 0.8.4 logged every frame (`log_interval=0`) in both real Minecraft (OpenGL,
  through Prism's wrapper command) and mcr (Vulkan layer). Both clients drew the same `fps_only` overlay. The
  measurement window was cut out of the CSV using the `elapsed` column. mcr's own per-second fps lines agree with
  MangoHud within 2 fps.
- **mcr.** Ran with `--connect 127.0.0.1:25583 --mods .cache/packs/<slug>/mods --game-dir <copy of run/packs/<slug>>
  --name Bench --render-distance 12 --size 2560x1440`, adding `--no-vsync` for the windowed runs and
  `--screenshot … --after-ticks 600 --bench-timeline 90 --bench 3000` (with `MCR_MOD_PROFILE=frame`) for the
  offscreen ones. The JVM AOT cache was warm, built after one discarded offscreen run per pack.
- **Scene.** The Echoes spawn (8.5, 78, 7.5, yaw 0, pitch 0) faces a birch tree up close, a light scene. The Medieval
  spawn (422.5, 70, 325.5) looks across a village. Screenshots confirmed that every client showed the same view.
- **Server state.** The server ran in peaceful mode at fixed noon (`advance_time false`), except for Echoes runs
  e-pack-1/2 and e-mcr-1/2. Those ran earlier with the day cycle on, and e-pack-1 ran at night. A skeleton then killed
  the idle player, which invalidated the first four mcr Echoes runs (they measured the death screen at about 3000
  fps). Those runs were repeated after the server was switched to peaceful and fixed noon.
- Render distance 32 was not measured.

## Tooling problems found

These affect `tools/official-launcher/measure_pack.py` and were worked around by hand for this run:

- `prismlauncher -o <name>` launches in offline mode and does not download missing libraries or assets, so a first
  launch fails with "Missing libraries". The 26.3 Fabric libraries were fetched from Prism's own meta files.
- Prism stores `instance.cfg` with QSettings, which splits the comma-separated `MANGOHUD_CONFIG=…` in `WrapperCommand`
  into a list. MangoHud then never loads. A `MANGOHUD_CONFIGFILE=` path avoids the commas.
- MangoHud 0.8.4 writes no CSV when `no_display` is set. It also writes the CSV only when `log_duration` ends: a game
  that is killed first leaves nothing.
- Prism passes the game directory to Java on stdin, so `watch` (which looks for `--gameDir` in the command line) never
  finds the game process. Matching the process by working directory works.
- Minecraft's client log does not contain the `[CHAT] … joined the game` line in these packs, so the join time comes
  from the server log.
