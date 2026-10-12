## ADDED Requirements

### Requirement: Game directory as working directory
Before creating the VM, the bridge SHALL make every launch path absolute, set `user.dir` to the game directory and make it the process's working directory, so mods' relative paths land in the game directory.

#### Scenario: Mod writes a relative config
- **WHEN** a mod writes `config/<name>` through a relative `java.io` path at startup
- **THEN** the file is created under the game directory and nowhere else
