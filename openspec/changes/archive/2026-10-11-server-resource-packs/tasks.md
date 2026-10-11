## 1. Assets
- [x] 1.1 `ZipSource::open_limited` with entry/size limits and path rejection
- [x] 1.2 Bound PNG dimensions in sprite decoding

## 2. Pack manager and downloader (`mcr-respack`)
- [x] 2.1 Manager state machine with vanilla matrix tests
- [x] 2.2 Downloader: caps, timeouts, SHA-1, cache, eviction; tests against a local HTTP server

## 3. Client link
- [x] 3.1 `PackLink` channels; configuration and play handling; disconnect reply

## 4. App integration
- [x] 4.1 Server entry setting (edit screen, `servers.dat`), confirm screen, required-decline disconnect
- [x] 4.2 Hot reload: assets, renderer, mesh pool remesh, entities/effects, GUI, audio overlay
- [x] 4.3 Pop and re-config handling

## 5. Verification
- [x] 5.1 Live: serve a test pack, join, screenshot retextured stone, pop; hash mismatch; hostile zip
- [x] 5.2 Gate: fmt, clippy, tests, `openspec validate --all --strict`
