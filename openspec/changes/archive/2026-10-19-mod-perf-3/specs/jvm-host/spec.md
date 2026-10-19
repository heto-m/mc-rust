## ADDED Requirements

### Requirement: Quit on SIGTERM
On Linux, `SIGTERM` SHALL make a running mcr quit through its normal shutdown, so that Java's AOT recording, class list and preload record are written as on a user quit.

#### Scenario: Terminated session
- **WHEN** a windowed mod-mode session receives `SIGTERM`
- **THEN** mcr quits cleanly, `DestroyJavaVM` runs, and the next start does not count a preload strike
