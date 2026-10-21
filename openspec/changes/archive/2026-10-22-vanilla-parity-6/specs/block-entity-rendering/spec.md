## ADDED Requirements

### Requirement: Beacon beam cadence
A beacon beam SHALL be scanned one step per tick as vanilla's `BeaconBlockEntity#tick` does.

#### Scenario: Extending the beam
- **WHEN** the block above a beam is removed
- **THEN** the beam lengthens at vanilla's rate
