## 1. Cleanup

- [x] 1.1 Replace placeholder types with real component/item types; remove all `todo!()`
- [x] 1.2 Un-ignore all ignored tests and make them pass
- [x] 1.3 Remove duplicates and inconsistent names (shared game, profile, component types)

## 2. Dispatch

- [x] 2.1 `ProtocolError` with packet, state and direction context
- [x] 2.2 `PacketIds` from `mcr_data::Version`
- [x] 2.3 Dispatch enums for all state/direction pairs
- [x] 2.4 Coverage test against mcr-data names

## 3. Capture validation

- [x] 3.1 Capture replay test for both capture dirs
- [x] 3.2 Fix codec bugs found until all captured frames are byte-exact
- [x] 3.3 Review uncaptured packets against the vanilla codecs and add byte fixtures

## 4. Bench

- [x] 4.1 Decode throughput bench over captured play stream
