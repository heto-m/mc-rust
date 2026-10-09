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
The system SHALL fetch and parse the player certificate response into an RSA private key, public key DER, expiry and signatures.

#### Scenario: Parse certificates
- **WHEN** a valid certificates response is parsed
- **THEN** the private key, public key, expiry and both signatures are exposed

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
