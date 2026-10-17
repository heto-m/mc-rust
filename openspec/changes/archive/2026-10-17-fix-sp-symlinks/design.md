# Design

`std::fs::remove_dir_all` has not followed symlinks since the CVE-2022-21658 fix: it unlinks a link it meets
instead of descending, and fails rather than racing when a directory is swapped for a link. A test plants a
link to an outside directory inside a world and checks the outside file survives the delete.

Checks run before every launch (bounded walk of `mcr-server`: 20,000 entries, depth 8), and each write
re-checks its own target, so a link planted between the check and the write is still refused: the temp file
is created with `create_new` and `O_NOFOLLOW`, and `rename` replaces a directory entry, never a link's target.
