## ADDED Requirements

### Requirement: Brigadier feedback and click events
The command box SHALL colour arguments, show parse errors and argument usage, show a ghost suggestion and
honour `autoSuggestions`. Plain chat SHALL complete player names on Tab. Chat click events SHALL follow
vanilla: run_command without a prompt when it parses, show_dialog, custom actions and silent copy.

#### Scenario: Trigger link
- **WHEN** the player clicks a chat link running `/trigger menu`
- **THEN** the command is sent without a confirm screen
