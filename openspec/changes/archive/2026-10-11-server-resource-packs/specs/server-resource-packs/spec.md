## ADDED Requirements

### Requirement: Server pack protocol handling
The client SHALL handle `resource_pack_push` and `resource_pack_pop` in the configuration and play states, answering with `resource_pack` statuses in vanilla's sequence: `ACCEPTED` then `DOWNLOADED` then `SUCCESSFULLY_LOADED`, or `INVALID_URL`, `FAILED_DOWNLOAD`, `FAILED_RELOAD`, `DECLINED`, `DISCARDED`.

#### Scenario: Non-http URL
- **WHEN** a push carries a URL whose scheme is not `http` or `https`
- **THEN** the client replies `INVALID_URL` and downloads nothing

#### Scenario: Pop
- **WHEN** a pop names a pack id (or none, meaning all)
- **THEN** the matching packs are removed and assets reload without them

#### Scenario: Replaced pack
- **WHEN** a push reuses the id of an active pack
- **THEN** the old pack is replaced silently and only the new pack reports statuses

### Requirement: Per-server pack setting
Each server entry SHALL have a "Server Resource Packs" setting of Prompt, Enabled or Disabled persisted in `servers.dat`.

#### Scenario: Enabled
- **WHEN** the setting is Enabled
- **THEN** pushed packs are accepted without a prompt

#### Scenario: Disabled
- **WHEN** the setting is Disabled and the pack is optional
- **THEN** the client replies `DECLINED`

#### Scenario: Prompt and answer
- **WHEN** the setting is Prompt, or Disabled with a required pack
- **THEN** a confirm screen shows the server prompt; Yes stores Enabled and accepts, No stores Disabled and declines

#### Scenario: Required pack declined
- **WHEN** a required pack is declined at the prompt
- **THEN** the client disconnects with `multiplayer.requiredTexturePrompt.disconnect`

### Requirement: Bounded verified download
Pack downloads SHALL use only http(s), SHALL abort beyond 250 MiB or the time limits, SHALL verify the SHA-1 when the server supplied a valid one, and SHALL cache files by hash under `downloads/`, evicting the oldest beyond 20 entries.

#### Scenario: Hash mismatch
- **WHEN** the downloaded bytes do not match the supplied SHA-1
- **THEN** the file is discarded and the client replies `FAILED_DOWNLOAD`

#### Scenario: Cache hit
- **WHEN** a cached file with the requested hash verifies
- **THEN** no request is made and `DOWNLOADED` follows `ACCEPTED`

#### Scenario: Oversize
- **WHEN** the server announces or sends more than 250 MiB
- **THEN** the download aborts with `FAILED_DOWNLOAD`

### Requirement: Hostile archive safety
Server pack archives SHALL be opened with limits on entry count, total and per-entry uncompressed size, and entries with traversal, absolute or backslash paths SHALL be ignored. Malformed JSON or images in a pack SHALL be logged and replaced by fallbacks, never panic.

#### Scenario: Zip bomb
- **WHEN** an archive declares more than the limits allow
- **THEN** it is rejected and the client replies `FAILED_RELOAD`

#### Scenario: Traversal entry
- **WHEN** an entry is named `../x` or `/x`
- **THEN** it is not indexed and cannot be read

### Requirement: Live reload
Applying or removing packs SHALL rebuild the resource stack (vanilla jar, asset index, packs in push order) and reload models, textures, sounds, language, fonts, item models and GUI atlases without restarting, remeshing every loaded section.

#### Scenario: Retextured block
- **WHEN** a pack replaces the stone texture and is applied
- **THEN** stone renders with the new texture in the loaded world

#### Scenario: Failed reload
- **WHEN** building assets from the pack list fails
- **THEN** the previous assets stay active and `FAILED_RELOAD` is sent
