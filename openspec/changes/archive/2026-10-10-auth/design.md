## Context

Networking is blocking with no async runtime. HTTP must be mockable and tests must not hit the network.

## Decisions

- `HttpClient` trait (`send(Request) -> Response`) with a `ureq` implementation; tests use a scripted mock matching on URL.
- Modules: `http`, `msa`, `xbox`, `minecraft`, `keys`, `cache`, `session`, `crypt`, `offline`, `error`, `secret`.
- `Secret` newtype with redacting Debug, serde transparent.
- Hash via `sha1` crate and manual two's-complement negation on the 20-byte digest.
- RSA via `rsa` crate (PKCS#1 v1.5 encrypt; private key parsed from PKCS#8 PEM, falling back to PKCS#1).
- Time is injected (unix seconds) for testable expiry.
- Offline UUID via MD5 and `uuid::Builder::from_md5_bytes`.

## Risks

Azure client ID must be registered by the owner and approved for Minecraft API access; flows are verified only against fixtures.
