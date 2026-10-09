## 1. Crate

- [x] 1.1 Add `crates/mcr-capture` with CLI, local serverbound encoders and unit tests for the encoders and sample caps (`cargo test -p mcr-capture`)
- [x] 1.2 Implement the protocol walk, recorder and index (`cargo run -p mcr-capture` against a live server produces `index.jsonl`)
- [x] 1.3 Implement server spawn and console script feeding, plus crate README

## 2. Capture

- [x] 2.1 Run both known-packs modes with a script and confirm captures exist under `.cache/captures/26.3`
