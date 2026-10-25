## ADDED Requirements

### Requirement: Differential harnesses
Each client subsystem SHALL have a harness that feeds the same generated cases to vanilla's code from the user's client jar and to ours, and reports every case whose normalized outputs differ.

#### Scenario: Mismatch report
- **WHEN** a harness finds cases where vanilla and mcr differ
- **THEN** it fails and prints the first mismatches with the case, both outputs and the path to the first differing field

#### Scenario: Matching run
- **WHEN** every case matches
- **THEN** the harness passes and records its case count

### Requirement: Offline and contained oracles
Oracle runs SHALL run without network access inside the memory-capped scope, and their outputs SHALL stay under `.cache/`.

#### Scenario: No network
- **WHEN** an oracle runs
- **THEN** it runs in a network namespace with no interfaces but loopback

### Requirement: Oracle caching
An oracle run SHALL be reused while the client jar, the oracle's classes and the case file are unchanged.

#### Scenario: Rust-only change
- **WHEN** only Rust sources change and a harness reruns
- **THEN** the JVM does not start and the cached vanilla output is diffed against the new probe output

### Requirement: Draw-stream comparison
The client SHALL be comparable with vanilla per screen, HUD state and fixed-camera world frame through a normalized draw stream recorded from both clients.

#### Scenario: Screen draw order
- **WHEN** the inventory screen is open at GUI scale 2 in both clients
- **THEN** their GUI draw streams match draw by draw

### Requirement: Parity gate
A branch SHALL NOT merge if it makes a passing harness fail.

#### Scenario: Regression
- **WHEN** a change makes a passing harness fail
- **THEN** `merge.sh` stops and names the harness
