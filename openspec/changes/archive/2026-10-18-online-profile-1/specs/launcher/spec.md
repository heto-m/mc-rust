## MODIFIED Requirements

### Requirement: Account refresh at launch
At launch a Microsoft account SHALL be refreshed without prompting and its token and UUID SHALL be used for online-mode servers; a rejected refresh SHALL ask the user to sign in again. The launch SHALL also pass the account's profile keys, Mojang's services keys and a key refresher to the client; failing to fetch keys or services keys SHALL NOT fail the launch. Offline accounts SHALL pass none.

#### Scenario: Refresh at launch
- **WHEN** Play is pressed with a Microsoft account whose Minecraft token expired
- **THEN** the token is refreshed from the refresh token and the game joins online-mode servers with it

#### Scenario: Keys reach the client
- **WHEN** Play is pressed with a Microsoft account whose certificates are fetched
- **THEN** the client config holds those keys and the services keys, and their debug output hides the private key
