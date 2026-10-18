## ADDED Requirements

### Requirement: Canonical servers only
Mod mode SHALL join only servers on the canonical protocol. A join whose wire is not canonical SHALL fail before login with a message saying mod mode needs a 26.3 server and naming the server's version.

#### Scenario: Mods and a 26.2 server
- **WHEN** `mcr --mods` connects to a 26.2 server
- **THEN** the join fails before login with "Mod mode needs a 26.3 server (this one runs 26.2)"
