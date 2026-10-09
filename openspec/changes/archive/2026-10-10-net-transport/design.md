## Context

`mcr-net` moves raw frames and knows nothing about packet definitions. Threads are blocking (no tokio). Reference behavior: vanilla `Varint21FrameDecoder`, `CompressionDecoder/Encoder`, `CipherDecoder`.

## Decisions

**Layers.** `FrameDecoder` / `FrameEncoder` are synchronous codecs (own buffers, compression, cipher) usable without threads: the reader/writer threads, the fake server in tests, and benches all share them. `Connection` only adds sockets, threads and channels.

**Limits.** Length prefix is at most 3 bytes (vanilla `MAX_VARINT21_BYTES`), so frames are <= 2^21-1 bytes; zero length is rejected. Uncompressed size cap is 8388608.

**Compression backend.** `libdeflater` (libdeflate bindings). Frames carry their uncompressed size up front, so the one-shot, non-streaming API fits exactly: output is pre-sized to the declared length (also bounding zip bombs) and compressors are reused. The criterion bench `net` compares it against `flate2` with the `zlib-rs` backend; results are recorded under "Benchmark results". It costs a C toolchain at build time.

**Cipher.** Encryption uses `cfb8::Encryptor` (inherently serial). Decryption is a small custom `Cfb8Decryptor` over `aes`: each CFB8 keystream byte depends only on already-received ciphertext, so 64-byte chunks go through `encrypt_blocks` in parallel. This is ~10x faster than the `cfb8` crate's decryptor (about 25 MiB/s vs 280 MiB/s on small frames) and is verified against it in a test.

**Channels.** `crossbeam-channel`: bounded MPMC with cheaper wakeups than `std::mpsc`, `recv_timeout`, and the same type serves as the buffer pool return path.

**Buffer pool.** Inbound payloads are `PooledBuf`s that return their `Vec` to a bounded pool channel on drop (oversized buffers are dropped instead). Outbound frames use a second pool: the game thread fills a pooled `Vec` with id and payload, the writer returns it after encoding.

**Writer batching.** The writer blocks for one message, then drains the queue non-blockingly into one output buffer (up to 64 KiB), encrypts the pending region once, and issues one `write_all`.

**Switch ordering.**
- Outbound: compression and encryption switches are queue messages, so they apply after all previously queued frames and before later ones. Encryption starts at the byte offset where the switch was encountered; earlier bytes in the batch stay plaintext.
- Inbound compression: the server enables it with a normal frame and compresses everything after. The reader thread owns the decision: `ConnectOptions::inbound_hook` is called on the reader thread with `(packet_id, payload)` of every decoded frame and may return a `Switch`, applied before the next frame is parsed. No round trip to the game thread.
- Inbound encryption: the server only encrypts after receiving the client's key response and sends nothing in between. `send_and_enable_encryption(id, payload, key)` first arms the reader (atomic flag plus slot), then queues the frame, then queues the outbound switch. The reader checks the flag after each socket read and before decrypting, so bytes that arrive after the key frame hit the wire are always decrypted. The hook may also return `Switch::Encryption`, which decrypts already buffered bytes at the boundary.
- `enable_compression(threshold)` on the connection is outbound only; inbound is the hook's job.

**Errors and shutdown.** One shared once-cell records the first failure from either thread and shuts the socket down so the other thread unblocks. The reader then emits exactly one `Event::Closed(Arc<NetError>)`. Dropping or `close()`-ing the connection queues a shutdown message, flushes, and joins both threads; the event receiver is dropped first so a reader blocked on a full channel exits.

**DNS.** `hickory-resolver` is async-only and drags in tokio. Instead a small blocking SRV client sends one UDP query to the first nameserver from `/etc/resolv.conf` (public resolvers as fallback, e.g. on Windows), 2s timeout, parsing compressed names, ignoring truncation (no TCP). Every failure falls back silently. A/AAAA uses `std::net::ToSocketAddrs`.

## Risks

- Hand-rolled DNS ignores platform resolver config on Windows; acceptable, SRV is optional.
- The C dependency of libdeflater; swapping is local to one module.

## Benchmark results

`cargo bench -p mcr-net --bench net` (MiB/s, same machine, level 6, synthetic chunk-like data):

| size | libdeflater compress | zlib-rs compress | libdeflater decompress | zlib-rs decompress |
|---|---|---|---|---|
| 512 B | 61 | 47 | 350 | 228 |
| 16 KiB | 142 | 124 | 649 | 543 |
| 256 KiB | 108 | 102 | 984 | 589 |

libdeflater wins everywhere, most on decompression (up to 1.7x), which is the hot inbound path.

Decode pipeline (256 frames x 16 KiB): plain 25 GiB/s, zlib 673 MiB/s, zlib + AES 323 MiB/s.
