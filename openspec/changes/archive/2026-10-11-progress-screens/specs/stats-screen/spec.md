## ADDED Requirements

### Requirement: Statistics screen

The client SHALL request statistics with `ClientCommand` when the screen opens and show them in the vanilla general, item and mob tabs.

#### Scenario: Opening statistics

- **WHEN** the player opens the statistics screen
- **THEN** the client sends the stats request and shows the values from the next `AwardStats`
