# mcr-capture

Records real vanilla 26.3 server traffic as byte-exact fixtures for the packet decoders. Captures land
in the gitignored `.cache/captures/`; only code is committed.

Each inbound frame is written (decompressed, without the packet id) to
`<out>/<state>/<direction>/<packet_name>/<seq>.bin` and listed in `<out>/index.jsonl`
(`seq`, `state`, `direction`, `id`, `name`, `len`, `timestamp_ms`). Samples are capped per packet name;
`registry_data` is kept in full and heavy packets (chunks, commands, tags, recipes, entity data) get a
larger cap.

## Regenerate

Requires the server jar (`cargo xtask fetch`) and an accepted Minecraft EULA.

```sh
export MCR_ACCEPT_EULA=1
# Known packs echoed back (server skips vanilla registry contents): .cache/captures/26.3
cargo run -p mcr-capture -- --spawn-server --script crates/mcr-capture/commands.txt --seconds 90
# Zero known packs (full registry_data): .cache/captures/26.3-no-known-packs
cargo run -p mcr-capture -- --spawn-server --script crates/mcr-capture/commands.txt --seconds 90 --no-known-packs
```

Without `--spawn-server`, start the server yourself (`cargo xtask server --nogui`) and pass
`--addr host:port`; scripts need the spawned server because they are fed to its console.

Script lines are console commands; `sleep <secs>` pauses and `{name}` is replaced by the player name.
The server console log is written to `.cache/captures/server.log`.
