## 1. Crate and codecs
- [x] 1.1 Scaffold `mcr-net` crate and workspace wiring
- [x] 1.2 Error types
- [x] 1.3 Compression module (libdeflater) and cipher module (AES-128-CFB8)
- [x] 1.4 `FrameDecoder` and `FrameEncoder`
- [x] 1.5 Buffer pool

## 2. Connection
- [x] 2.1 Reader and writer threads, events, switches, shutdown
- [x] 2.2 Server address parsing and SRV client

## 3. Verification
- [x] 3.1 Unit tests: framing edge cases, compression boundary, CFB8 known answer, DNS packets
- [x] 3.2 Loopback test with fake server enabling compression and encryption mid-stream
- [x] 3.3 Criterion benches: decode throughput, backend comparison
- [x] 3.4 fmt, clippy, tests, archive
