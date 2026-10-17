## ADDED Requirements

### Requirement: Passenger seats
A passenger SHALL stand at its vehicle's position plus the vehicle's passenger attachment for the passenger's index in the vehicle's passenger list, minus the passenger's own vehicle attachment, each scaled by age and the `scale` attribute. This SHALL apply to the local player and to remote riders. Boats, camels and happy ghasts SHALL use their per-seat positions rotated by the vehicle yaw.

#### Scenario: Player on a horse
- **WHEN** the local player rides an adult horse
- **THEN** the player's feet are 1.44375 - 0.6 blocks above the horse's position

#### Scenario: Two in a boat
- **WHEN** two players sit in an oak boat facing south
- **THEN** the first sits 0.2 blocks ahead of the boat's centre and the second 0.6 blocks behind it
