## ADDED Requirements

### Requirement: Compatibility record
`docs/mod-compat.md` SHALL list every swept mod with its version, a status of works, partial, broken or blocked, and a one-line reason naming the root cause.

#### Scenario: Reading a result
- **WHEN** a user looks up a swept mod
- **THEN** the table gives its status and why

### Requirement: Sweep procedure
A sweep SHALL run real mod jars through the `mcr --mods` binary offscreen against a vanilla server, each mod alone with its libraries and then all together, recording boot, join, the main feature and Java log errors. Mod jars SHALL stay out of the repository, and regressions found SHALL be covered by tests driven by the test mod rather than by downloaded jars.

#### Scenario: Fix from a sweep
- **WHEN** a sweep finds a bridge gap and it is fixed
- **THEN** a test that uses the test mod reproduces the gap and passes with the fix
