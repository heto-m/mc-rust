# command-suggestions Specification

## Purpose
Client-side Brigadier command tree from the commands packet with local completion and server suggestion round trips.

## Requirements

### Requirement: Command tree
The client SHALL build a bounded command tree from the `commands` packet, ignoring out-of-range child and redirect indices and following redirects with a bounded depth.

#### Scenario: Hostile indices
- **WHEN** a node references a child index outside the node list
- **THEN** the reference is ignored and no panic occurs

### Requirement: Local suggestions
The client SHALL suggest literals, booleans and enumerated argument values from the tree for the text before the cursor, and report usage for the next argument when nothing completes.

#### Scenario: Literal completion
- **WHEN** the input is `/gam`
- **THEN** `gamemode` and `gamerule` are suggested replacing `gam`

### Requirement: Server suggestions
The client SHALL send `command_suggestion` for arguments marked as asking the server and apply the response only if its id matches the latest request.

#### Scenario: Stale response
- **WHEN** a response arrives with an older id
- **THEN** it is discarded

### Requirement: Brigadier feedback and click events
The command box SHALL colour arguments, show parse errors and argument usage, show a ghost suggestion and
honour `autoSuggestions`. Plain chat SHALL complete player names on Tab. Chat click events SHALL follow
vanilla: run_command without a prompt when it parses, show_dialog, custom actions and silent copy.

#### Scenario: Trigger link
- **WHEN** the player clicks a chat link running `/trigger menu`
- **THEN** the command is sent without a confirm screen
