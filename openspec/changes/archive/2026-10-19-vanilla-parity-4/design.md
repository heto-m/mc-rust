## Context

A read-only audit compared mcr's native client (not the mod-mode Java GUI) against the decompiled 26.3
client, subsystem by subsystem. Every gap below was confirmed by reading both sides. Vanilla references
are `Class#method` identifiers only. mcr references are `path:line` at commit `fd550b0`. Severity:

- **G**: gameplay-breaking. A feature does not work, the player is stuck, or the server rubber-bands or
  rejects what the client did.
- **V**: visible. A player notices that it looks, sounds or behaves differently from vanilla.
- **M**: minor. A small or rare difference.

Each gap has an id that `tasks.md` refers to. The ids are grouped by subsystem: P physics, I interaction,
C containers, H HUD, E entities/hand/camera, R world rendering, N packet handling, S sound, chat, options
and screens.

## Decisions

- One task per gap, grouped into batches that one implementation agent can finish. The batches go from
  highest impact to lowest. Gaps that share code are kept in the same batch even if their severity
  differs, so two agents never edit the same function.
- Gaps reported by more than one subsystem audit are listed once, under the subsystem that owns the fix.
  The other places point to that id.
- When the fix needs a missing state (sleeping, spectator menu, use hand), the batch adds the state first
  and then the features that read it.
- Out of scope: mod-mode Java GUI parity, demo mode, Realms and telemetry, and the debug renderers.

## Gap catalogue

### P: player movement and physics

#### P1 No collision with entities (G)
- Vanilla: `Entity#collide` and `Level#noCollision` include `getEntityCollisions`. `AbstractBoat`, `Shulker`
  and `HappyGhast` return true from `canBeCollidedWith`.
- mcr: `crates/mcr-physics/src/world.rs:1-29` (`CollisionWorld`) exposes only block states.
  `Colliders::gather` (`crates/mcr-physics/src/collide.rs:200-219`) gathers block shapes only. The same
  applies to pose fitting and the sneak edge check (`mover.rs:58-60`, `tick.rs:69-72`).
- Impact: you fall through boats, shulkers and a harnessed happy ghast's top, then the server teleports you.

#### P2 World border is not solid (V)
- Vanilla: `Entity#collectColliders` adds `WorldBorder#getCollisionShape` when
  `WorldBorder#isInsideCloseToBorder` holds.
- mcr: no border term in `collide.rs` or `mover.rs`. `WorldBorder` (`crates/mcr-client/src/state.rs:76`)
  feeds only rendering and the dig check.
- Impact: you walk through the border and the server rubber-bands you back.

#### P3 Entities never push the local player (V)
- Vanilla: `LivingEntity#pushEntities` and `AbstractBoat#tick` call `Entity#push`. On the client
  `EntitySelector#pushableBy` lets remote entities push the local player.
- mcr: nothing adds push velocity. The only writers of `physics.vel` are
  `crates/mcr-client/src/handler.rs:270` (explosion), `handler.rs:450` (motion) and
  `crates/mcr-client/src/entity_events.rs:81,109`.
- Impact: you walk through crowds and are never shoved by mobs or boats.

#### P4 No push-out of suffocating blocks (V)
- Vanilla: `LocalPlayer#aiStep` calls `moveTowardsClosestSpace` at four corners (±0.35 width) using
  `suffocatesAt`.
- mcr: `crates/mcr-physics/src/tick.rs:103-208` (`ai_step`) has no equivalent.
- Impact: you stay stuck in a block instead of being nudged out.

#### P5 Auto-jump option does nothing (V)
- Vanilla: `LocalPlayer#updateAutoJump` sets `autoJumpTime`, and `aiStep` then calls `Input#makeJump`.
- mcr: `auto_jump` (`crates/mcr-settings/src/options.rs:113`) is never read. Physics has no auto-jump.

#### P6 Blindness does not block or stop sprinting (V)
- Vanilla: `LocalPlayer#isSprintingPossible` requires `!Player#isMobilityRestricted`, which is true under
  BLINDNESS.
- mcr: `crates/mcr-physics/src/tick.rs:98-101` checks only food, may-fly and shallow water. Neither
  `Effects` (`crates/mcr-physics/src/types.rs:91`) nor `refresh_physics_inputs`
  (`crates/mcr-client/src/state.rs:686`) carries blindness.
- Impact: you can sprint while blind, and START_SPRINTING is sent.

#### P7 Dolphin's Grace has no effect (V)
- Vanilla: `LivingEntity#travelInWater` uses a slowdown of 0.96 with DOLPHINS_GRACE.
- mcr: `travel_in_water` (`crates/mcr-physics/src/tick.rs:506-535`) never applies it, and `Effects` has no
  field for it.

#### P8 Leather boots are never detected, so powder snow is never walkable (V)
- Vanilla: `PowderSnowBlock#canEntityWalkOnPowderSnow` checks for leather boots in the FEET slot. That gates
  the solid shape and the 0.2 climb in `LivingEntity#handleRelativeFrictionAndCalculateMovement`.
  `InsideBlockEffectType.FREEZE` sets `isInPowderSnow` for any touched cell.
- mcr: `PlayerPhysics.leather_boots` (`crates/mcr-physics/src/player.rs:39`) is read at `player.rs:203` and
  `tick.rs:466` but never set. `in_powder_snow` is set only for the feet block
  (`crates/mcr-physics/src/effects.rs:118-122`).
- Impact: with leather boots you sink while the server has you standing, so you rubber-band. You cannot
  climb powder snow.

#### P9 Scale attribute ignored for the player's box and eye height (V)
- Vanilla: `LivingEntity#getDimensions` scales the pose dimensions by `getScale()`.
- mcr: `bounding_box_for` and `eye_height` (`crates/mcr-physics/src/player.rs:172-181`) use fixed sizes.
  `PhysicsIds` (`crates/mcr-client/src/state.rs:353-364`) has no scale.
- Impact: collision, step-up, pose fitting and the camera are wrong on servers that change scale.

#### P10 Gliding through vines cancels the elytra (V)
- Vanilla: `LivingEntity#onClimbable` returns false while fall flying in a `CAN_GLIDE_THROUGH` block.
- mcr: `on_climbable` (`crates/mcr-physics/src/tick.rs:363-377`) has no exception, so `travel_fall_flying`
  (`crates/mcr-physics/src/fly.rs:57-61`) stops the glide.

#### P11 Ridden pigs and striders step only 0.6 blocks (V)
- Vanilla: `LivingEntity#maxUpStep` returns at least 1.0 when a player controls the mob.
- mcr: `make_sim` (`crates/mcr-client/src/riding.rs:216`) uses the synced step height as is.

#### P12 Ridden horses and camels sink in deep water (V)
- Vanilla: `LivingEntity#floatInLiquidWhileRidden` adds +0.04 Y for `CAN_FLOAT_WHILE_RIDDEN` mobs.
- mcr: the mount body goes through the player's `travel_in_water` (`crates/mcr-physics/src/tick.rs:506`)
  with no float term.

#### P13 Saddled nautilus cannot be steered (G)
- Vanilla: `AbstractNautilus` implements `PlayerRideableJumping`. Its `getControllingPassenger` returns the
  player when saddled, and the client simulates it (`tickRidden`, `travelInWater`, `getRiddenSpeed`).
- mcr: `VehicleKind` (`crates/mcr-client/src/riding.rs:106-132`) has no nautilus, so it becomes `Other`,
  then `Sim::Passive`, and no `MoveVehicle` is ever sent.
- Impact: the mount does not move.

#### P14 Turning a boat does not turn the camera (V)
- Vanilla: `AbstractBoat#positionRider` adds `deltaRotation` to the rider's yaw. `clampRotation` limits the
  view to ±105°.
- mcr: `Boat::delta_rotation` (`crates/mcr-physics/src/vehicle.rs:68`) is never called. The rider's yaw is
  the mouse yaw (`crates/mcr-client/src/riding.rs:405`).

#### P15 Ridden mob air steering speed (M)
- Vanilla: `LivingEntity#getFlyingSpeed` returns `getSpeed()*0.1` for a player-controlled mob.
- mcr: the mount uses `PlayerPhysics::flying_speed` (`crates/mcr-physics/src/player.rs:275`), a fixed 0.02.

#### P16 Cobweb ignores Weaving (M)
- Vanilla: `WebBlock#entityInside` uses (0.5, 0.25, 0.5) under Weaving.
- mcr: always (0.25, 0.05, 0.25) (`crates/mcr-physics/src/effects.rs:113-114`).

#### P17 Nether lava current too weak (M)
- Vanilla: the FAST_LAVA environment attribute gives a lava push of 0.007 (`Entity#updateFluidInteraction`).
- mcr: `fast_lava` (`crates/mcr-physics/src/player.rs:43`, `fluid.rs:224`) is never set.

#### P18 friction_modifier and air_drag_modifier not wired (M)
- Vanilla: both attributes feed `LivingEntity#travelInAir`.
- mcr: `refresh_physics_inputs` (`crates/mcr-client/src/state.rs:662-680`) leaves them at 1.0.

#### P19 Attribute values not sanitised (M, hostile input)
- Vanilla: `AttributeInstance#calculateValue` applies `RangedAttribute#sanitizeValue` (range clamp, NaN to
  minimum).
- mcr: `AttributeState::value` (`crates/mcr-client/src/entity.rs:125-146`) does neither. A huge
  `step_height` inflates the collision hull scan (`crates/mcr-physics/src/mover.rs:164-169`), and a NaN
  poisons the position.

#### P20 Block speed factor applies while gliding (M)
- Vanilla: `Player#getBlockSpeedFactor` returns 1 when flying or fall flying.
- mcr: `crates/mcr-physics/src/mover.rs:33-36` exempts creative flight only.

#### P21 Riptide does not bounce off entities (M)
- Vanilla: `LivingEntity#checkAutoSpinAttack` multiplies velocity by -0.2 on hitting an entity.
- mcr: `crates/mcr-physics/src/fly.rs:129-136` stops only on a horizontal collision.

#### P22 Floating bottom scaffolding cannot be stood on (M)
- Vanilla: `ScaffoldingBlock#getCollisionShape` returns `SHAPE_UNSTABLE_BOTTOM` for an unstable bottom
  piece.
- mcr: `crates/mcr-physics/src/collide.rs:63-66` has only the stable shape or nothing.

#### P23 Boats skip block effects (M)
- Vanilla: `AbstractBoat#tick` calls `applyEffectsFromBlocks`.
- mcr: `Boat::tick` (`crates/mcr-physics/src/vehicle.rs:303-313`) never does.

#### P24 Camel dash during cooldown and camel sprint (M)
- Vanilla: `LocalPlayer#aiStep` charges the jump bar only when `getJumpCooldown()==0`. Camel sprint is the
  normal sprint state.
- mcr: `crates/mcr-client/src/riding.rs:331-355` charges and sends START_RIDING_JUMP during the cooldown.
  `riding.rs:420-422` sprints only while the key is held.

#### P25 Vehicle fluid current and moves while the camera is detached (M)
- Vanilla: `EntityFluidInteraction.CurrentAccumulator#applyTo` normalises flow for non-players.
  `LocalPlayer#sendPosition` sends nothing when `!isControlledCamera()`.
- mcr: boats and mounts use the player-style average (`crates/mcr-physics/src/fluid.rs:234-257`). Move
  packets go out with a detached camera (`crates/mcr-client/src/client.rs:938-956`). Vehicle knockback is
  N8.

### I: interaction

#### I1 Spears (piercing and kinetic weapons) do not work (G)
- Vanilla: `Minecraft#startAttack` calls `MultiPlayerGameMode#piercingAttack` (PlayerAction STAB plus a STAB
  swing) for `PIERCING_WEAPON`. `continueAttack` never mines with one. A `KINETIC_WEAPON` starts using for
  72000 ticks.
- mcr: `PlayerActionKind::Stab` is never sent. Left-click attacks, mines or swings
  (`crates/mcr-app/src/play.rs:1448-1510`). `use_kind` (`crates/mcr-client/src/use_item.rs:570-597`) has no
  `kinetic_weapon` or `blocks_attacks` branch.
- Impact: spear jabs do nothing, spears mine blocks, and a charge never ends on release.

#### I2 Items used by the server but not tracked never send RELEASE_USE_ITEM (G)
- Vanilla: `Minecraft#handleKeybinds` calls `releaseUsingItem` whenever `isUsingItem` and the use key is up.
  `BundleItem#use` and `BrushItem#useOn` start using.
- mcr: release only follows a local `using_item` (`crates/mcr-app/src/play.rs:1518-1520,1550-1552`). The
  bundle is `Instant` (`crates/mcr-client/src/use_item.rs:155`, early return at `use_item.rs:409`). Brush and
  component-driven items get `None` from `use_kind`, and the server-flag sync (`use_item.rs:471-477`) goes
  through `use_kind` too. A brush also falls through to the off hand (`use_item.rs:202-213`).
- Impact: one click empties a bundle. A brush keeps brushing after release, and an off-hand block gets
  placed as well.

#### I3 Entity interactions send location (0,0,0) (G)
- Vanilla: `MultiPlayerGameMode#interact` sends the hit point relative to the entity. `ArmorStand#interact`
  uses its y to pick the slot.
- mcr: `crates/mcr-app/src/play.rs:1525-1528` sends `[0.0; 3]`. `Target::Entity`
  (`crates/mcr-app/src/target.rs:23`) has no hit point.
- Impact: you cannot take armour off an armour stand by hand, and servers that read the point get bad data.

#### I4 Adventure mode can never mine, even with can_break (G)
- Vanilla: `Player#blockActionRestricted` allows an adventure player whose held item's
  `canBreakBlockInAdventureMode` matches.
- mcr: `can_act_on_block` (`crates/mcr-client/src/actions.rs:308-318`) returns false for every adventure
  player.

#### I5 Entity reach is 4.5 in survival; range attributes ignored (V)
- Vanilla: `LocalPlayer#pick` uses `entityInteractionRange()` (default 3.0) and `blockInteractionRange()`
  (4.5), both server-synced attributes.
- mcr: `reach` (`crates/mcr-app/src/target.rs:27-29`) is hard-coded to 4.5 or 5.0 and used for blocks and
  entities (`target.rs:309`, `crates/mcr-app/src/play.rs:1627-1644`).
- Impact: survival players reach entities 1.5 blocks too far (an anticheat risk). Attribute changes have
  no effect.

#### I6 Entity pick boxes use the default type size (V)
- Vanilla: `LivingEntity#getDimensions` scales by age, the scale attribute and pose. Slimes scale by size.
  `Interaction#getDimensions` uses its synced width and height.
- mcr: `crates/mcr-app/src/target.rs:280-287` uses `entity_type_info` width and height.
- Impact: babies, small slimes, sneaking players and interaction entities are targeted through empty air
  or missed.

#### I7 Attacking and mining while using an item (V)
- Vanilla: while `isUsingItem`, `handleKeybinds` drops attack, use and pick clicks. `continueAttack`
  requires `!isUsingItem`.
- mcr: `gameplay` and `do_attack` (`crates/mcr-app/src/play.rs:1408-1446`) never check the use state.

#### I8 UseItem and UseItemOn repeat every 4 ticks while in use (V)
- Vanilla: the repeat needs `!isUsingItem` (`Minecraft#handleKeybinds`).
- mcr: `do_use` repeats whenever `use_delay == 0` (`crates/mcr-app/src/play.rs:1515`). `start_use`
  (`crates/mcr-client/src/use_item.rs:195-214`) has no use-state check.
- Impact: sweeping the crosshair over doors while drawing a bow toggles them.

#### I9 Survival mining timing fixed at dig start (V)
- Vanilla: `MultiPlayerGameMode#continueDestroyBlock` adds `getDestroyProgress` every tick, restarts on
  `sameDestroyTarget` (position and item), uses a 5-tick `destroyDelay` and swings every tick.
- mcr: `mine` (`crates/mcr-app/src/play.rs:1469-1510`) computes `estimate_dig_ticks` once and checks only the
  position. The vanilla-faithful `Client::continue_destroy` (`crates/mcr-client/src/actions.rs:227`) is
  never called by the app. Creative breaks every 5 ticks instead of 6 (`play.rs:1472-1475`), and nothing
  swings during the delay or on bedrock.
- Impact: changing ground, water, Haste or tool mid-dig keeps the old time. Finishing early causes a
  server revert flicker.

#### I10 Block placement prediction incomplete (V)
- Vanilla: the client runs `ItemStack#useOn`, including `mayBuild`/`can_place_on`, and `BlockItem#place`
  checks obstruction by any entity with the real placement state.
- mcr: `try_place` (`crates/mcr-client/src/use_item.rs:323-406`) has no adventure or `can_place_on` check,
  tests only the local player, and predicts only property-less blocks.
- Impact: wrong predictions flash the block and decrement the stack. The server never resends the slot,
  so the count stays one short.

#### I11 Use-without-item blocks missing from the consume table (V)
- Vanilla: `FlowerPotBlock`, `DragonEggBlock` and `RedstoneWireBlock` consume the click, so no UseItem
  follows.
- mcr: `block_consumes_use` (`crates/mcr-client/src/use_item.rs:85-147`) lacks `flower_pot`, `potted_*`,
  `dragon_egg` and `redstone_wire`.
- Impact: the server also uses the held item (throws, eats, places).

#### I12 Spectator interaction missing (V)
- Vanilla: left-click on an entity calls `MultiPlayerGameMode#spectate`, which sends
  `ServerboundSpectatorActionPacket`. Right-click still sends UseItemOn so spectators can look into
  containers. Scroll changes fly speed by ±0.005 in 0..0.2 (`MouseHandler#onScroll`).
- mcr: left-click sends `Attack` (`crates/mcr-app/src/play.rs:1452-1455`). `start_use` returns early for
  spectators (`crates/mcr-client/src/use_item.rs:196`). Scroll changes the hotbar slot
  (`play.rs:1694-1716`).
- Impact: spectators cannot take over a mob's view, open chests or change fly speed.

#### I13 Pick block covers blocks only, without Ctrl data (V)
- Vanilla: `Minecraft#pickBlockOrEntity` sends `PickItemFromEntity` for entities and sets `includeData` from
  `hasControlDown()` (either Ctrl).
- mcr: `crates/mcr-app/src/play.rs:2437-2441` handles blocks with `include_data = false`. `ctrl_held` tracks
  left Ctrl only (`play.rs:1733`), which also affects Ctrl+Q (`play.rs:2429`).

#### I14 Attack strength ticker not reset on item change or abort (V)
- Vanilla: `Player#tick` resets the ticker when the main-hand item changes. `stopDestroyBlock` resets it.
- mcr: `Hand::attack_ticker` (`crates/mcr-app/src/hand.rs:290-306`) resets only on attack or miss
  (`crates/mcr-app/src/play.rs:1401`).

#### I15 No miss penalty (V)
- Vanilla: a MISS in `Minecraft#startAttack` sets `missTime = 10` outside creative, which blocks attacks for
  10 ticks.
- mcr: `crates/mcr-app/src/play.rs:1459-1465` swings without a lockout.

#### I16 Own eating and drinking makes no particles or sounds (V)
- Vanilla: `ItemStack#onUseTick` calls `Consumable#emitParticlesAndSounds` every 4 ticks. Entity event 9
  (`Player#handleEntityEvent`, `completeUsingItem`) gives the final burst. The server's sound excludes the
  user.
- mcr: nothing emits these. `crates/mcr-app/src/entity_events.rs` has no event 9.

#### I17 Item sounds that only the user's client plays are missing (V)
- Vanilla: `BucketItem#use`, `FlintAndSteelItem#useOn` and `SpyglassItem#use` play through
  `level.playSound(player, …)`, which the server skips for that player.
- mcr: only placement sounds are predicted (`crates/mcr-app/src/play.rs:1554-1558`).

#### I18 Buckets and boats aimed at fluid (M)
- Vanilla: `BucketItem#use` and `BoatItem#use` raycast fluids and return SUCCESS or FAIL.
- mcr: `item_use_swings` (`crates/mcr-client/src/interaction.rs:136-138`) looks only at the block target,
  so it gives no swing on water and a false swing on stone with an empty bucket.

#### I19 Attacking and using while rowing (M)
- Vanilla: `LocalPlayer#rideTick` sets `handsBusy`, which blocks `startAttack` and `startUseItem`.
- mcr: the check is an empty block (`crates/mcr-client/src/use_item.rs:199-201`).

#### I20 Holding use on an entity does not repeat (M)
- Vanilla: `startUseItem` repeats the interaction every 4 ticks.
- mcr: entity targets act only on the click (`crates/mcr-app/src/play.rs:1529,1559`).

#### I21 No interaction-shape override in the raycast (M)
- Vanilla: `BlockGetter#clipWithInteractionOverride` uses `getInteractionShape` (cauldron, hopper,
  composter, scaffolding), so clicks into the bowl report face UP.
- mcr: outline shapes only (`crates/mcr-app/src/target.rs:81-89,126-202`).

#### I22 A reverted prediction does not snap the player back (M)
- Vanilla: `BlockStatePredictionHandler` stores the player position. `ClientLevel#syncBlockState` moves the
  player back if the restored block collides.
- mcr: `predict_block`/`finish_predictions` (`crates/mcr-client/src/digging.rs:233-248`) restore only the
  block.

#### I23 Hotbar scroll moves several slots and keeps the accumulator (M)
- Vanilla: `ScrollWheelHandler#getNextScrollWheelSelection` moves one slot per event and resets on a
  direction change.
- mcr: `crates/mcr-app/src/play.rs:1709-1715` moves up to 9 slots and never resets.

### C: containers, inventory and item tooltips

#### C1 Horse, donkey, llama and nautilus inventories never open (G)
- Vanilla: `ClientPacketListener#handleMountScreenOpen` builds `HorseInventoryMenu` or
  `NautilusInventoryMenu` and opens the matching screen.
- mcr: `crates/mcr-client/src/handler.rs:663-678` emits `ContainerOpened { menu_type: -1 }`.
  `crates/mcr-app/src/play.rs:1250-1262` gets `None` from `MenuKind::from_registry_id(-1)` and calls
  `Client::close_container`. `MenuKind::Mount` and `MenuKind::Nautilus` are never built.
- Impact: no saddle, armour or chest access on mounts. The server gets an immediate close.

#### C2 Inventory key while riding opens the player inventory (G)
- Vanilla: `Minecraft#handleKeybinds` checks `MultiPlayerGameMode#isServerControlledInventory` (riding a
  `HasCustomInventoryScreen`) and then sends the OPEN_INVENTORY player command (`LocalPlayer#sendOpenInventory`).
- mcr: `crates/mcr-app/src/play.rs:1997-2010` always pushes the local screen. `Client::open_inventory`
  (`crates/mcr-client/src/actions.rs:466`) is only called from `script.rs`.

#### C3 Villager trade list always empty (G)
- Vanilla: `MerchantScreen` lists the offers from `ClientboundMerchantOffersPacket` with level, XP bar and
  out-of-stock state.
- mcr: offers are stored (`crates/mcr-client/src/gamestate.rs:249-324`, `handler.rs:894`), but `StateView`
  (`crates/mcr-app/src/view.rs:30-153`) does not implement `GameView::merchant`, so the default `None`
  (`crates/mcr-ui/src/gui/env.rs:84`) applies.
- Impact: no trade buttons. Trading only works by guessing payment items.

#### C4 Stonecutter recipe list always empty (G)
- Vanilla: `StonecutterMenu`/`StonecutterScreen` list the synced stonecutter recipes for the input.
- mcr: `crates/mcr-client/src/registry.rs:169-174` keeps only the inputs and drops the result displays.
  `StateView` does not implement `stonecutter_options` (default `&[]`, `env.rs:88`), so
  `crates/mcr-ui/src/gui/container/menus.rs:268-292` makes no buttons.

#### C5 Loom shows no patterns (G)
- Vanilla: `LoomScreen` lists the selectable banner patterns (`no_item_required` tag or the pattern item's).
- mcr: `StateView` does not implement `loom_pattern_count` (default 0, `env.rs:92`), so
  `loom_patterns_shown` (`menus.rs:378`) is false. Only a pattern item with exactly one pattern works.

#### C6 Anvil rename never syncs on item placement (G)
- Vanilla: `AnvilScreen#slotChanged` sets the box to the hover name, and its responder sends
  `ServerboundRenameItemPacket`. `AnvilMenu#createResult` strips `CUSTOM_NAME` for a blank name.
- mcr: `crates/mcr-ui/src/gui/container/menus.rs:192-208` sets the value then `take_changed()` swallows the
  edit, so no rename packet goes out. The name comes from `item_tooltip().first_plain()`, the base name.
- Impact: repairing a renamed item strips its name and costs a level more. The previous typed name carries
  over to the next item.

#### C7 Crafter screen corrupts the client's hotbar and inventory (G)
- Vanilla: `CrafterMenu#addSlots` orders 9 crafter slots, 36 player slots, then the result slot (45).
- mcr: `mirror_player_slots` (`crates/mcr-client/src/inventory.rs:214-228`) assumes the player slots are the
  last 36, so for the crafter it shifts the inventory by one and copies the result into the hotbar. The
  number-key hotbar mapping in `predict_click` (`inventory.rs:284-291`) is off by one too.
- Impact: every crafter open corrupts the HUD hotbar, the held item and the inventory view until another
  full content arrives.

#### C8 Item tooltips show only the base item name (V, close to G)
- Vanilla: `ItemStack#getTooltipLines` gives the hover name (custom name italic, item_name, rarity colour),
  component lines (enchantments, stored enchantments, potion contents, trims, lore, dye, attribute
  modifiers, unbreakable, can_break/can_place_on), advanced lines with F3+H (durability, id, component
  count), all filtered by `tooltip_display`, and `tooltip_style` frames.
- mcr: `crates/mcr-ui/src/gui/env.rs:361-379` looks up the `item.minecraft.<id>`/`block.minecraft.<id>` key
  only. `StateView` does not override `item_tooltip`. `advanced_item_tooltips`
  (`crates/mcr-settings/src/options.rs:150`) is never read. `crates/mcr-ui/src/gui/tooltip.rs:108-109`
  ignores `tooltip_style`. Creative search (`crates/mcr-ui/src/gui/creative/screen.rs:341-353`) matches only
  that line, while vanilla searches all tooltip lines.
- Impact: enchanted books, potions and tipped arrows look the same. Renames, enchantments, lore and
  durability are invisible.

#### C9 Bundle UI missing (V)
- Vanilla: `BundleMouseActions` (scroll selects an item and sends `ServerboundSelectBundleItemPacket`),
  `ClientBundleTooltip` (contents grid, fullness bar, shown with an item in hand),
  `BundleItem#isBarVisible`/`getBarWidth`/`getBarColor`, and `BundleItem#overrideStackedOnOther` for left
  clicks.
- mcr: no bundle code in mcr-ui. `durability_bar` (`crates/mcr-ui/src/items/icons.rs:333`) covers damage
  only. Tooltips are hidden with a carried item (`crates/mcr-ui/src/gui/container/screen.rs:319`).
  `crates/mcr-client/src/menu.rs:250` predicts a left click with a bundle as a swap.

#### C10 Enchanting table clue tooltip missing (V)
- Vanilla: `EnchantmentScreen#extractRenderState` shows the clue and the lapis and level requirement lines
  from data 4-9.
- mcr: `StateView` does not implement `enchant_clue` (default `None`, `env.rs:105`), and `menus.rs:741-746`
  returns before the requirement lines.

#### C11 Ghost item stays on the cursor after closing a container (V)
- Vanilla: `Player#closeContainer` switches to `inventoryMenu`, whose carried stack is separate. The server
  returns the item through `set_player_inventory`.
- mcr: one shared `Inventory::cursor`, not cleared by `Client::close_container`
  (`crates/mcr-client/src/actions.rs:498-506`) or `Inventory::close` (`inventory.rs:123-131`).

#### C12 Status effects list missing from inventory screens (V)
- Vanilla: `InventoryScreen` and `CreativeModeInventoryScreen` draw `EffectsInInventory`.
- mcr: the `container/inventory/effect_background*` sprites are referenced nowhere.

#### C13 Screen previews missing: smithing armour stand, enchanting book, cartography map (V)
- Vanilla: `SmithingScreen#updateArmorStandPreview`, `EnchantmentScreen#extractBook`,
  `CartographyTableScreen#extractMap` (map content and the locked/max-scale error sprite).
- mcr: the entity preview runs only for the player inventory (`crates/mcr-app/src/play.rs:2872-2885`).
  `draw_cartography` (`menus.rs:839-889`) draws only frame sprites and the scaled frame for a locked map.

#### C14 Anvil cost not red when unaffordable (V)
- Vanilla: `AnvilScreen#extractLabels` uses red when the result slot's `mayPickup` is false.
- mcr: `menus.rs:783-786` is green unless the cost is 40 or more.

#### C15 Smithing cycling slot icons and onboarding tooltips missing (V)
- Vanilla: `SmithingScreen` uses `CyclingSlotBackground` per template and the missing-template, slot
  description and error tooltips.
- mcr: static icons (`crates/mcr-ui/src/gui/container/kind.rs`, Smithing) and no tooltips.

#### C16 Quick-craft preview details (M)
- Vanilla: `AbstractContainerScreen#extractSlot` caps at `min(item max, slot max)` with a yellow count, and
  draws no carried item when the remainder is 0.
- mcr: `crates/mcr-ui/src/gui/container/screen.rs:172-183` caps at the item max with a white count.
  `screen.rs:219-228` draws the item with a yellow "0".

#### C17 Merchant double-click collects (M)
- Vanilla: `MerchantMenu#canTakeItemForPickAll` is false, so no PICKUP_ALL is sent.
- mcr: `takes_pick_all` (`kind.rs`) allows it and `layouts.rs:391-402` predicts a collect.

#### C18 Lectern buttons (M)
- Vanilla: `LecternScreen#createMenuControls` shows Take Book only when `mayBuild()`, with translated labels.
- mcr: `crates/mcr-ui/src/gui/container/menus.rs:83-86` always shows hard-coded English "Done"/"Take Book".

#### C19 Screen keys hard-coded (M)
- Vanilla: container screens use `keyHotbarSlots`, `keySwapOffhand`, `keyDrop` and `keyInventory`, including
  mouse-bound hotbar keys. The creative saved-hotbar placeholder names the bound key
  (`CreativeModeInventoryScreen#selectTab`).
- mcr: literal 1-9, F, Q and E (`crates/mcr-ui/src/gui/container/click.rs:331-345`, `screen.rs:389`). The
  placeholder always says "C"/"1" (`crates/mcr-ui/src/gui/creative/screen.rs:972`).

#### C20 StateView leaves spectator, trims and score at defaults (M)
- Vanilla: `CrafterScreen` excludes spectators from slot toggles. Trim materials resolve from the registry.
  `DeathScreen#init` shows `Player#getScore` (synced `DATA_SCORE_ID`).
- mcr: `StateView` (`crates/mcr-app/src/view.rs:30`) does not implement `spectator`, `trim_materials` or
  `score` (`crates/mcr-ui/src/gui/env.rs:76,113,117`). Spectators get crafter toggles
  (`screen.rs:445`, `menus.rs:482-489`). Registry-id trims draw nothing in GUI icons
  (`crates/mcr-ui/src/items/stack.rs:176-188`). The death screen score (`crates/mcr-ui/src/gui/menus.rs:563`)
  is always 0.

#### C21 Crafter F-swap toggle (M)
- Vanilla: `CrafterScreen#slotClicked` reads inventory slot 40 for the off hand.
- mcr: `crates/mcr-ui/src/gui/container/screen.rs:458-462` maps button 40 past the end.

#### C22 Hotbar pop animation (M)
- Vanilla: `ClientPacketListener#handleContainerSetSlot` sets `popTime = 5` when a hotbar stack grows, and
  `Hud#extractSlot` squeezes it.
- mcr: no pop time anywhere (`crates/mcr-ui/src/screens/hud.rs:138-150`).

#### C23 Map decorations: wrong sprites, no names, no frame filter (V)
- Vanilla: `MapDecorationTypes` maps `banner_<colour>` to the `<colour>_banner` asset, `desert_pyramid` to
  its own asset and `ocean_ruin_warm` to `warm_ocean_ruins`. `MapRenderer#render` draws the decoration name
  under the icon and skips decorations without `renderOnFrame` in item frames.
- mcr: `sprite` (`crates/mcr-app/src/map.rs:45-57`) leaves `banner_*` and `ocean_ruin_warm` unmapped and maps
  `desert_pyramid` to `jungle_temple`. `decorations` (`map.rs:60-74`) drops the name. `emit_map`
  (`crates/mcr-entity-model/src/maps.rs:80-121`) has no name or frame filter.
- Impact: banner markers and warm ruins are missing textures, desert pyramids show a jungle temple, and
  named markers have no label.

### H: HUD

#### H1 Status effect icons missing (V)
- Vanilla: `Hud#extractEffects`: beneficial row at y=1, harmful at y=27, ambient background, blink in the
  last 10 s, `showIcon` respected.
- mcr: `draw_gameplay` (`crates/mcr-app/src/hud.rs:344-366`) has no effect pass.

#### H2 Attack indicator missing (V)
- Vanilla: `Hud#extractCrosshair` (crosshair mode) and `Hud#extractItemHotbar` (hotbar mode) draw
  `getAttackStrengthScale`.
- mcr: `attack_indicator` (`crates/mcr-settings/src/options.rs:162`) is never read. `hud.rs:352-354` draws
  the crosshair alone.

#### H3 Crosshair not inverted, drawn in third person and spectator (V)
- Vanilla: `Hud#extractCrosshair` draws only in first person, in spectator only over a `MenuProvider` or
  interactable block (`canRenderCrosshairForSpectator`), with the `CROSSHAIR` pipeline's invert blend.
- mcr: `crates/mcr-app/src/hud.rs:352-354` always draws it. mcr-ui has one premultiplied pipeline
  (`crates/mcr-ui/src/renderer.rs:111-142`).
- Impact: the crosshair vanishes on snow and sky, and shows in F5 views.

#### H4 Camera overlays missing (pumpkin, powder snow, portal, nausea, fire, in-block, underwater) (V)
- Vanilla: `Hud#extractCameraOverlays` draws `Equippable.cameraOverlay`, `powder_snow_outline` at
  `getPercentFrozen`, `extractPortalOverlay` and `extractConfusionOverlay`. `ScreenEffectRenderer#submit`
  draws the in-block sprite, `misc/underwater.png` and the fire overlay.
- mcr: `crates/mcr-app/src/overlays.rs:132-148` draws only the vignette, spyglass and totem.
- Impact: wearing a pumpkin, freezing, standing in a portal, burning, being in a block or underwater give
  no screen feedback.

#### H5 Overlays drawn above the HUD (V)
- Vanilla: `Hud#extractRenderState` draws camera overlays first, then the HUD.
- mcr: `crates/mcr-app/src/play.rs:2696` draws the HUD, then `play.rs:2706` draws overlays, including the
  opaque spyglass mask (`crates/mcr-app/src/overlays.rs:185-189`) and the vignette.
- Impact: scoping covers the hotbar, chat, titles and sidebar in black.

#### H6 Selected item name popup missing (V)
- Vanilla: `Hud#extractSelectedItemName` with `toolHighlightTimer` (40 × notification time), rarity colour,
  fade over the last 10 ticks.
- mcr: no pass in `draw_gameplay` (`hud.rs:344-366`).

#### H7 Mount health and jump bar missing (V)
- Vanilla: `Hud#extractVehicleHealth` draws vehicle hearts and skips the food row. `JumpableVehicleBar`
  replaces the XP bar while riding a jumpable mount.
- mcr: no vehicle hearts. Food is always drawn (`crates/mcr-ui/src/screens/hud.rs:286-300`), and the air row
  is fixed (`crates/mcr-app/src/hud.rs:494`). `jump_scale` (`crates/mcr-client/src/riding.rs:331-342`) is
  never shown (`hud.rs:391-392,457`).

#### H8 Hunger icons never show and food does not jitter at zero saturation (V)
- Vanilla: `Hud#extractFood` uses `food_*_hunger` sprites with HUNGER and jitters icons when saturation is
  0 and `tick % (food*3+1) == 0`.
- mcr: `HudState.hunger_effect` is honoured (`crates/mcr-ui/src/screens/hud.rs:286`) but never set
  (`crates/mcr-app/src/hud.rs:439-459`). `HudState` has no saturation (`player.saturation` exists at
  `crates/mcr-client/src/player.rs:21`).

#### H9 Air bubbles wrong (V)
- Vanilla: `Hud#extractAirBubbles`: shown when the eye is in water or air is below max, always 10 slots,
  a bursting bubble only underwater, `air_empty` slots, and `playAirBubblePoppedSound`.
- mcr: `crates/mcr-app/src/hud.rs:485-508` returns at full air, draws no empty bubbles, bursts out of water,
  and plays no pop sound.

#### H10 Off-hand slot ignores the main-hand option (V)
- Vanilla: `Hud#extractItemHotbar` puts the off-hand slot on the right for a left main arm.
- mcr: always on the left (`crates/mcr-ui/src/screens/hud.rs:147-149,175-176`, `crates/mcr-app/src/hud.rs:480-482`).

#### H11 Tab list hearts objectives render nothing (V)
- Vanilla: `PlayerTabOverlay#extractTablistHearts`.
- mcr: `crates/mcr-app/src/hud.rs:724,818` skips the score when `hearts` is set.

#### H12 Name tags (V)
- Vanilla: `LivingEntityRenderer#shouldShowName` hides non-team tags with F1. The anchor is the
  pose-dependent `NAME_TAG` attachment, range is the `NAME_TAG_DISTANCE` attribute.
  `SubmitNodeCollection#submitNameTag` uses background `getBackgroundOpacity(0.25)` and about 0x7F text
  alpha for sneaking and see-through passes.
- mcr: tags draw regardless of `hud_hidden` (`crates/mcr-app/src/play.rs:2678-2695`). The anchor uses the
  standing height (`crates/mcr-app/src/entities.rs:684-690`, `nametags.rs:125`), the range is 64. Dim tags
  use `0x20FF_FFFF` (`crates/mcr-app/src/nametags.rs:18,239-243`), and the background uses the raw
  `text_background_opacity` (`play.rs:2691`, `nametags.rs:203`).

#### H13 Locator bar background covers the XP level (V)
- Vanilla: `Hud#extractHotbarAndDecorations` draws the bar background, then the level, then the contents.
- mcr: the level is drawn in `HudSprites::draw` (`crates/mcr-ui/src/screens/hud.rs:192-201`) before
  `draw_locator` (`crates/mcr-app/src/hud.rs:466-468`, `hud.rs:203-205`).

#### H14 Jukebox "Now Playing" action bar missing (V)
- Vanilla: `LevelEventHandler#playJukeboxSong` calls `Hud#setNowPlaying` with animated colours.
- mcr: level event 1010 only plays the sound (`crates/mcr-app/src/play.rs:1350-1360`). `Titles::set_overlay`
  (`crates/mcr-client/src/social.rs:270-273`) has no animate flag.

#### H15 Subtitles missing (V)
- Vanilla: `SubtitleOverlay` with `showSubtitles`.
- mcr: `show_subtitles` (`crates/mcr-settings/src/options.rs:124`) is never read and no overlay exists.

#### H16 Chat scale, spacing, delay and focused height ignored (M)
- Vanilla: `ChatComponent#extractRenderState` uses `chatScale` and `9*(spacing+1)`, `ChatListener` delays
  with `chatDelay`, lines per page from `chatHeightFocused`.
- mcr: fixed 9 px lines and scale 1 (`crates/mcr-app/src/hud.rs:141-155`,
  `crates/mcr-ui/src/screens/chat.rs:345-385`). `ChatScreen::new` forces 20 lines
  (`crates/mcr-ui/src/gui/chat_screen.rs:127-131`).

#### H17 Sidebar and title backdrops (M)
- Vanilla: `Hud#displayScoreboardSidebar` uses `getBackgroundColor(0.3)`/0.4, following
  `textBackgroundOpacity` when background-for-chat-only is off. Titles get a backdrop in that mode.
- mcr: hard-coded `0x6600_0000`/`0x5000_0000` (`crates/mcr-app/src/hud.rs:619,621`), no backdrop
  (`hud.rs:644-671`).

#### H18 System message tag key (M)
- Vanilla: `ChatListener#handleSystemMessage` tags with `GuiMessageTag.systemSinglePlayer()`.
- mcr: `chat.tag.system` for all (`crates/mcr-ui/src/screens/chat.rs:67`, `crates/mcr-client/src/handler.rs:1575`).

#### H19 Tab heads follow the account, not the server's online mode (M)
- Vanilla: `PlayerTabOverlay` uses `ClientPacketListener#onlineMode` from the login packet.
- mcr: `show_heads: config.online.is_some()` (`crates/mcr-app/src/play.rs:2663`) although
  `server.online_mode` is stored (`crates/mcr-client/src/handler.rs:970`).

#### H20 Title and action bar cut-off (M)
- Vanilla: drawn while alpha > 0.
- mcr: drawn while alpha > 8 (`crates/mcr-app/src/hud.rs:640,653`).

### E: entities, first-person hand, camera and particles

#### E1 Invisible entities are drawn with their full body (G)
- Vanilla: `LivingEntityRenderer#submit`/`isBodyVisible`/`getRenderType` hide an invisible body (outline
  only when glowing) and draw it translucent only when `isInvisibleToPlayer` is false (spectator, team
  `seeFriendlyInvisibles`). Armour and held items still render.
- mcr: `v.invisible` (`crates/mcr-app/src/entities.rs:1109`) only skips the shadow
  (`crates/mcr-entity-model/src/scene.rs:919`). `finish` (`scene.rs:468-500`) pushes every part.
- Impact: invisible players, mobs and armour stands are fully visible, which defeats invisibility in PvP and
  invisible-armour-stand builds.

#### E2 Off-hand use not animated, wrong hand animated, off-hand scoping (V)
- Vanilla: `FirstPersonHandsAndItemsRenderer#submitArmWithItem` animates the hand with `useItemHand == hand`.
  `AbstractClientPlayer#getFieldOfViewModifier` zooms for a scope in either hand. `attackHand` picks the
  swinging arm.
- mcr: the use kind comes from the main item only (`crates/mcr-app/src/hand.rs:352`), `using_here =
  is_main && …` (`hand.rs:497`), scoping is main-hand only (`hand.rs:405`, `crates/mcr-app/src/play.rs:1052`),
  and the off hand never swings (`hand.rs:444`).
- Impact: an off-hand shield, food, bow or spyglass does not animate, and a main-hand bow or food plays the
  off hand's animation.

#### E3 Off-hand charged crossbow hides the main hand (V)
- Vanilla: `FirstPersonHandsAndItems#evaluateWhichHandsToRender` hides the off hand only for a charged
  main-hand crossbow.
- mcr: `crates/mcr-app/src/hand.rs:457-458` returns `(false, true)` for an off-hand charged crossbow.

#### E4 View bobbing and hurt tilt apply only to the hand or only to the world (V)
- Vanilla: `GameRenderer#renderLevel` applies `bobHurt` and `bobView` to the world projection, and
  `renderItemInHand` applies both to the hand.
- mcr: `view_roll` (`crates/mcr-app/src/play.rs:2467`) covers death roll and hurt tilt, `player_camera`
  (`play.rs:2512`) has no walk bob, and bobbing exists only in `apply_bob` (`crates/mcr-app/src/hand.rs:463`)
  with no hurt tilt on the hand.

#### E5 FOV: bow zoom, clamp, death and fluid FOV missing (V)
- Vanilla: `AbstractClientPlayer#getFieldOfViewModifier` (bow draw up to -15%), `Camera#tickFov` clamp
  0.1..1.5, `Camera#modifyFovBasedOnDeathOrFluid` (death, ×6/7 in water or lava, also for the hand).
- mcr: `update_fov` (`crates/mcr-app/src/play.rs:1043-1061`) has no bow term and no clamp. No death or fluid
  term outside mod mode (`crates/mcr-app/src/shell.rs:1670`). Hand FOV fixed at 70
  (`crates/mcr-app/src/entities.rs:1123`).

#### E6 Nausea and portal screen warp missing (V)
- Vanilla: `GameRenderer#renderLevel` applies the spinning skew scaled by `screenEffectScale²`.
- mcr: none. `screen_effect_scale` (`crates/mcr-settings/src/options.rs:105`) is never read.

#### E7 Burning entities show no fire (V)
- Vanilla: `EntityRenderer#extractRenderState` sets `displayFireAnimation` and the fire sprites are drawn
  around the entity.
- mcr: on-fire only forces block light 15 (`crates/mcr-app/src/entities.rs:633,700`).

#### E8 Third-person held items never show use state; crossbow pose frozen (V)
- Vanilla: item models resolve with their holder (bow pull, shield blocking, crossbow pull, trident throw).
  `HumanoidMobRenderer`/`AvatarRenderer` feed `ticksUsingItem` into the arm pose.
- mcr: `crates/mcr-entity-model/src/scene.rs:549` calls `items.item(stack, ctx)` with `using = None`
  (`items.rs:333`). `EntityView::use_ticks` (`view.rs:161`) is never set in mcr-app, so `hands.rs:181` and
  `rig_humanoid.rs:274` always see 0.

#### E9 Missing player layers: cape, shoulder parrots, stuck arrows and stingers, riptide swirl (V)
- Vanilla: `AvatarRenderer` adds `CapeLayer` (cloak physics), `ParrotOnShoulderLayer`, `ArrowLayer`,
  `BeeStingerLayer`, `SpinAttackEffectLayer` and `Deadmau5EarsLayer`.
- mcr: `crates/mcr-entity-model/src/humanoids.rs:663-733` builds skin, armour, wings, head item and held
  items only. `crates/mcr-app/src/skins.rs` fetches skins only, no capes.

#### E10 Local player does not sit when riding in third person (V)
- Vanilla: `HumanoidModel` applies the riding pose from `isPassenger`.
- mcr: the local view (`crates/mcr-app/src/entities.rs:612-638`) never sets `passenger`; remote entities do
  (`entities.rs:681`).

#### E11 Minecart pitch, boat paddles and hit wobble missing (V)
- Vanilla: `AbstractMinecartRenderer` tilts by `xRot` and wobbles on hit. `AbstractBoatRenderer#submit`
  wobbles and tilts in bubble columns, and `BoatModel` animates the paddles.
- mcr: static rigs with yaw only (`crates/mcr-entity-model/src/misc.rs:659-707`).

#### E12 Many particle types use a generic fallback or are skipped (V)
- Vanilla: `ParticleResources` registers a provider for every type (falling leaves live 300 ticks with
  gravity, `falling_dust` is tinted, `dust_pillar` uses the block sprite).
- mcr: `kind_of` (`crates/mcr-particles/src/sim.rs:138-191`) sends `vibration`, `trail` and `shriek` to
  `Skip`, and everything unlisted to `Generic` (`sim.rs:708-712`): cherry/pale oak/tinted leaves,
  snowflake, ash, white ash, spores, spore blossom, sonic boom, gust, dragon breath, sculk charge, soul,
  glow, squid ink, firefly, electric spark, wax, dust plume, trial spawner detection, ominous spawning.
  `dust_pillar` gets `Data::None` (`crates/mcr-particles/src/lib.rs:160-178`) and is dropped
  (`sim.rs:276`).

#### E13 First-person re-equip and lowering behaviours missing (V)
- Vanilla: `FirstPersonHandsAndItems#itemUsed` dips the hand after a placement or use, both hands lower
  while paddling (`isHandsBusy`), and an item change resets the swap ticker.
- mcr: `crates/mcr-app/src/hand.rs:308-338` has none of these. See also I14.

#### E14 Swing duration and animation type fixed (M)
- Vanilla: `LivingEntity#getModifiedSwingDuration` (Haste, Mining Fatigue) and the item's swing animation
  (NONE, WHACK, STAB). `CrossbowItem#getChargeDuration` (Quick Charge), `getUseDuration` from the consumable
  component.
- mcr: `DEFAULT_SWING_TICKS = 6` (`crates/mcr-app/src/hand.rs:20,282`), crossbow charge 25
  (`hand.rs:525`), use kinds by item name (`hand.rs:38-64,539-576`). No riptide or spear first-person pose.

#### E15 Upside-down names and shaking (M)
- Vanilla: `LivingEntityRenderer#setupRotations` flips Dinnerbone/Grumm. `isShaking` jitters frozen and
  converting mobs.
- mcr: neither in `entity_transform` (`crates/mcr-entity-model/src/scene.rs:1069-1097`).

#### E16 Walk animation for passengers and babies (M)
- Vanilla: `LivingEntity#updateWalkAnimation` uses ×3 for babies, and `LivingEntityRenderer` zeroes walk
  for passengers and the dead.
- mcr: `crates/mcr-app/src/entities.rs:1095-1096` always passes limb swing.

#### E17 Armour flashes red when hurt (M)
- Vanilla: `EquipmentLayerRenderer#renderLayers` uses `NO_OVERLAY`.
- mcr: `crates/mcr-entity-model/src/equipment.rs:118` sets `part.hurt = true`.

#### E18 Shadows: no distance fade or light term; option ignored (M)
- Vanilla: `EntityRenderer#extractShadow` fades to 0 at 16 blocks, draws per block on full-collision blocks
  with light above 3, keeps shadows while dying, and respects `entityShadows`.
- mcr: one quad (`crates/mcr-entity-model/src/scene.rs:913-966`), skipped while dying, and
  `set_shadows(true)` hard-coded (`crates/mcr-app/src/entities.rs:412`).

#### E19 Glint speed and strength options ignored (M)
- Vanilla: `GameRenderer` passes `glintSpeed` and `glintStrength`.
- mcr: fixed values (`crates/mcr-entity-model/src/equipment.rs:129-132`, `scene.rs:1106-1108`,
  `crates/mcr-ui/src/items/icons.rs:18`, `raster.rs:107`).

#### E20 Third-person distance and eye height ignore attributes and scale (M)
- Vanilla: `Camera#alignWithEntity` uses `CAMERA_DISTANCE × getScale()` (the mount's if larger).
- mcr: `THIRD_PERSON_DISTANCE = 4.0` (`crates/mcr-app/src/play.rs:90`). See P9 for eye height.

#### E21 Flat dropped-item stacks scattered in 3D (M)
- Vanilla: `ItemEntityRenderer#submitMultipleFromCount` lays flat items along local Z.
- mcr: ±0.15 on all axes (`crates/mcr-entity-model/src/misc.rs:139-157`).

### R: world rendering

#### R1 Mob effects never change vision: night vision, conduit power, blindness, darkness (G)
- Vanilla: `LightmapRenderStateExtractor#extract` feeds night vision, the darkness pulse
  (`calculateDarknessScale`) and brightness minus darkness. Conduit power counts as water vision.
  `BlindnessFogEnvironment` and `DarknessFogEnvironment#setupFog` pull fog to 5 and 15 blocks, and
  `FogRenderer#computeFogColor` darkens or brightens the fog.
- mcr: `Environment.night_vision` and `darkness_scale` (`crates/mcr-render/src/environment.rs:27-29`) are
  never set. `effects::environment` (`crates/mcr-app/src/effects.rs:997-1024`) and
  `crates/mcr-app/src/play.rs:2591-2616` ignore `player.effects`. No blindness or darkness fog exists.
- Impact: night vision does nothing, and blindness and the Warden's darkness do not limit vision.

#### R2 No lava or powder snow fog (V)
- Vanilla: `LavaFogEnvironment` (0x991A00, 0.25-1 block, 0-5 with fire resistance) and
  `PowderedSnowFogEnvironment` (0-2 blocks).
- mcr: only water (`apply_underwater`, `crates/mcr-app/src/effects.rs:989`; `play.rs:2597`).

#### R3 Underwater fog colour, biome and ramp wrong (V)
- Vanilla: `WaterFogEnvironment` takes colour and distances from biome attributes, scales the end by
  `max(0.25, LocalPlayer#getWaterVision)` (piecewise, 0.6 at 100 ticks, 1.0 at 600, counting down by 10 a tick
  out of water), and `computeFogColor` brightens by water vision.
- mcr: fixed colour times a sky-light factor (`crates/mcr-app/src/effects.rs:989-995`), linear `ticks/600`
  ramp, no brightening, `sky_fog_end` unset, timer reset to 0 on leaving water (`play.rs:888`).
- Impact: same water in every biome, too foggy for too long, and black underwater caves.

#### R4 Biome sky and fog colours ignored (V)
- Vanilla: biomes set `visual/sky_color` (`OverworldBiomes#calculateSkyColor`) and fog colours; Nether
  biomes set `visual/fog_color` (`NetherBiomes`). Blended around the camera.
- mcr: `Environment::overworld` always uses plains colours; `overworld_with` is never called from the app.
  `Environment::nether` (`crates/mcr-render/src/environment.rs:418`) hard-codes 0x330808.

#### R5 Rain fog missing (V)
- Vanilla: `AtmosphericFogEnvironment#setupFog` pulls fog in by the smoothed `rainFogMultiplier`.
- mcr: overworld fog stays 0..1024 (`crates/mcr-render/src/environment.rs:82`).

#### R6 Superflat draws a black lower sky (V)
- Vanilla: `ClientLevel.ClientLevelData#getHorizonHeight` returns `minY` for flat worlds;
  `SkyRenderer#shouldRenderDarkDisc` uses it.
- mcr: `horizon_height` fixed at 63 (`crates/mcr-render/src/environment.rs:96`), used at
  `crates/mcr-render/src/renderer.rs:1949-1953`, never set from the login `is_flat`.

#### R7 End sky flashes missing (V)
- Vanilla: `EndFlashState` drives `SkyRenderer#renderEndFlash` and the lightmap sky factor.
- mcr: `Environment::end` is static (`crates/mcr-render/src/environment.rs:405`).

#### R8 AO ignores per-block shade brightness (V)
- Vanilla: `BlockModelLighter` uses `getShadeBrightness`: 0.2 for full collision, else 1.0, with
  `TransparentBlock`, barrier, light and structure void at 1.0 and soul sand, mud and 8-layer snow at 0.2.
- mcr: `shade5` (`crates/mcr-mesh/src/mesher.rs:161`) uses `ATTR_FULL_COLLISION` only.
- Impact: glass and barriers cast AO shadows; soul sand, mud and full snow cast none.

#### R9 Translucent quads not sorted within a section (V)
- Vanilla: `SectionRenderDispatcher.RenderSection#resortTransparency` sorts quads by camera distance.
- mcr: `build_translucent` (`crates/mcr-render/src/renderer.rs:1742-1783`) sorts sections only, with depth
  writes off (`renderer.rs:950`).

#### R10 Pane and bar faces between neighbours not culled (V)
- Vanilla: `IronBarsBlock#skipRendering` skips faces between connected panes and bars.
- mcr: `face_visible`/`skips_same` (`crates/mcr-mesh/src/mesher.rs:170-189,870`) handle names ending in
  "glass" and a few others.
- Impact: z-fighting strips at every joint of stacked panes and bars.

#### R11 Per-element light_emission dropped (V)
- Vanilla: `CuboidModelElement` reads `light_emission` and applies it per vertex.
- mcr: parsed (`crates/mcr-assets/src/bake.rs:69,249`), dropped by `compile_quad`
  (`crates/mcr-mesh/src/cache.rs:846-873`). Open eyeblossom and firefly bush glow layers go dark at night.

#### R12 Emissive block list wrong (V)
- Vanilla: `emissiveRendering` for magma always and for sculk sensor and calibrated sculk sensor only while
  ACTIVE.
- mcr: `crates/mcr-mesh/src/cache.rs:565` marks magma and every sculk sensor state, not calibrated ones.

#### R13 Beacon beam without a pyramid (V)
- Vanilla: `BeaconBlockEntity#getBeamSections` is empty while `levels == 0` (`updateBase` every 80 ticks).
- mcr: `beacon_data`/`beam_sections` (`crates/mcr-app/src/block_entities.rs:226-231`,
  `crates/mcr-block-entity/src/data.rs:546`) scan upward only.

#### R14 Fog colour details (M)
- Vanilla: `AtmosphericFogEnvironment#getBaseColor` and `FogRenderer#computeFogColor`: sunrise tint toward
  the sun, void darkening near minY, boss darkening, weather darkening, sky mix at the current render
  distance, Nether and End mixed toward black.
- mcr: none of these; the mix runs once in `overworld_with` (`crates/mcr-render/src/environment.rs:392`) and
  `set_render_distance_chunks` (`environment.rs:442`) does not redo it.

#### R15 Rendering options parsed but never applied (M)
- Vanilla wires `ao`, `biomeBlendRadius`, `cutoutLeaves`, `chunkSectionFadeInTime`, `hideLightningFlashes`,
  `darknessEffectScale`, `improvedTransparency`, `textureFiltering`, `maxAnisotropyBit` and `mipmapLevels`.
- mcr: parsed (`crates/mcr-settings/src/options.rs:73-90`) but unused; blend radius is always
  `DEFAULT_BLEND_RADIUS` (`crates/mcr-app/src/render_system.rs:124,159,287`).

#### R16 New chunks do not fade in (M)
- Vanilla: `terrain.vsh`/`terrain.fsh` with `ChunkVisibility` (0.75 s default).
- mcr: no fade (`crates/mcr-render/src/shaders/terrain.wgsl`).

#### R17 Leaf, grate, mangrove root and water-overlay culling sets (M)
- Vanilla: `LeavesBlock#skipRendering` culls leaf-to-leaf only with cutout leaves off.
  `TransparentBlock` includes copper grates. `MangroveRootsBlock#skipRendering` skips only on Y.
  `FluidRenderer#tesselate` uses the overlay for HalfTransparent blocks and leaves.
- mcr: enclosed leaves cull interior faces (`crates/mcr-mesh/src/mesher.rs:544-551`), and the
  `ATTR_HALF_TRANSPARENT` list (`crates/mcr-mesh/src/cache.rs:548-563`) misses copper grates, culls mangrove
  roots on every axis and gives powder snow and mangrove roots the water overlay.

#### R18 Block light does not flicker (M)
- Vanilla: `LightmapRenderStateExtractor#tick` random-walks `blockLightFlicker`.
- mcr: constant 1.4 (`crates/mcr-render/src/environment.rs:72`).

#### R19 Snow line and frozen-ocean rain simplified (M)
- Vanilla: `Biome#getHeightAdjustedTemperature` (noise, seaLevel+17), `TemperatureModifier.FROZEN`.
- mcr: flat line at y=80 (`crates/mcr-app/src/effects.rs:101,287-295`).

#### R20 Break cracks follow the outline box, not the model (M)
- Vanilla: the crumbling decal is applied to the block's model quads.
- mcr: outline boxes (`crates/mcr-app/src/effects.rs:434-459`, `crates/mcr-render/src/effects/destroy.rs`).

#### R21 Fluid side shading in the Nether (M)
- Vanilla: `FluidRenderer#tesselate` shades sides by `up × north`/`up × west`.
- mcr: plain face shade (`crates/mcr-mesh/src/fluid.rs`, side loop).

#### R22 Biome tint height per column (M)
- Vanilla: `ClientLevel#calculateBlockTint` uses each block's y.
- mcr: highest tinted block per column (`crates/mcr-mesh/src/mesher.rs:403-411`, `tint.rs:424`).

#### R23 Variant seeds for two-block blocks (M)
- Vanilla: `DoublePlantBlock`, `DoorBlock` and `AbstractBedBlock#getSeed` seed both halves from one position.
- mcr: each cell's own position (`crates/mcr-mesh/src/mesher.rs:481-487`).

#### R24 End portal draws all six faces (M)
- Vanilla: `TheEndPortalBlockEntity#shouldRenderFace` draws up and down only.
- mcr: `0x3F` (`crates/mcr-block-entity/src/scene.rs:609`).

#### R25 Environment chosen by level name (M)
- Vanilla: dimension type attributes and timelines (`DimensionTypes`).
- mcr: matches `state.dimension.name` (`crates/mcr-app/src/effects.rs:1003-1015`); datapack dimensions get a
  static noon.

### S: sleeping, sound, chat, commands, options and screens

#### S1 Sleeping is not modelled; a player in bed cannot leave it (G)
- Vanilla: `Gui#tick` opens `InBedChatScreen` while sleeping; its Leave Bed button or Esc sends the
  STOP_SLEEPING player command (`InBedChatScreen#sendWakeUp`). `Player#isImmobile` blocks input, the box is
  `SLEEPING_DIMENSIONS`, `Camera#alignWithEntity` lies along the bed, `GameRenderer#renderItemInHand` skips the
  hand, `LivingEntityRenderer#setupRotations` lays sleepers in the bed, and `Hud#extractSleepOverlay` fades
  the screen.
- mcr: nothing references sleeping in mcr-app, mcr-client or mcr-ui. `PlayerCommand::StopSleeping`
  (`crates/mcr-protocol/src/play/serverbound/interact.rs:189`) is never sent. `player_tick` zeroes keys only
  when dead or detached (`crates/mcr-client/src/client.rs:931-945`). No SLEEPING pose in
  `crates/mcr-entity-model/src/catalog.rs:100-125` or `scene.rs:1069-1097`.
- Impact: a player who lies down is stuck until morning or damage, can walk around in bed and is teleported
  back, and sleepers stand upright in beds.

#### S2 Lightning makes no sound (V)
- Vanilla: `LightningBolt#tick` plays `entity.lightning_bolt.thunder` and `entity.lightning_bolt.impact`
  client-side; the server does not send them.
- mcr: `crates/mcr-app/src/effects.rs:416-423` only flashes the sky.

#### S3 Stop-sound packet ignored (V)
- Vanilla: `ClientPacketListener#handleStopSoundEvent` stops by name and/or source.
- mcr: `crates/mcr-client/src/handler.rs:265` emits `ClientEvent::StopSound`, which `Play::on_event`
  (`crates/mcr-app/src/play.rs`, catch-all) drops.
- Impact: `/stopsound` and map music scripts do nothing.

#### S4 Portal trigger sound missing (V)
- Vanilla: `LocalPlayer#handlePortalTransitionEffect` plays `block.portal.trigger` when the portal effect
  starts. The same state drives H4's portal overlay and E6's warp.
- mcr: nothing tracks standing in a portal.

#### S5 Chat click events handled differently (V)
- Vanilla: `Screen#defaultHandleGameClickEvent` sends `run_command` through
  `ClientPacketListener#sendUnattendedCommand` (confirm only on parse errors or missing permission, signed
  commands go to the chat box), opens `show_dialog`, sends `custom` as `ServerboundCustomClickActionPacket`
  and copies silently.
- mcr: `crates/mcr-ui/src/gui/chat_screen.rs:314-329` and `crates/mcr-ui/src/gui/clicks.rs:8-21` drop
  `ShowDialog` and `Custom`, and confirm every `RunCommand` and `CopyToClipboard` under the link question
  (`crates/mcr-ui/src/gui/menus.rs:184`).
- Impact: server click menus always prompt with the wrong text, and dialog and custom links are dead.

#### S6 No Brigadier feedback in the command box (V)
- Vanilla: `CommandSuggestions#formatText` colours arguments, `updateUsageInfo` shows parse errors,
  `fillNodeUsage`/`getSmartUsage` shows the argument chain, and `EditBox#setSuggestion` shows a ghost suffix.
- mcr: word-based completion (`crates/mcr-client/src/commands.rs:103-221`), one sorted usage per line that
  disappears while the server is asked (`crates/mcr-app/src/view.rs:270-318`), drawn white at x=4
  (`crates/mcr-ui/src/gui/chat_screen.rs:514-522`), single-colour input
  (`crates/mcr-ui/src/gui/edit_box.rs:393-406`).

#### S7 Suggestion popup ignores autoSuggestions; no player names in chat (V)
- Vanilla: the popup auto-shows only for commands with `autoSuggestions`; plain chat completes on Tab from
  online names plus server completions (`ClientSuggestionProvider#getCustomTabSuggestions`).
- mcr: always auto-shows (`crates/mcr-ui/src/gui/chat_screen.rs:168-236`); plain chat uses only
  `chat_completions` (`crates/mcr-app/src/view.rs:270-285`).

#### S8 Chat colour and link options ignored (V)
- Vanilla: `chatColors` strips formatting, `chatLinks`/`chatLinksPrompt` gate URLs (`Screen#clickUrlAction`).
- mcr: `chat_colors`, `chat_links` and `chat_links_prompt` are never read, so the chat settings toggles
  (`crates/mcr-app/src/settings.rs:306-356`) are inert. Scale, spacing, delay and height are H16.

#### S9 Options screens mostly missing (V)
- Vanilla: `OptionsScreen` (FOV, Online, Skin Customization, Music & Sounds, Video, Controls, Language,
  Chat, Resource Packs, Accessibility, Credits), `VideoSettingsScreen` (~30 options), `SoundOptionsScreen`,
  `ControlsScreen`, `MouseSettingsScreen`, `AccessibilityOptionsScreen`, `SkinCustomizationScreen`.
- mcr: five buttons (`crates/mcr-app/src/settings.rs:87-114`), 8 video items (`settings.rs:229-284`), sound
  sliders only (`settings.rs:286-304`), no mouse, accessibility, skin, online or resource pack screen,
  hard-coded English labels, GUI scale fixed at 0-4.

#### S10 Key binds screen (V)
- Vanilla: `KeyBindsList` with category headers, per-key reset, Reset Keys, red conflicts.
- mcr: a flat list (`crates/mcr-app/src/settings.rs:486-604`).

#### S11 Client information ignores options (V)
- Vanilla: `Options#buildPlayerInformation` sends language, chat visibility, chat colours, model parts, main
  hand, server listing and particles; `broadcastOptions` resends on change.
- mcr: defaults are never overridden (`crates/mcr-app/src/shell.rs:986-1015`); `chat_colors: true` and
  `model_customisation: 0x7f` are hard-coded (`crates/mcr-client/src/client.rs:344-356`,
  `crates/mcr-client/src/actions.rs:588-600`); resent only on view distance changes.
- Impact: others always see you right-handed with every skin layer; the server ignores chat visibility.

#### S12 Options nothing reads (V)
- Vanilla applies them all.
- mcr never reads `hide_lightning_flashes`, `discrete_mouse_scroll`, `high_contrast`, `darkness_effect_scale`,
  `fullscreen`, `narrator`, `directional_audio` and `sound_device` (others are listed under their feature:
  P5, H2, H15, E6, E18, E19, R15).

#### S13 No F3 key combos; F3 toggles on press (V)
- Vanilla: `KeyboardHandler#handleDebugKeys` (F3+A/B/C/D/G/H/I/L/N/P/Q/T/1-4, F3+F4 game mode switcher,
  F3+Esc), overlay toggled on release only when no combo was used.
- mcr: `crates/mcr-app/src/play.rs:2443-2444` toggles on press; no `key.debug.*` handling and no game mode
  switcher.

#### S14 F2 screenshot and F11 fullscreen do nothing (V)
- Vanilla: `Minecraft#handleGlobalKeyPress`.
- mcr: screenshots only from the CLI (`crates/mcr-app/src/main.rs:748-752`); no fullscreen toggle.

#### S15 Social Interactions, Quick Actions and pause screen additions missing (V)
- Vanilla: P opens `SocialInteractionsScreen`, G opens the `quick_actions` dialog tag
  (`Minecraft#handleKeybinds`), and `PauseScreen#createPauseMenu` adds server links or
  `pause_screen_additions` dialogs and the icon row.
- mcr: neither key is handled (`crates/mcr-app/src/play.rs:150-180,2404-2465`); the pause menu
  (`crates/mcr-ui/src/gui/menus.rs:394-433`) has Return, Advancements, Stats, Options/LAN and Disconnect.

#### S16 Spectator hotbar menu missing (G for spectators)
- Vanilla: `SpectatorGui` with `keySpectatorHotbar` and the number keys teleports to players and teams
  (`ServerboundTeleportToEntityPacket`); `Hud#extractHotbarAndDecorations` hides the item hotbar in spectator.
- mcr: spectators get the item hotbar (`crates/mcr-app/src/hud.rs:462-465`); `TeleportToEntity`
  (`crates/mcr-protocol/src/play/serverbound/misc.rs:107`) is never sent.

#### S17 Entity-bound sounds do not follow the entity (M)
- Vanilla: `EntityBoundSoundInstance#tick` tracks the entity and stops when it is removed.
- mcr: the position is taken once (`crates/mcr-app/src/play.rs:1311-1326`).

#### S18 Bubble column inside sounds missing (M)
- Vanilla: `BubbleColumnAmbientSoundHandler`.
- mcr: absent.

#### S19 Sounds keep playing while a local world is paused (M)
- Vanilla: `Minecraft#runTick` calls `pauseAllExcept(MUSIC, UI)`.
- mcr: pause is used only by the mod bridge (`crates/mcr-modbridge/src/sound/voices.rs:170`).

#### S20 Sound packet seed ignored (M)
- Vanilla: `ClientLevel#playSeededSound` picks the variant from the seed.
- mcr: own LCG (`crates/mcr-app/src/audio.rs`); `ClientEvent::Sound` has no seed.

#### S21 Small chat differences (M)
- Vanilla: Shift-click insertion (`ChatScreen#handleComponentClicked`), Shift+scroll one line, persistent
  `command_history.txt` (`CommandHistory`).
- mcr: none; scroll always moves 7 lines (`crates/mcr-ui/src/gui/chat_screen.rs:493`).

#### S22 Credits controls (M)
- Vanilla: `WinScreen` speeds up while Space is held, Ctrl adds ×15, Up reverses while held.
- mcr: Space toggles, Ctrl does nothing, Up latches (`crates/mcr-ui/src/gui/credits.rs:274-285`).

#### S23 Smooth camera key (M)
- Vanilla: `key.smoothCamera` toggles cinematic camera smoothing.
- mcr: not handled.

### N: clientbound packet handling and entity state

The archived packet-audit table marks `level_event`, `stop_sound` and `sound_entity` complete. That holds
in mod mode, where Java plays them, but not in native mode. Effects (`update_mob_effect`) are R1 and E6,
`stop_sound` is S3, `sound_entity` is S17, swing types are E14 and the hotbar pop is C22.

#### N1 Almost every level event is dropped in native mode (V, high impact)
- Vanilla: `LevelEventHandler#levelEvent` covers about 90 ids (1000-1054, 1500-1505, 2000-2020, 3000-3021)
  and `globalLevelEvent` covers 1023, 1028 and 1038.
- mcr: `crates/mcr-client/src/handler.rs:230-236` forwards the event, and only
  `crates/mcr-app/src/play.rs:1338-1367` consumes it, for 2001, 1010 and 1011.
- Impact: no dispenser, door, anvil, brewing, composter, crafter, grindstone, portal travel, fire
  extinguish or zombie door sounds; no bone meal, splash and lingering potion, eye of ender, spawner, wax,
  scrape, sculk, trial spawner, vault, lightning rod or end gateway effects; no global wither, dragon death
  and end portal sounds.

#### N2 Pistons do not animate; pushed blocks vanish while moving (V)
- Vanilla: `PistonBaseBlock#triggerEvent` runs on the client and creates the `PistonMovingBlockEntity`
  itself; the server sends no data for it.
- mcr: block events reach the animator, which ignores pistons
  (`crates/mcr-block-entity/src/anim.rs:66-85`). A `moving_piston` gets empty data, so nothing is drawn
  (`crates/mcr-block-entity/src/data.rs:501-506`, `scene.rs:1886-1891`).
- Impact: pushed blocks disappear for 2 ticks and pop in; the head does not slide.

#### N3 Note blocks show no note particle (M)
- Vanilla: `NoteBlock#triggerEvent` spawns `NOTE` on the client.
- mcr: not handled in `crates/mcr-block-entity/src/anim.rs:66-85`.

#### N4 Compass and clock items frozen (V)
- Vanilla: the `CompassAngle` and `Time` item-model properties read `ClientLevel#getRespawnData`, the last
  death location, lodestone targets and the dimension clock.
- mcr: `ItemContext.compass_angle`, `day_fraction` and `dimension` are never written
  (`crates/mcr-assets/src/item_eval.rs:155-184`; `crates/mcr-entity-model/src/items.rs:388-394` uses
  `AssetCtx::default()`; `crates/mcr-ui/src/items/icons.rs:117`). The spawn stored at
  `crates/mcr-client/src/handler.rs:308-315` is never read and the death location is dropped.

#### N5 Immediate respawn breaks after the first respawn (V)
- Vanilla: `handleRespawn` copies `shouldShowDeathScreen` from the old player; game event 11 is not resent.
- mcr: `reset_for_spawn` (`crates/mcr-client/src/player.rs:109-122`) resets `show_death_screen` to true.
- Impact: with `immediate_respawn`, every death after the first shows the death screen.

#### N6 Respawn data-kept flags ignored (V)
- Vanilla: `handleRespawn` keeps attributes (flag 1) and entity data, velocity and rotation (flag 2).
- mcr: `data_to_keep` is never read (`crates/mcr-client/src/handler.rs:68`), and `reset_for_spawn` wipes
  attributes, data and effects.
- Impact: after a portal the player loses speed, max health, scale, reach and other attributes until a
  resync; absorption and skin layers reset.

#### N7 Own vehicle rubber-bands on position sync (V)
- Vanilla: `handleEntityPositionSync` only updates the base position for a locally controlled entity.
- mcr: `position_sync` (`crates/mcr-client/src/handler.rs:1423-1434`) has no controlled check, unlike
  `move_entity` (`handler.rs:1314`); the boat or mount snaps (`crates/mcr-client/src/riding.rs:303-321`).
- Impact: beaching a boat and about once a minute the vehicle jumps back by ping times speed.

#### N8 Controlled vehicle ignores server velocity and bubble-column events (M)
- Vanilla: `handleSetEntityMotion` → `lerpMotion` on the vehicle; `AbstractBoat#handleEntityEvent` 71/72.
- mcr: motion goes to `e.velocity` (`crates/mcr-client/src/handler.rs:446-455`), which the riding sim never
  reads; events 71/72 unhandled (`crates/mcr-client/src/entity_events.rs:20-36`).

#### N9 Many projectiles stutter between updates (V)
- Vanilla: entities without an interpolation handler snap to each packet and keep simulating
  (`AbstractArrow#tick`, `FireworkRocketEntity#tick`, hurting projectiles).
- mcr: only 12 types are simulated (`crates/mcr-client/src/entity.rs:44-57`); tridents, wind charges,
  fireballs, wither skulls, dragon fireballs, llama spit, firework rockets and eyes of ender interpolate and
  stop (`entity.rs:370-382,585-600`).

#### N10 Living entity interpolation differs (M)
- Vanilla: `SteppedInterpolationHandler` with the type's update interval (2 for players), queued steps and
  replayed per-tick positions.
- mcr: a fixed 3 steps to the final position (`crates/mcr-client/src/entity.rs:19,370-382,410-421`);
  intermediate steps folded (`entity.rs:497-513`).

#### N11 Particle flags dropped or misused (M)
- Vanilla: `handleParticleEvent` honours the randomization type, and `ClientLevel#doAddParticle` uses
  `overrideLimiter` to skip the 32-block limit; `alwaysShow` only affects the particle setting roll.
- mcr: `crates/mcr-client/src/handler.rs:237-244` drops both, `crates/mcr-app/src/effects.rs:486-501` forces
  defaults, and `crates/mcr-particles/src/lib.rs:315-321` uses `always_show` as the distance override.

#### N12 Entity velocity clamped to ±3.9 (M)
- Vanilla: 26.3 motion uses the LP vector codec without a clamp (`Entity#lerpMotion`).
- mcr: `clamp_motion` (`crates/mcr-client/src/handler.rs:1680-1683`), applied at 358, 452 and 1416.

#### N13 Bundles not applied atomically (M)
- Vanilla: `handleBundlePacket` applies the bundle together.
- mcr: the delimiter is counted as unhandled (`crates/mcr-client/src/handler.rs:937`) and frames apply as
  they arrive (`crates/mcr-client/src/client.rs:841-853`).

#### N14 player_look_at uses a fixed eye height (M)
- Vanilla: `ClientboundPlayerLookAtPacket#getPosition` uses the target's eye height and falls back to the
  packet position.
- mcr: 1.62 and nothing for an unknown entity (`crates/mcr-client/src/handler.rs:1487-1500`).

#### N15 Entity teleports ignore velocity-relative flags (M)
- Vanilla: `PositionMoveRotation#calculateAbsolute` applies DELTA and ROTATE_DELTA.
- mcr: `teleport_entity` (`crates/mcr-client/src/handler.rs:1414-1417`) sets velocity absolutely.

#### Known deferred items (not counted)
`projectile_power`, `move_minecart_along_track`, `debug_*`, `debug_sample`, `pong_response`, `tag_query`,
`game_test_highlight_pos`, the demo intro popup and game event 13 stay deferred as recorded in
`parity-followups-1`.

## Batch 21 notes: block rendering fidelity

- **Translucent sort (R9).** Quads are sorted farthest first by centroid distance, stable, like
  `VertexSorting.byDistance`. The mesher sorts on the worker against the camera position in the snapshot,
  so a fresh mesh arrives sorted. Later resorts follow vanilla's `TranslucencyPointOfView`: a section is
  re-sorted when the camera's section relative to it (clamped to -1..1 per axis) changes, and also on any
  camera block change while that point of view is axis-aligned or the section is within one section of
  the camera. The renderer keeps each section's translucent quads (40 B each) in an `Arc`, hands stale
  sections to one background sort thread, at most `max(n/8, 15)` per frame, and re-uploads the sorted
  quads into the same arena slots as one atomic batch. Sorting is a `u64` key sort, about 5 ns a quad.
  The render thread never sorts. Rejected: sorting every frame (cost grows with view distance) and an
  index buffer per section (a second arena and indirect index ranges for one gain).
- **Leaves (R17).** Vanilla keeps every leaf face while `cutoutLeaves` is on, which is the default, so
  the enclosed-leaf culling is gone. Forests mesh about 4% more quads.
  With `cutoutLeaves` off, leaves draw opaque and cull against each other, as in vanilla.
- **Tint per height (R22).** A section carries up to `MAX_TINT_SLOTS = 4` blended tint tables, one per
  distinct tinted y window, and a slot map in the top byte of table 0. Sections with more distinct
  windows reuse the slot of the tinted level below. The tables are computed only for y levels that hold
  tinted blocks.
- **Rendering options (R15).** `ao`, `biomeBlendRadius` and `cutoutLeaves` rebuild the mesh pool and
  remesh. `chunkSectionFadeInTime` drives the terrain fade. `textureFiltering`, `maxAnisotropyBit` and
  `mipmapLevels` rebuild the block sampler. `hideLightningFlashes` and `darknessEffectScale` were
  already applied. Filtering `NONE` keeps hardware nearest sampling instead of vanilla's linear
  sampler with the `sampleNearest` shader trick. The two match except at mip seams. `improvedTransparency`
  (vanilla's Fabulous layered translucency) is not implemented. It stays a stored option and is left for
  the owner to schedule.
- **Fade-in (R16).** Each section records the time it was first drawn. The renderer default is no fade.
  The play state sets it from the options.
- **Beacon (R13).** Levels are recomputed with the beam every 20 ticks, where vanilla does it every 80
  ticks. The beam is empty at level 0.
- **Break cracks (R20).** Cracks are drawn on the block model's quads, with offset and variant. Block
  entities with no baked model fall back to their outline boxes.
- **Hostile input.** Snow line, beacon base, resort and seed arithmetic use i64, checked or wrapping
  integers, so a hostile sea level, `min_y` or position cannot overflow.
- **End portal (R24).** Only the up and down planes are drawn, and only the one facing the camera, since
  vanilla's portal pipeline culls back faces and the flat pipeline does not.
- **Shader cost.** The filter mode is a pipeline override constant. Terrain pipelines are rebuilt when
  it changes, so the default nearest path carries no filtering code (a uniform branch cost 3% GPU). The
  per-height tint lookup runs only in sections flagged with more than one table. The fade visibility
  rides in the fog varying, and the fade and the fog blend fold into one `mix`.
- **Measured** (release, 1920x1080, render distance 16, 600 frames, median GPU time, interleaved runs):
  over a forest and beach, 0.158 ms before and 0.172 ms after. Leaf faces that vanilla keeps account for
  about 0.005 ms of that, and the fade-in for about 0.003 ms. The rest is spread over small per-fragment
  additions: the opaque-cutout flag, the tint kind mask and the fluid up shade. The mesher is 2.0% slower per section,
  with 4% more quads. The translucent sort takes 2.9 µs for 15 water sections and 145 µs for a
  12288-quad glass checkerboard, on the sort thread.

## Close-out notes

- **Leaves and occlusion.** Vanilla gives leaves `noOcclusion()` (`Blocks#leavesProperties`), so
  `isSolidRender` is false and `SectionCompiler` never marks them opaque in the `VisGraph`. mcr reads the
  same flag into `ATTR_SOLID_RENDER`, and batch 21 did not change it. The reported 402-against-154 draw
  spot did not reproduce. On the main server world (seed 12345, render distance 16, 1920x1080) 1296 views
  over a 400-block grid, plus forest and beach views in world and connected mode, never kept more draws
  with occlusion on than the build before batch 21. 98 views kept fewer, because the leaf faces vanilla
  keeps fill holes in the depth pyramid. With occlusion off the counts are identical. Median GPU time over
  600 frames, three interleaved runs:

  | View | Before | After |
  |---|---|---|
  | Over the dark forest toward the beach, culling on | 0.157 ms, 1102 draws | 0.152 ms, 933 draws |
  | Under the canopy looking up, culling on | 0.185 ms, 850 draws | 0.197 ms, 702 draws |
  | Over the forest, culling off | 0.161 ms | 0.175 ms |
  | Under the canopy, culling off | 0.182 ms | 0.210 ms |

  The remaining cost is fragment work on the leaf faces vanilla keeps. It cannot be culled without
  changing the picture. Solid and cutout quads share one discarding pipeline, and sections draw in cull
  order, so leaves behind terrain are shaded more often than in vanilla, which draws SOLID before CUTOUT.
  Splitting the passes is recorded in `vanilla-parity-5`.
- **Block entities at join.** `NetworkWorld` folded `ChunkLoaded` and `ChunkUnloaded` into the mesh world
  and never passed them on, so block entities that arrived in chunk data reached the scene only if the
  chunk was already loaded when the scene assets were built. End portals, gateways and chests in later
  chunks were invisible until a block update, and unloaded chunks never released their entries. Chunk
  events now reach the game too, and the scene indexes its entries by chunk, so a resent chunk replaces
  its block entities and an unloaded one drops them.
- **Re-audit.** Every batch was re-read against the decompiled client. The leftovers, with the notes from
  the batch reports, are collected in `vanilla-parity-5`.

## Severity totals

| Subsystem | G | V | M | Total |
|---|---|---|---|---|
| P physics | 2 | 12 | 11 | 25 |
| I interaction | 4 | 13 | 6 | 23 |
| C containers | 7 | 9 | 7 | 23 |
| H HUD | 0 | 15 | 5 | 20 |
| E entities, hand, camera | 1 | 12 | 8 | 21 |
| R world rendering | 1 | 12 | 12 | 25 |
| S sleep, sound, chat, options | 2 | 14 | 7 | 23 |
| N packets | 0 | 7 | 8 | 15 |
| **Total** | **17** | **94** | **64** | **175** |
