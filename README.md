# mc-rust

A from-scratch Minecraft: Java Edition client in Rust, compatible with vanilla servers and built for
performance (wgpu, GPU-driven rendering, multithreaded everything).

Status: early development. Targets Minecraft 26.3.

```sh
cargo run -p mcr-app      # the client
cargo xtask --help        # dev tooling
```

You need your own copy of Minecraft; no Mojang assets are distributed here.
