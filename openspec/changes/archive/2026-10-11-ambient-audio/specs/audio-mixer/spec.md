## ADDED Requirements

### Requirement: Voice end reports
The mixer SHALL report voices that finish playing by themselves to the game thread, and SHALL NOT report voices removed by a stop command.

#### Scenario: Finished and stopped
- **WHEN** one voice runs out and another is stopped
- **THEN** only the first is reported
