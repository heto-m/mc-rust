# toasts Specification

## Purpose
Top-right toast queue with slide timing, five slots, sounds, and advancement, recipe and system toasts.

## Requirements

### Requirement: Toast manager
The system SHALL show toasts at the top right with a 600 ms squared slide in and out, at most five 32-pixel slots occupied at once, and queue the rest in arrival order.

#### Scenario: Queue
- **WHEN** a sixth single-slot toast is added while five are visible
- **THEN** it waits until a slot frees and then slides in

### Requirement: Toast sounds
The system SHALL play `ui.toast.in` and `ui.toast.out` on visibility changes, at most once per update, and `ui.toast.challenge_complete` when a challenge toast appears.

#### Scenario: One sound per update
- **WHEN** two toasts change visibility in the same update
- **THEN** a single visibility sound is played

### Requirement: Advancement toasts
The system SHALL queue a toast, framed as task, goal or challenge, when an advancement with `show_toast` becomes complete in a non-reset update.

#### Scenario: Not on login
- **WHEN** the initial advancement sync arrives
- **THEN** no toast is queued

### Requirement: Recipe toasts
The system SHALL show one recipe toast, cycling the unlocked results, for recipe book entries flagged for notification.

#### Scenario: Merged
- **WHEN** a second notifying recipe arrives while a recipe toast is visible
- **THEN** it is added to that toast instead of a new one

### Requirement: System toasts
The system SHALL support titled system toasts with an optional message that size to their text.

#### Scenario: Wide text
- **WHEN** a system toast message is wider than the minimum toast
- **THEN** the toast grows to fit it

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
