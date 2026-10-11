## ADDED Requirements

### Requirement: Credits screen
Game event 4 SHALL open a screen scrolling the End poem (with the player name substituted), the credits and the post-credits text from the resource stack at 0.5 pixels per tick (0.75 without the poem), five times faster after Space. Escape or scrolling past the end SHALL close it by sending perform-respawn.

#### Scenario: Win game
- **WHEN** game event 4 arrives
- **THEN** the credits screen opens and `music.credits` plays

#### Scenario: Escape
- **WHEN** Escape is pressed
- **THEN** the screen closes and a perform-respawn client command is sent

#### Scenario: Scroll end
- **WHEN** the scroll passes the text height plus two screen heights
- **THEN** the screen closes the same way

#### Scenario: Hostile files
- **WHEN** the text files are huge or the credits JSON is malformed
- **THEN** the line count is bounded and malformed JSON adds nothing
