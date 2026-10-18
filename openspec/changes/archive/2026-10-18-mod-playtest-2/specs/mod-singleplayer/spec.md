## ADDED Requirements

### Requirement: Difficulty in Java's World Options
In a local mcr world, Java's World Options SHALL let the owner change and lock the difficulty as vanilla singleplayer does. A change SHALL reach mcr's managed server; a lock SHALL hold in both clients for the rest of the session and be stored in the world's `level.dat` once its server stopped. Hardcore worlds and locked worlds SHALL keep the difficulty controls inactive, and Rust SHALL refuse changes for them.

#### Scenario: Change difficulty
- **WHEN** the owner of a local world without cheats sets Hard in World Options and applies it
- **THEN** the server's difficulty becomes Hard and both clients show Hard

#### Scenario: Lock difficulty
- **WHEN** the owner locks the difficulty and confirms
- **THEN** the difficulty controls turn inactive, further change requests are refused, and the next session of that world starts locked

#### Scenario: Hardcore
- **WHEN** the world is hardcore
- **THEN** the difficulty button and lock stay inactive and a change request is refused
