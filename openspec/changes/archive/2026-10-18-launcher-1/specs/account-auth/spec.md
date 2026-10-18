## ADDED Requirements

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
