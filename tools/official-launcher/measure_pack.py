#!/usr/bin/env python3
"""Measure a fetched CurseForge pack in the real Java client (Prism Launcher by default), for comparison with mcr's mod mode.

Owner how-to (Prism; one-time: Prism needs an account, any works on the offline-mode bench server. Settings >
Accounts > Add offline, or keep your Microsoft one; instances use the default account):

    python3 tools/official-launcher/measure_pack.py prepare echoes-of-the-sift-fabric --set pack   # also: --set mcr
    python3 tools/official-launcher/measure_pack.py prepare medieval-immersed --set pack           # also: --set mcr
    python3 tools/official-launcher/measure_pack.py serve medieval-immersed      # bench server on 25583, own 4G scope
    # restart Prism (it reads instances at start), launch "mcr bench - <Pack> (full pack)" or "(mcr set)":
    # the first launch downloads Minecraft, Fabric and libraries and records no useful times; quit and launch again
    python3 tools/official-launcher/measure_pack.py watch medieval-immersed --set pack   # before or while the game runs
    # the game joins 127.0.0.1:25583 by itself; wait for chunks, press Shift_L+F2 (MangoHud) to log 60 s of frames
    # standing still, quit the game; watch then prints the summary (rerun watch --report for the last run)
    python3 tools/official-launcher/measure_pack.py serve --stop

`--set pack` is every jar the pack lists (Sodium, Iris and the rest: the real player experience); `--set mcr` the jars
mcr's client loads. `prepare` is idempotent and replaces mods/ and options.txt of its own instances only, adding them
to the group "mcr bench" (instgroups.json is backed up once as instgroups.json.mcr-bench-backup). The instance has
1280x720, uncapped fps, no vsync, ZGC, the heap of mcr's measurements (2048 MiB, 2304 MiB over 60 mcr jars) and
MangoHud via WrapperCommand, logging to ~/.minecraft-mcr-bench/mangohud/<slug>-<set> (no spaces: Prism splits the wrapper); `--autostart-log SECS` makes it log without the key.
Title is the "Connecting to" log line (quick play skips the title screen); join is the "joined the game" chat line.

Official launcher (the earlier path): add `--launcher official` to prepare/watch; it copies into ~/.minecraft-mcr-bench
and adds an installation to ~/.minecraft/launcher_profiles.json (backed up once), and watch asks for Enter at the title.
MC_LAUNCHER_DIR, MCR_BENCH_DIR, PRISM_DIR override ~/.minecraft, ~/.minecraft-mcr-bench, ~/.local/share/PrismLauncher.
"""

import argparse
import csv
import datetime
import json
import os
import re
import select
import shutil
import signal
import statistics
import subprocess
import sys
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CACHE = REPO / ".cache"
PACKS = CACHE / "packs"
FABRIC_PROFILE = CACHE / "fabric" / "profile-26.3-0.19.5.json"
LAUNCHER_DIR = Path(os.environ.get("MC_LAUNCHER_DIR", Path.home() / ".minecraft"))
BENCH_ROOT = Path(os.environ.get("MCR_BENCH_DIR", Path.home() / ".minecraft-mcr-bench"))
PRISM_DIR = Path(os.environ.get("PRISM_DIR", Path.home() / ".local/share/PrismLauncher"))
BENCH_PORT = 25583
GROUP = "mcr bench"
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


def prepare_official(args):
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


def watch_official(args):
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


def pack_title(slug):
    return json.loads((PACKS / slug / "pack.json").read_text())["name"]


def default_heap_mib(slug):
    return 2304 if len(pack_jars(slug, "mcr")) > 60 else 2048


def prism_name(slug, which):
    label = "full pack" if which == "pack" else "mcr set"
    return re.sub(r"[/\\:]", "-", f"mcr bench - {pack_title(slug)} ({label})")


def prism_dir(slug, which):
    return PRISM_DIR / "instances" / prism_name(slug, which)


def mango_dir(slug, which):
    return BENCH_ROOT / "mangohud" / f"{slug}-{which}"


def read_cfg(path):
    cfg = {}
    if path.exists():
        for line in path.read_text().splitlines():
            if "=" in line and not line.startswith("["):
                k, v = line.split("=", 1)
                cfg[k] = v
    return cfg


def prism_java():
    found = sorted((PRISM_DIR / "java").glob("java-runtime-epsilon*/bin/java"))
    return str(found[-1]) if found else None


def mango_env(mango, autostart):
    config = f"no_display,log_duration=60,output_folder={mango}"
    if autostart:
        config += f",autostart_log={autostart}"
    return f"env MANGOHUD_DLSYM=1 MANGOHUD_CONFIG={config} mangohud"


def prism_components():
    return {
        "components": [
            {"cachedName": "LWJGL 3", "cachedVersion": "3.4.3", "dependencyOnly": True, "uid": "org.lwjgl3", "version": "3.4.3"},
            {
                "cachedName": "Minecraft",
                "cachedRequires": [{"suggests": "3.4.3", "uid": "org.lwjgl3"}],
                "cachedVersion": "26.3",
                "important": True,
                "uid": "net.minecraft",
                "version": "26.3",
            },
            {
                "cachedName": "Intermediary Mappings",
                "cachedRequires": [{"equals": "26.3", "uid": "net.minecraft"}],
                "cachedVersion": "26.3",
                "dependencyOnly": True,
                "uid": "net.fabricmc.intermediary",
                "version": "26.3",
            },
            {
                "cachedName": "Fabric Loader",
                "cachedRequires": [{"uid": "net.fabricmc.intermediary"}],
                "cachedVersion": "0.19.5",
                "uid": "net.fabricmc.fabric-loader",
                "version": "0.19.5",
            },
        ],
        "formatVersion": 1,
    }


CFG_DEFAULTS = {
    "AutoCloseConsole": "false", "AutomaticJava": "true", "CloseAfterLaunch": "false", "ConfigVersion": "1.3",
    "EnableFeralGamemode": "false", "EnableMangoHud": "false", "Env": "{}", "InstanceAccountId": "",
    "InstanceType": "OneSix", "IgnoreJavaCompatibility": "false", "JoinWorldOnLaunch": "", "LaunchMaximized": "false",
    "LogPrePostOutput": "true", "ManagedPack": "false", "MinecraftWinHeight": "480", "MinecraftWinWidth": "854",
    "QuitAfterGameStop": "false", "RecordGameTime": "true", "ShowConsole": "false", "ShowConsoleOnError": "true",
    "UseAccountForInstance": "false", "iconKey": "default", "notes": "",
}


def prepare_prism(args):
    slug, which = args.slug, args.set
    if not (PACKS / slug / "pack.json").exists():
        sys.exit(f"no fetched pack {slug}: run cargo xtask modpack-fetch .cache/packs/<zip> first")
    heap = args.heap_mib or default_heap_mib(slug)
    inst = prism_dir(slug, which)
    game = inst / "minecraft"
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
    mango = mango_dir(slug, which)
    mango.mkdir(parents=True, exist_ok=True)

    cfg = {**CFG_DEFAULTS, **read_cfg(inst / "instance.cfg")}
    java_args = "-XX:+UseZGC"
    cfg.update({
        "name": prism_name(slug, which),
        "OverrideMemory": "true", "MaxMemAlloc": str(heap), "MinMemAlloc": "512", "PermGen": "128",
        "OverrideJavaArgs": "true", "JvmArgs": java_args,
        "OverrideWindow": "true", "MinecraftWinWidth": "1280", "MinecraftWinHeight": "720", "LaunchMaximized": "false",
        "OverrideCommands": "true", "WrapperCommand": mango_env(mango, args.autostart_log),
        "JoinServerOnLaunch": "true", "JoinServerOnLaunchAddress": f"127.0.0.1:{BENCH_PORT}",
        "ShowConsole": "false",
    })
    cfg.setdefault("uuid", uuid.uuid4().hex)
    java = prism_java()
    if java:
        cfg.update({"OverrideJavaLocation": "true", "JavaPath": java})
    inst.mkdir(parents=True, exist_ok=True)
    (inst / "instance.cfg").write_text("[General]\n" + "".join(f"{k}={v}\n" for k, v in sorted(cfg.items())))
    (inst / "mmc-pack.json").write_text(json.dumps(prism_components(), indent=4) + "\n")

    groups_path = PRISM_DIR / "instances" / "instgroups.json"
    groups = json.loads(groups_path.read_text()) if groups_path.exists() else {"formatVersion": "1", "groups": {}}
    backup = groups_path.with_name("instgroups.json.mcr-bench-backup")
    if groups_path.exists() and not backup.exists():
        shutil.copy2(groups_path, backup)
    members = groups.setdefault("groups", {}).setdefault(GROUP, {"hidden": False, "instances": []})
    if inst.name not in members["instances"]:
        members["instances"].append(inst.name)
    groups_path.write_text(json.dumps(groups, indent=4) + "\n")
    print(f"prism instance: {inst} ({len(list(mods.iterdir()))} jars, heap {heap} MiB, group '{GROUP}')")
    print(f"frame logs: {mango}; server: 127.0.0.1:{BENCH_PORT}; groups backup: {backup}")
    if not java:
        print("no Prism-managed Java 25 found; Prism picks one itself on launch (AutomaticJava)")


def serve(args):
    state = CACHE / "bench"
    state.mkdir(exist_ok=True)
    pidfile = state / "server.pid"
    pid = int(pidfile.read_text()) if pidfile.exists() else None
    alive = pid is not None and Path(f"/proc/{pid}").exists()
    if args.stop:
        if alive:
            os.killpg(pid, signal.SIGTERM)
            print(f"stopped bench server (process group {pid})")
        else:
            print("no bench server running")
        pidfile.unlink(missing_ok=True)
        return
    if not args.slug:
        sys.exit("serve needs a slug (or --stop)")
    if alive:
        sys.exit(f"bench server already running (pid {pid}); serve --stop first")
    log = state / f"server-{args.slug}.log"
    build = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=5G", "-p", "MemorySwapMax=0",
             "env", "CARGO_BUILD_JOBS=6", "cargo", "build", "-q", "-p", "xtask"]
    subprocess.run(build, cwd=REPO, check=True)
    cmd = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=4G", "-p", "MemorySwapMax=0",
           "cargo", "xtask", "fabric-server", "--pack", args.slug, "--port", str(BENCH_PORT)]
    with log.open("w") as out:
        proc = subprocess.Popen(cmd, cwd=REPO, stdin=subprocess.DEVNULL, stdout=out, stderr=subprocess.STDOUT, start_new_session=True,
                                env={**os.environ, "MCR_ACCEPT_EULA": "1"})
    pidfile.write_text(str(proc.pid))
    print(f"bench server for {args.slug} on 127.0.0.1:{BENCH_PORT}, pid/process group {proc.pid}, log {log}")
    print("ready when the log shows 'Done'; stop with: measure_pack.py serve --stop")


LOG_TIME = re.compile(r"^\[(\d\d):(\d\d):(\d\d)\]")


def log_events(log, start):
    """Seconds after `start` of the connect and join log lines (log times are local HH:MM:SS)."""
    day = datetime.datetime.fromtimestamp(start).replace(hour=0, minute=0, second=0, microsecond=0)
    events = {}
    prev = 0
    for line in log.read_text(errors="replace").splitlines():
        m = LOG_TIME.match(line)
        if not m:
            continue
        h, mi, se = map(int, m.groups())
        t = day.timestamp() + h * 3600 + mi * 60 + se
        if t < prev:
            day += datetime.timedelta(days=1)
            t += 86400
        prev = t
        if "connect" not in events and re.search(r"Connecting to 127\.0\.0\.1", line):
            events["connect"] = t - start
        if "join" not in events and re.search(r"\[CHAT\] .* joined the game", line):
            events["join"] = t - start
    return events


def process_start(pid):
    ticks = os.sysconf("SC_CLK_TCK")
    boot = time.time() - float(Path("/proc/uptime").read_text().split()[0])
    return boot + int(Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]) / ticks


def watch_prism(args):
    inst = prism_dir(args.slug, args.set)
    if not inst.exists():
        sys.exit(f"{inst} is missing: run prepare first")
    game = inst / "minecraft"
    state_path = inst / "bench-run.json"
    if not args.report:
        print(f"waiting for the game with --gameDir {game}; launch '{inst.name}' in Prism now (Ctrl-C to give up)")
        pid = None
        while pid is None:
            pid = find_game(game)
            time.sleep(0.2)
        start = process_start(pid)
        print(f"game pid {pid}, started {time.time() - start:.1f} s ago; join is automatic, then press Shift_L+F2 for frames")
        peak = 0
        while Path(f"/proc/{pid}").exists():
            peak = max(peak, status_kib(pid, "VmRSS") or 0)
            state_path.write_text(json.dumps({"start": start, "peak_kib": peak}))
            time.sleep(1.0)
    if not state_path.exists():
        sys.exit("no recorded run: launch the game while watch is running")
    state = json.loads(state_path.read_text())
    log = game / "logs" / "latest.log"
    events = log_events(log, state["start"]) if log.exists() else {}
    print()
    print(f"== {args.slug} ({args.set}) in Prism Launcher")
    print(f"title: {events['connect']:.1f} s after launch (quick play: the connect line)" if "connect" in events else "title: no 'Connecting to' line in latest.log")
    print(f"joined at {events['join']:.1f} s after launch" if "join" in events else "joined: no '[CHAT] ... joined the game' line seen")
    print(f"peak rss {state['peak_kib'] // 1024} MB (sampled each second)")
    print(frame_stats(mango_dir(args.slug, args.set), state["start"]) or f"frame time: no MangoHud log in {mango_dir(args.slug, args.set)}")


def prepare(args):
    (prepare_official if args.launcher == "official" else prepare_prism)(args)


def watch(args):
    (watch_official if args.launcher == "official" else watch_prism)(args)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("prepare", "watch"):
        p = sub.add_parser(name)
        p.add_argument("slug")
        p.add_argument("--set", choices=("mcr", "pack"), default="pack")
        p.add_argument("--launcher", choices=("prism", "official"), default="prism")
        if name == "prepare":
            p.add_argument("--heap-mib", type=int, default=None)
            p.add_argument("--autostart-log", type=int, default=0, help="MangoHud starts logging this many seconds after launch")
        else:
            p.add_argument("--report", action="store_true", help="summarise the last run without waiting for the game")
    p = sub.add_parser("serve")
    p.add_argument("slug", nargs="?")
    p.add_argument("--stop", action="store_true")
    args = parser.parse_args()
    if args.cmd == "serve":
        serve(args)
    else:
        if args.cmd == "prepare" and args.launcher == "official" and args.heap_mib is None:
            args.heap_mib = 2048
        (prepare if args.cmd == "prepare" else watch)(args)


if __name__ == "__main__":
    main()
