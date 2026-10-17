# Design

## Layout on disk

```
<game dir>/
  eula.txt                         player's EULA agreement (written by mcr after the prompt)
  versions/26.3/server.jar         downloaded, SHA-1 checked; server.json records sha1+size
  versions/26.3/bundle/            bundler output (-DbundlerRepoDir), shared by all worlds
  runtime/java-runtime-epsilon/    Mojang's Java runtime, only when no Java 25 was found
  saves/<folder>/                  vanilla world (level.dat, region/, ...)
  saves/<folder>/mcr-server/       server cwd: server.properties, ops.json, eula.txt,
                                   logs/console.log, logs/latest.log, server.pid, mcr-world.properties
  saves/usercache.json             written by the server (its "universe" is the saves dir)
```

The server runs with `--universe <saves> --world <folder> --port <p> nogui`, so the level is the vanilla world
folder and every server-side file stays inside the world (`mcr-server/`), which vanilla ignores. Deleting a
world removes everything that belongs to it.

## Java: reuse first, Mojang's runtime as fallback

`mcr_jvm::HomeSearch` is what the mod layer uses at runtime (`MCR_JAVA_HOME`, the dev `.cache/tools/jdk25`,
`JAVA_HOME`, `java` on the path; Java 25 or newer). Singleplayer uses the same search so a machine that runs
mods runs worlds with the same JDK. When it finds nothing, mcr installs the component the version JSON names
(`javaVersion.component`, `java-runtime-epsilon` for 26.3) from Mojang's `java-runtime` manifest, the same
runtime the official launcher uses: raw files only (no LZMA), SHA-1 per file, at most 10,000 entries and
1 GiB, relative paths without `..`, symlinks only to targets inside the runtime, staged then renamed.

## Process model

`ManagedServer::start` spawns a supervisor thread that owns the `Child` for its whole life. This matters for
`PR_SET_PDEATHSIG`: Linux sends the signal when the *thread* that forked the child exits, so the fork must
happen on a thread that outlives the server. The supervisor:

- spawns `systemd-run --user --scope -q -p MemoryMax=<heap+1G> -p MemorySwapMax=0 java …` when a probe of
  `systemd-run --user --scope true` succeeds (systemd-run execs in place, so the child pid is the JVM), else
  `java …` directly; `process_group(0)` and a `pre_exec` that sets `PR_SET_PDEATHSIG(SIGTERM)` and exits if
  the parent is already gone. SIGTERM lets the JVM's shutdown hook save the world.
- reads stdout and stderr on two threads with a bounded line reader (4 KiB per line, the rest dropped),
  strips control characters, appends to `console.log` (stops at 64 MiB) and forwards only parsed events
  (`Done`, `Preparing spawn area: N%`, stage lines) to the supervisor through one channel shared with commands.
- handles `Console(line)`, `Stop { grace }` (writes `save-all flush` and `stop`, then kills the group after
  the grace) and `Kill`; kills the group if `Done` does not arrive before the ready timeout (5 min).
- reports `Event::Stage`, `Event::Ready` and `Event::Exited { code, message }` to the shell.

`Drop` on `ManagedServer` stops gracefully (30 s) and joins, so closing the window saves the world.

## Stale servers

Each start writes `mcr-server/server.pid` as `<pid> <token>` after the spawn; the token is random per launch
and is also passed to the JVM as `-Dmcr.sp.token=<token>`, next to `-Dmcr.sp.world=<canonical world path>`.
A world folder can come from a download, so the pid file is untrusted: before a start, mcr signals a pid only
when it can prove it is its own server for this world — same uid (`/proc/<pid>/status`), a server jar on the
command line, this world's exact marker and the pid file's token. On Linux it also asks `fcntl(F_GETLK)` who
holds `session.lock` (Java's `FileChannel.tryLock` takes a POSIX record lock, which reports its owner); a
holder is stopped only with the same proof (minus the token), otherwise the start is refused ("world is in
use"). Proven servers get SIGTERM, then SIGKILL after 60 s (a JVM saving a big world on SIGTERM needs time). Off Linux nothing is ever signalled.

## Hostile world folders and properties

`server.properties` is read by `java.util.Properties`, so mcr parses an existing file with the same rules
(continuations, escapes, `=`/`:`/whitespace separators, `#`/`!` comments, `\uXXXX`), re-serialises the
entries it does not manage, and writes every managed key last, escaped like `Properties.store` (`\`, `=`,
`:`, `#`, `!`, leading spaces, CR, LF, FF, tab, non-ASCII as `\uXXXX`). No value (world name, seed, motd) can
start another key, and no file shipped in a downloaded world can override the forced keys: loopback
`server-ip`, the port, `online-mode=false`, rcon, query, management server, JMX and status off, empty rcon
password, no required resource pack, operator level 4. Reading is bounded: at most 64 KiB of the file, logical
lines (continuations included) up to 4 KiB, 1000 entries, and the parser is linear in its input. `ops.json` is rewritten on every launch and holds only
the local player's offline UUID, and only when cheats are on.

## Pause

The pause screen reports `pauses_game` only for a local server. Each frame the shell compares "any screen on
the play stack pauses" with the last state it sent and writes `tick freeze` / `tick unfreeze` to the server
console. The console is used instead of a player command: it needs no operator status (so survival worlds
without cheats pause too, like vanilla), and `broadcast-console-to-ops=false` keeps the feedback out of chat.
Before Save and Quit the shell unfreezes if it froze.

## Memory

Heap = total RAM / 4 clamped to 2048–4096 MiB, `MCR_SP_HEAP_MIB` (512–16384) overriding; the scope's
`MemoryMax` = heap + 1024 MiB because of metaspace, code cache, thread stacks and direct buffers. During
development `MCR_SP_HEAP_MIB=1536` keeps the scope at 2560M.

## Ports

A free port is taken by binding `127.0.0.1:0` and releasing it just before the start; both `server-port` and
`--port` get it. The tiny race with another program is reported as a start failure.

## EULA

The dedicated server refuses to run without `eula=true`. mcr asks once with the EULA link, stores the answer
in `<game dir>/eula.txt`, and writes the server's `eula.txt` only after that.

## Not done

Open to LAN (needs `/publish`-like support and a non-loopback bind), Re-Create, backups, data pack and
experiment screens, single-biome and custom superflat presets, writing `icon.png` on save, and singleplayer
from Java's title screen in mod mode.
