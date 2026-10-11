## 0. Spike
- [ ] 0.1 With the booted bridge, build a `BridgeChannel` `Connection`, run `setupInboundProtocol`/`setupOutboundProtocol` for login, and feed a captured `LoginFinished` frame from `.cache/captures`; confirm inline completion and listener swap; record the result and the fallback decision in design.md

## 1. Rust side
- [ ] 1.1 `mcr-net` `frame_tap` option (reader thread, after decode), with a loopback test proving every frame is observed once in order
- [ ] 1.2 Phase tracker from clientbound transition ids, with unit tests on a recorded login → config → play → config sequence
- [ ] 1.3 Clientbound ring writer with spill and desync handling
- [ ] 1.4 Serverbound ring reader, id parsing, forward table, per-class drop counters, and unit tests for each phase row

## 2. Java side
- [ ] 2.1 `BridgeChannel` and the `Connection.connect` redirect, plus the `request_connect` downcall and the Rust-initiated `ConnectScreen.startConnecting`
- [ ] 2.2 Inbound drain in `awaitFrame`, inbound/outbound phase hooks, and outbound queue drain
- [ ] 2.3 Chat/command redirect to `send_chat`
- [ ] 2.4 Player shadow: `PlayerState` message, `LocalPlayer` hooks, suppression of vanilla gameplay keybinds
- [ ] 2.5 Disconnect propagation both ways

## 3. Verification
- [ ] 3.1 Integration test against `xtask server` (vanilla 26.3, offline, one agent at a time): chunk set equality, block states at 20 sampled positions, entity id set equality after 30 s
- [ ] 3.2 Test mod (`java/mcr-testmod`) sending a `ClientPlayNetworking` payload on join; the server log shows the custom payload, and capture shows a single keep-alive response per request
- [ ] 3.3 Disconnect both ways: server kick → Java disconnect screen; Java pause-menu disconnect → Rust closes the socket

## 4. Measurement
- [ ] 4.1 Criterion bench: tap overhead per frame on the reader thread (target under 1% of decode time)
- [ ] 4.2 Java heap per loaded column and Java-thread chunk-parse time at render distance 12; record in design.md and decide whether the mirror-backed storage follow-up is needed
- [ ] 4.3 fmt, clippy, tests, archive
