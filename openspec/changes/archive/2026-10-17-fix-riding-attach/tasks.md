## 1. Implementation

- [x] 1.1 `mcr-client::attach`: attachment tables, boat/camel/minecart seats, baby and scale handling
- [x] 1.2 Use it for the local passenger in `riding.rs` and for remote riders in the entity renderer
- [x] 1.3 Tests: horse adult and baby, boat with one and two passengers, raft, camel, minecart, pig, happy ghast

## 2. Verify

- [x] 2.1 fmt, clippy, workspace tests
- [x] 2.2 Live check against a vanilla server: horse, boat, pig, minecart (`tests/live_seats.rs`, ignored)
