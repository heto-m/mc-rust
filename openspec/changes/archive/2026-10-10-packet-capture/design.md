## Context

`mcr-net` provides a threaded `Connection` with an inbound hook that applies compression switches at the exact frame boundary. `mcr-data` maps packet ids to names per state and direction. The managed server runs via `cargo xtask server`.

## Decisions

- Serverbound packets are encoded by tiny local functions using `mcr-buf` helpers, so the capture does not depend on `mcr-protocol`, which is being written concurrently and must be validated against this tool's output.
- The inbound hook only handles `login_compression`; state transitions are driven by the main loop on the frames it receives.
- Only inbound frames are recorded; outbound traffic is ours and trivially known.
- Server spawning shells out to `cargo xtask server --nogui` with piped stdin and stdout; readiness is the `Done` line. Without the flag the user runs the server separately and scripts are unavailable.
- Large-layout packets bypass the cap via an allowlist; everything else caps at 20.

## Risks / Trade-offs

- Serverbound layouts may drift between versions: they are exercised live, so a mismatch shows as a disconnect.
