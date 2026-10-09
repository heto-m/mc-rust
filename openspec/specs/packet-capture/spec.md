# packet-capture Specification

## Purpose
Record real server traffic as byte-exact per-packet fixture files with an index, driving the join flow and optionally a managed server console.

## Requirements

### Requirement: Fixture layout
The tool SHALL write each recorded frame's decompressed payload, without the packet id, to `<out>/<state>/<direction>/<packet_name>/<seq>.bin`, and SHALL append one JSON line per recorded frame to `<out>/index.jsonl` carrying seq, state, direction, id, name, length and timestamp.

#### Scenario: Named packet
- **WHEN** a clientbound play packet with a known id is received
- **THEN** its payload is stored under the play/clientbound directory named after the packet and an index line is appended

#### Scenario: Unknown id
- **WHEN** a packet id has no name in the game data
- **THEN** it is stored under a name derived from the numeric id

### Requirement: Sample caps
The tool SHALL record at most a fixed number of samples per packet name, except for packets whose layouts are large or varied, which SHALL be kept in larger numbers or in full.

#### Scenario: Chatty packet
- **WHEN** more than the cap of a capped packet name arrive
- **THEN** only the first samples up to the cap are stored

#### Scenario: Registry data
- **WHEN** the server sends registry data
- **THEN** every such packet is stored

### Requirement: Protocol walk
The tool SHALL join as an offline-mode player by sending handshake with login intent and login hello, honour login compression, acknowledge login, answer the configuration exchange, enter play, and keep the session alive by echoing keep-alives and pings, confirming teleports and chunk batches, and returning to configuration when the server requests it.

#### Scenario: Join and stay
- **WHEN** the tool connects to an offline-mode server
- **THEN** it reaches play and remains connected for the requested duration

### Requirement: Known-packs modes
By default the tool SHALL echo the server's known packs back. With the no-known-packs option it SHALL reply with an empty list so the server sends complete registry data.

#### Scenario: No known packs
- **WHEN** the option is set
- **THEN** the server's registry data packets carry explicit entries and are recorded

### Requirement: Console script
When it spawns the server, the tool SHALL wait for the server to finish starting and then feed each line of a script file to the server console, honouring sleep lines and substituting the player name.

#### Scenario: Scripted commands
- **WHEN** a script file is supplied with a server spawn
- **THEN** its commands reach the server console while the capture runs
