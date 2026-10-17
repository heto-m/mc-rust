# Proposal

## Why

A world folder can come from a download. Before this change mcr followed symlinks inside it: a world shipping
`mcr-server` (or `eula.txt`, `server.properties`, `ops.json`, `logs/console.log`, `server.pid`,
`level.dat_old`, `session.lock`) as a symlink would make mcr write or read files outside the world.

## What Changes

- New `mcr_singleplayer::safe` module: worlds are canonicalised and must be real directories directly inside
  the canonical saves folder; directories are created one level at a time and never through a link; files are
  written to a fresh temp file (`create_new`, `O_NOFOLLOW`) and renamed over the target, refusing a symlinked
  target; reads open with `O_NOFOLLOW`.
- A launch refuses, with the failure screen, a world whose folder, `level.dat`, `level.dat_old`,
  `session.lock` or anything under `mcr-server` is a symlink. New worlds get a fresh folder (`create_dir`).
- Rename writes `level.dat_old` and `level.dat` the same way; delete refuses a symlinked world folder, and
  `remove_dir_all` unlinks links inside the world without following them (tested).
- The pid file and the server's console log go through the same checks.
- Tests: symlinked world, `mcr-server`, planted files, `level.dat_old`, links inside a deleted world, and world
  names with `..` or separators.

## Out of scope

Files the vanilla server itself writes (it validates symlinks in the level folder on its own) and the
player's game directory, which the player controls.
