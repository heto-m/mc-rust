## Why

The client needs an authenticated Minecraft identity to join online-mode servers: Microsoft login, Minecraft profile, chat-signing keys, session join and the login encryption handshake. Offline mode is needed for local testing.

## What Changes

- New crate `mcr-auth` with blocking HTTP (`ureq`, rustls) behind a mockable transport trait.
- Microsoft OAuth2 device code flow and refresh-token flow; client ID supplied by the caller or `MCR_MSA_CLIENT_ID`.
- Xbox Live user auth, XSTS (typed XErr errors), Minecraft login, entitlement check, profile.
- Profile key pair (`/player/certificates`) parsed with an RSA private key.
- JSON token cache with expiry and automatic refresh; secrets redacted from `Debug`.
- Session server join with the Minecraft-style signed SHA-1 server hash.
- Encryption handshake helpers: shared secret generation, RSA PKCS#1 v1.5 encryption.
- Offline-mode UUID derivation and name validation.
- Example binary `examples/login.rs`.

## Capabilities

### New Capabilities
- `account-auth`: Microsoft/Xbox/Minecraft authentication, token cache, session join, encryption handshake helpers and offline identity.

### Modified Capabilities

None.

## Impact

New crate `crates/mcr-auth`; shared deps appended to workspace `Cargo.toml`.

## Out of scope

Registering an Azure app (the owner must register one and request Minecraft API access), Bedrock/Realms auth, packet encoding, the actual stream cipher, network I/O in tests.
