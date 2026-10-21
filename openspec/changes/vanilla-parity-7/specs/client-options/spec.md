## ADDED Requirements

### Requirement: Vanilla debug entries
The debug options screen SHALL list every vanilla debug entry the client has data for, and the F3 text
SHALL follow vanilla's entry order.

#### Scenario: Targeted block tags
- **WHEN** the player enables `looking_at_block_tags` and looks at oak logs
- **THEN** the F3 text lists the block's tags

### Requirement: Scrolling World Options
The World Options rows SHALL scroll when they do not fit the screen.

#### Scenario: Small GUI
- **WHEN** World Options opens at a GUI height under 240 px
- **THEN** every row can be reached by scrolling
