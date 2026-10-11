## ADDED Requirements

### Requirement: Creative flight with an elytra
A player that is creative flying SHALL NOT start gliding (`Player.canGlide` requires not flying), so double-tapping jump with an elytra equipped toggles creative flight like vanilla's `LocalPlayer.aiStep`. Every client-side change of the flying ability (double tap, landing, the spectator force) SHALL be reported with `ServerboundPlayerAbilities`.

#### Scenario: Double tap with an elytra
- **WHEN** a creative player wearing an elytra double-taps jump on the ground
- **THEN** creative flight turns on and no glide starts
