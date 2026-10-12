## ADDED Requirements

### Requirement: Server packs in Java
When mcr has applied a set of server resource packs, Java SHALL load the same pack files from mcr's downloads and SHALL drop them when they are popped, without downloading them again, prompting, or answering the server.

#### Scenario: Pack applied
- **WHEN** the server pushes a pack and mcr downloads and applies it
- **THEN** Java's resources include that pack and the server receives only mcr's answers

#### Scenario: Pack popped
- **WHEN** the server pops the pack
- **THEN** Java reloads without it

### Requirement: Quiet proxy refusals
Requests that the loopback web proxy refuses because their host is not allow-listed SHALL produce at most one short log line per logger and no stack trace or error toast.

#### Scenario: Update check
- **WHEN** a mod's update check asks for `api.modrinth.com`
- **THEN** the log shows one line naming the logger and the allow-list, and no exception
