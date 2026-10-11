## ADDED Requirements

### Requirement: Now-playing toast
When the music toast option is `pause_and_toast` and music is audible, a started track SHALL slide a now-playing toast (note icon with cycling colour, translated title) in from the left for 5 seconds times the notification display time; it SHALL hide early when the track stops. Any non-`never` setting SHALL also draw the toast fixed in the top left of the pause screen while music is audible.

#### Scenario: Track starts
- **WHEN** a track starts with `pause_and_toast`
- **THEN** the toast slides in, holds 5 seconds and slides out

#### Scenario: Never
- **WHEN** the option is `never`
- **THEN** no toast is drawn anywhere

#### Scenario: Pause screen
- **WHEN** the pause screen is open with the option `pause`
- **THEN** the fixed toast is drawn and the sliding one is not
