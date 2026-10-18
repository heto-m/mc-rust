#!/usr/bin/env python3
"""Measure a fetched CurseForge pack in the official Minecraft launcher, for comparison with mcr's mod mode.

    python3 tools/official-launcher/measure_pack.py prepare <slug> [--set mcr|pack] [--heap-mib N]
    python3 tools/official-launcher/measure_pack.py watch <slug> [--set mcr|pack]

Steps (the same server, pack and window size mcr was measured with):
  1. cargo xtask modpack-fetch .cache/packs/<zip>; cargo xtask fabric-server --pack <slug> --port 25576
  2. prepare: copies the jars and overrides into ~/.minecraft-mcr-bench/<slug>-<set>, forces uncapped fps and no
     vsync, installs the Fabric 0.19.5 profile and adds the installation "mcr bench: <slug> (<set>)" (1280x720,
     ZGC, --heap-mib like mcr's mcr-mods.json) to launcher_profiles.json, backing that file up once.
  3. Start the launcher under MangoHud, which logs per-frame times (the command is printed by watch).
  4. watch, then Play: press Enter in the terminal when the title screen shows, join 127.0.0.1:25576, wait for
     chunks, press Shift_L+F2 to log 60 s of frames standing still, quit the game. watch prints title and join
     time, peak RSS and frame times, in the shape of mcr's run/bin/measure.sh summary.
`--set mcr` uses the jars mcr's client loads; `--set pack` every jar the pack lists (Sodium, Iris and the rest).
MC_LAUNCHER_DIR and MCR_BENCH_DIR override ~/.minecraft and ~/.minecraft-mcr-bench.
"""

import argparse
import csv
import json
import os
import re
import select
import shutil
import statistics
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CACHE = REPO / ".cache"
PACKS = CACHE / "packs"
FABRIC_PROFILE = CACHE / "fabric" / "profile-26.3-0.19.5.json"
LAUNCHER_DIR = Path(os.environ.get("MC_LAUNCHER_DIR", Path.home() / ".minecraft"))
BENCH_ROOT = Path(os.environ.get("MCR_BENCH_DIR", Path.home() / ".minecraft-mcr-bench"))
FORCED_OPTIONS = {"maxFps": "260", "enableVsync": "false", "pauseOnLostFocus": "false"}


def instance_dir(slug, which):
    return BENCH_ROOT / f"{slug}-{which}"


def pack_jars(slug, which):
    """`mcr`: the jars mcr's client loads. `pack`: every jar the pack lists, as the CurseForge app installs it."""
    pack = json.loads((PACKS / slug / "pack.json").read_text())
    jars = []
    for entry in pack["files"]:
        name = entry["file"]
        if not name.endswith(".jar"):
            continue
        if which == "mcr" and entry.get("client") != "kept":
            continue
        found = list((PACKS / "files").glob(f"*/{glob_escape(name)}"))
        if not found:
            sys.exit(f"{name} is not in {PACKS / 'files'}; run cargo xtask modpack-fetch first")
        jars.append(found[0])
    return jars


def glob_escape(name):
    return re.sub(r"([\[\]*?])", r"[\1]", name)


def write_options(path):
    lines = path.read_text().splitlines() if path.exists() else []
    seen = set()
    out = []
    for line in lines:
        key = line.split(":", 1)[0]
        if key in FORCED_OPTIONS:
            out.append(f"{key}:{FORCED_OPTIONS[key]}")
            seen.add(key)
        else:
            out.append(line)
    out += [f"{k}:{v}" for k, v in FORCED_OPTIONS.items() if k not in seen]
    path.write_text("\n".join(out) + "\n")


def prepare(args):
    slug, which = args.slug, args.set
    if not (PACKS / slug / "pack.json").exists():
        sys.exit(f"no fetched pack {slug}: run cargo xtask modpack-fetch .cache/packs/<zip> first")
    game = instance_dir(slug, which)
    mods = game / "mods"
    if mods.exists():
        shutil.rmtree(mods)
    mods.mkdir(parents=True)
    for jar in pack_jars(slug, which):
        shutil.copy2(jar, mods / jar.name)
    overrides = PACKS / slug / "overrides"
    if overrides.exists():
        shutil.copytree(overrides, game, dirs_exist_ok=True)
    write_options(game / "options.txt")
    (game / "eula.txt").unlink(missing_ok=True)

    profile = json.loads(FABRIC_PROFILE.read_text())
    version_id = profile["id"]
    version_dir = LAUNCHER_DIR / "versions" / version_id
    version_dir.mkdir(parents=True, exist_ok=True)
    (version_dir / f"{version_id}.json").write_text(json.dumps(profile, indent=2))

    profiles_path = LAUNCHER_DIR / "launcher_profiles.json"
    if not profiles_path.exists():
        sys.exit(f"{profiles_path} is missing: start the official launcher once first")
    backup = profiles_path.with_suffix(".json.mcr-bench-backup")
    if not backup.exists():
        shutil.copy2(profiles_path, backup)
    profiles = json.loads(profiles_path.read_text())
    key = f"mcr-bench-{slug}-{which}"
    profiles.setdefault("profiles", {})[key] = {
        "name": f"mcr bench: {slug} ({which})",
        "type": "custom",
        "lastVersionId": version_id,
        "gameDir": str(game),
        "javaArgs": f"-Xmx{args.heap_mib}M -Xms512M -XX:+UseZGC",
        "resolution": {"width": 1280, "height": 720},
        "icon": "Furnace",
        "created": time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime()),
    }
    profiles_path.write_text(json.dumps(profiles, indent=2))
    print(f"instance: {game} ({len(list(mods.iterdir()))} jars)")
    print(f"launcher installation: mcr bench: {slug} ({which}), Fabric {version_id}, heap {args.heap_mib} MiB")
    print(f"launcher profiles backed up to {backup}")


def proc_matches(pid, game):
    try:
        cmd = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
    except OSError:
        return False
    args = [a.decode(errors="replace") for a in cmd]
    return any(a == "--gameDir" and i + 1 < len(args) and Path(args[i + 1]) == game for i, a in enumerate(args))


def find_game(game):
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit() and proc_matches(entry.name, game):
            return int(entry.name)
    return None


def status_kib(pid, key):
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith(key + ":"):
                return int(line.split()[1])
    except OSError:
        pass
    return None


def frame_stats(folder, since):
    """MangoHud logs: per-frame `frametime` in ms (older builds wrote microseconds)."""
    logs = sorted(p for p in folder.glob("*.csv") if p.stat().st_mtime >= since and "summary" not in p.name)
    if not logs:
        return None
    rows = []
    with logs[-1].open() as f:
        reader = csv.reader(f)
        header = None
        for row in reader:
            if header is None:
                if "frametime" in row:
                    header = row.index("frametime")
                continue
            try:
                rows.append(float(row[header]))
            except (ValueError, IndexError):
                continue
    if not rows:
        return None
    if statistics.median(rows) > 1000:
        rows = [r / 1000 for r in rows]
    rows.sort()
    p = lambda q: rows[min(len(rows) - 1, int(q * len(rows)))]
    mean = statistics.fmean(rows)
    return f"frame time ms over {len(rows)} frames: mean {mean:.2f} p50 {p(0.5):.2f} p99 {p(0.99):.2f} max {rows[-1]:.2f} ({1000 / mean:.0f} fps), from {logs[-1].name}"


def watch(args):
    game = instance_dir(args.slug, args.set)
    if not game.exists():
        sys.exit(f"{game} is missing: run prepare first")
    mango = game / "mangohud"
    mango.mkdir(exist_ok=True)
    started_watch = time.time()
    print("start the launcher with frame logging, if it is not running yet:")
    print(f"  MANGOHUD_DLSYM=1 MANGOHUD_CONFIG=no_display,log_duration=60,output_folder={mango} mangohud minecraft-launcher")
    print(f"waiting for the game with --gameDir {game}; launch 'mcr bench: {args.slug} ({args.set})' now")
    pid = None
    while pid is None:
        pid = find_game(game)
        time.sleep(0.2)
    ticks = os.sysconf("SC_CLK_TCK")
    boot = time.time() - float(Path("/proc/uptime").read_text().split()[0])
    start = boot + int(Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]) / ticks
    print(f"game pid {pid}, started {time.time() - start:.1f} s ago")
    print("press Enter the moment the title screen appears (after the loading overlay)")
    title = None
    log = game / "logs" / "latest.log"
    join = None
    peak = 0
    while Path(f"/proc/{pid}").exists():
        ready, _, _ = select.select([sys.stdin], [], [], 1.0)
        if ready and title is None:
            sys.stdin.readline()
            title = time.time() - start
            print(f"title at {title:.1f} s; now join the test server and stand still")
        rss = status_kib(pid, "VmRSS")
        peak = max(peak, rss or 0)
        if join is None and log.exists() and log.stat().st_mtime >= start:
            if re.search(r"\[CHAT\] .* joined the game", log.read_text(errors="replace")):
                join = time.time() - start
                print(f"joined at {join:.1f} s; let chunks load, then log frames (MangoHud Shift_L+F2, 60 s), then quit")
    print()
    print(f"== {args.slug} ({args.set}) in the official launcher")
    print(f"title: {title:.1f} s after launch" if title else "title: not marked")
    print(f"joined at {join:.1f} s after launch" if join else "joined: no '[CHAT] ... joined the game' line seen")
    print(f"peak rss {peak // 1024} MB (sampled each second)")
    stats = frame_stats(mango, started_watch)
    print(stats or f"frame time: no MangoHud log in {mango}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("prepare", "watch"):
        p = sub.add_parser(name)
        p.add_argument("slug")
        p.add_argument("--set", choices=("mcr", "pack"), default="mcr")
        if name == "prepare":
            p.add_argument("--heap-mib", type=int, default=2048)
    args = parser.parse_args()
    (prepare if args.cmd == "prepare" else watch)(args)


if __name__ == "__main__":
    main()
