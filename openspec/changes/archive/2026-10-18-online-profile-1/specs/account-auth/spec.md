## MODIFIED Requirements

### Requirement: Profile key pair
The system SHALL fetch and parse the player certificate response into an RSA private key, public key DER, expiry and signatures. When fetching new keys fails, unexpired cached keys SHALL be kept and expired ones dropped, without failing the sign-in or refresh. The private key SHALL never appear in debug output.

#### Scenario: Parse certificates
- **WHEN** a valid certificates response is parsed
- **THEN** the private key, public key, expiry and both signatures are exposed

#### Scenario: Certificate service down
- **WHEN** the token is refreshed but the certificates request fails and the cached keys have expired
- **THEN** the refresh succeeds with no keys

## ADDED Requirements

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
