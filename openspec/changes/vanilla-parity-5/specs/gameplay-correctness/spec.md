## ADDED Requirements

### Requirement: Placement state prediction
Block placement prediction SHALL use the block's placement state, including slab merging, and SHALL test
that state's collision against every entity before predicting.

#### Scenario: Stairs
- **WHEN** the player places oak stairs on the top half of a block's side
- **THEN** the predicted state is upside-down stairs facing away from the player, shown before the server
  answers

### Requirement: Adventure predicates on block entity data
`can_break` and `can_place_on` predicates SHALL match block entity NBT and data components as vanilla's
`BlockPredicate` does.

#### Scenario: Named chest only
- **WHEN** an adventure player holds a pickaxe that may break only chests with a given custom name
- **THEN** mining another chest is not predicted

### Requirement: Loyal trident return
A loyal trident SHALL start returning only after it dealt damage, spent more than four ticks in the ground
or lost its physics.

#### Scenario: Trident in the ground
- **WHEN** a loyal trident sticks in a block
- **THEN** it starts returning on its fifth tick in the ground
