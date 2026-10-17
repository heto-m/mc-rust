## ADDED Requirements

### Requirement: No writes through symlinks
World folders SHALL be treated as untrusted. A world SHALL be a real directory directly inside the canonical saves folder. A launch SHALL refuse a world whose folder, `level.dat`, `level.dat_old`, `session.lock` or anything under `mcr-server` is a symlink. mcr SHALL write world files only through a new temp file (no-follow) renamed over a non-link target, and read them with no-follow opens.

#### Scenario: Planted ops.json link
- **WHEN** a downloaded world ships `mcr-server/ops.json` as a symlink to a file elsewhere
- **THEN** the world does not start, a failure screen names the link, and the other file is unchanged

#### Scenario: Delete with an inner link
- **WHEN** a world containing a symlink to an outside directory is deleted
- **THEN** only the link is removed and the outside directory keeps its files
