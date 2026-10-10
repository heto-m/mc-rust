## ADDED Requirements

### Requirement: Headless binary
The `mcr-headless` binary SHALL connect to a server and run scripted behaviours without any renderer.

#### Scenario: Idle join
- **WHEN** run with `--addr` and `--seconds N`
- **THEN** it joins, reports position each second and exits cleanly after N seconds

#### Scenario: Script
- **WHEN** run with `--script steps.txt`
- **THEN** each step (walk, jump, chat, command, dig, place, inventory, wait) executes in order
