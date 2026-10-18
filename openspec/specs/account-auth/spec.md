# account-auth Specification

## Purpose
Authenticate a Minecraft account (Microsoft, Xbox, Minecraft services), cache it, join online-mode servers, and provide offline identities.

## Requirements

### Requirement: Microsoft device code login
The system SHALL authenticate a Microsoft account via the OAuth2 device code flow with scope `XboxLive.signin offline_access`, using a client ID supplied by configuration or the `MCR_MSA_CLIENT_ID` environment variable, and SHALL NOT embed any default client ID.

#### Scenario: Pending then approved
- **WHEN** the token endpoint answers `authorization_pending` and then returns tokens
- **THEN** polling continues until tokens are returned

#### Scenario: Missing client ID
- **WHEN** no client ID is configured
- **THEN** login fails with a typed configuration error

#### Scenario: Denied or expired
- **WHEN** the endpoint reports `authorization_declined` or `expired_token`
- **THEN** login fails with a typed error

### Requirement: Xbox Live and Minecraft token chain
The system SHALL exchange the Microsoft token for an Xbox user token, an XSTS token for relying party `rp://api.minecraftservices.com/`, and a Minecraft access token, then verify game ownership and fetch the profile.

#### Scenario: XSTS error mapping
- **WHEN** XSTS returns XErr 2148916233 or 2148916238
- **THEN** a distinct typed error (no Xbox account / child account) is returned

#### Scenario: No entitlement
- **WHEN** the entitlements list contains no game ownership
- **THEN** a typed not-owned error is returned

### Requirement: Profile key pair
The system SHALL fetch and parse the player certificate response into an RSA private key, public key DER, expiry and signatures. When fetching new keys fails, unexpired cached keys SHALL be kept and expired ones dropped, without failing the sign-in or refresh. The private key SHALL never appear in debug output.

#### Scenario: Parse certificates
- **WHEN** a valid certificates response is parsed
- **THEN** the private key, public key, expiry and both signatures are exposed

#### Scenario: Certificate service down
- **WHEN** the token is refreshed but the certificates request fails and the cached keys have expired
- **THEN** the refresh succeeds with no keys

### Requirement: Token cache
The system SHALL persist account state as JSON with expiry times at a caller-chosen path and SHALL refresh expired tokens automatically. Secrets SHALL NOT appear in Debug output.

#### Scenario: Valid cache
- **WHEN** the cached Minecraft token is unexpired
- **THEN** no network request is made

#### Scenario: Expired cache
- **WHEN** the Minecraft token is expired and a refresh token exists
- **THEN** the chain is re-run from the refresh token and the cache is rewritten

### Requirement: Session join
The system SHALL compute the Minecraft server hash as the signed two's-complement hexadecimal SHA-1 of server ID, shared secret and server public key DER, and post it to the session server join endpoint.

#### Scenario: Known vectors
- **WHEN** hashing "Notch", "jeb_" and "simon"
- **THEN** results are `4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48`, `-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1`, `88e16a1019277b15d58faf0541e11910eb756f6`

### Requirement: Encryption handshake helpers
The system SHALL generate a 16-byte random shared secret and encrypt it and the verify token with the server's DER public key using RSA PKCS#1 v1.5.

#### Scenario: Round trip
- **WHEN** the secret is encrypted with a public key
- **THEN** the matching private key decrypts it to the original

### Requirement: Offline identity
The system SHALL derive an offline UUID as version-3 UUID of MD5 over `OfflinePlayer:<name>` and validate player names.

#### Scenario: Derivation
- **WHEN** the name is valid
- **THEN** the UUID has version 3 and variant RFC 4122 and is deterministic

#### Scenario: Invalid name
- **WHEN** the name is empty, longer than 16 characters or has characters outside printable ASCII without spaces
- **THEN** a typed error is returned

### Requirement: Cancellable device sign-in
The system SHALL offer a device-code sign-in whose wait callback can cancel polling, failing with a typed cancelled error before the next token request, and SHALL otherwise run the same chain as the device login (Xbox, XSTS, Minecraft, entitlements, profile, keys).

#### Scenario: Cancelled while pending
- **WHEN** the token endpoint answers `authorization_pending` and the wait callback then returns false
- **THEN** sign-in fails with the cancelled error and no further request is sent

### Requirement: Non-interactive refresh
The system SHALL refresh a cached account without ever starting a device login: a valid token SHALL be returned without network, an expired one SHALL be refreshed and rewritten, and a rejected refresh token or a missing cache SHALL fail with a typed error.

#### Scenario: Rejected refresh
- **WHEN** the cached token expired and the refresh token is rejected with `invalid_grant`
- **THEN** the refresh fails with the refresh-rejected error and no device code is requested

### Requirement: Mojang services keys
The system SHALL fetch Mojang's player-certificate public keys from `/publickeys` (at most 16 keys, each a base64 SPKI RSA key) and SHALL validate a profile public key as vanilla does: SHA1withRSA over profile UUID, expiry in milliseconds and key DER, valid when any services key verifies it.

#### Scenario: Valid profile key
- **WHEN** a profile key is signed by a test services key over the profile's UUID, expiry and DER
- **THEN** validation succeeds, and fails for another UUID or expiry

### Requirement: Chat signing primitives
The system SHALL sign and verify chat bodies with SHA256withRSA (PKCS#1 v1.5) over vanilla's signed message layout, producing 256-byte signatures for 2048-bit keys, and SHALL reject signatures of any other length.

#### Scenario: Round trip
- **WHEN** a body is signed with a test private key and verified with its public key
- **THEN** verification succeeds, and fails after changing one byte of the content
