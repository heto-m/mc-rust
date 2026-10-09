## ADDED Requirements

### Requirement: Frame format
The transport SHALL frame each packet as a VarInt length (at most 3 bytes, so at most 2^21-1) followed by that many bytes, where the body is a VarInt packet id plus payload. It SHALL reject zero-length frames, length prefixes wider than 3 bytes, and frames larger than 2^21-1 bytes.

#### Scenario: Oversize length prefix
- **WHEN** the peer sends a length prefix of four continuation-bit bytes
- **THEN** decoding fails with a corrupted-frame error

#### Scenario: Zero-length frame
- **WHEN** the peer sends a length of 0
- **THEN** decoding fails with a zero-length error

#### Scenario: Partial frames
- **WHEN** a frame arrives split across arbitrary read boundaries
- **THEN** it is decoded identically to one arriving whole

### Requirement: Threshold compression
When compression is enabled with threshold T >= 0, the transport SHALL prefix each frame body with a VarInt data length: 0 for bodies shorter than T sent uncompressed, otherwise the uncompressed size followed by the zlib stream. A negative threshold SHALL disable compression. The decoder SHALL reject declared sizes below T or above 8388608 and any inflated size differing from the declared size. The encoder SHALL reject bodies above 8388608.

#### Scenario: Threshold boundary
- **WHEN** a body of exactly T bytes and one of T-1 bytes are encoded
- **THEN** the first is compressed and the second is sent with data length 0

#### Scenario: Size mismatch
- **WHEN** a compressed frame inflates to a size different from its declared size
- **THEN** decoding fails

### Requirement: Stream encryption
After encryption is enabled with a 16-byte shared secret, every byte in both directions SHALL be processed with AES-128-CFB8 using the secret as key and IV, in place, with the cipher state carried across frames.

#### Scenario: Known answer
- **WHEN** the NIST SP 800-38A CFB8-AES128 vector is encrypted
- **THEN** the output matches the published ciphertext

### Requirement: Exact switch boundaries
Compression and encryption switches SHALL take effect at exact frame boundaries. Outbound switches apply after every previously queued frame. Inbound switches apply on the reader thread, either immediately after a frame selected by a caller-provided hook, or from an armed encryption key before any further bytes are decrypted.

#### Scenario: Compression enabled by the last uncompressed frame
- **WHEN** the server sends a frame enabling compression followed immediately by compressed frames
- **THEN** the hook switches the reader after the enabling frame and all following frames decode

#### Scenario: Encryption enabled mid-stream
- **WHEN** the client sends a frame with `send_and_enable_encryption`
- **THEN** that frame is plaintext, all later outbound bytes are encrypted, and all later inbound bytes are decrypted

### Requirement: Threaded connection
`Connection::connect` SHALL open a TCP connection with TCP_NODELAY and spawn one reader and one writer thread. The game thread SHALL receive `Event`s (frames with pooled payload buffers, or a final `Closed` carrying the reason) via `try_recv` and `recv_timeout`, and SHALL send frames with `send`. The writer SHALL batch queued frames into a single write.

#### Scenario: Peer disconnects
- **WHEN** the server closes the socket
- **THEN** the game thread receives exactly one `Closed` event with a reason, and later `send` calls fail

#### Scenario: Graceful shutdown
- **WHEN** the connection is closed locally
- **THEN** queued frames are flushed, both threads exit, and the socket is closed
