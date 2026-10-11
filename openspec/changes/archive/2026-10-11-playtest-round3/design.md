## Decisions

- **Dispatch along the vanilla class chain.** `entity_events::effects(type, event)` returns what the most-derived `handleEntityEvent` override does, falling back through `TamableAnimal`/`AbstractHorse` → `Animal` → `Mob` → `LivingEntity` → `Entity`, so an event id reused by several classes (3 is death for mobs and the break burst for snowballs; 4 is an attack for golems and the bite for fangs) only fires for the right types. The class table (`entity_class.rs`) was derived from the 26.3 class hierarchy.
- **Only local sounds.** On a client, `Entity.playSound` plays nothing for entities other than the local player, because the server sends those sounds itself. Effects therefore only reproduce `playLocalSound` calls (armor stand hit, armadillo peek, zombie villager cure, evoker fangs, item break, kinetic hit, fireworks, totem).
- **Clocks, not flags.** Each animation an event starts is a per-entity clock in `EntityEvents`, ticked like the entity's `aiStep` and read by the renderer through `EntityView.events` (`EventAnim`) plus the existing `attack_ticks`, `offer_ticks` and `shake`. Clocks are dropped with their entity and capped at 4096 entities.
- **State changes live in `mcr-client`.** Swapping hands, the ravager roar knockback and the fishing rod pull on the local player, reduced debug info and use-item completion change client state, so `ClientState::apply_entity_event` handles them before the event reaches the app.
- **Fireworks** follow `FireworkParticles.Starter`: one explosion every other tick, blast sound on the first tick (large when there are three or more explosions or a large ball, `_far` beyond 16 blocks), twinkle sound at the end. Sparks carry colour, fade colour, trail and twinkle in the particle data; trails spawn half-aged copies, twinkling sparks hide every other three-tick window after the first third of their life. Firework sparks now use vanilla's gravity of 0.1 and white colour for rocket trails.
- **Collars.** `SynchedEntityData` only sends non-default values when an entity is added, and a collar that is still red (the default) is never sent. The renderer read "absent" as "no collar". The synched data indices (tame flag at 18 bit 4, wolf collar at 21, cat collar at 23) were already right, because `AgeableMob` defines both `DATA_BABY_ID` (16) and `AGE_LOCKED` (17) in 26.3.

- **Live collar check.** A headless client on a vanilla 26.3 server saw a tamed wolf with `CollarColor:11b` send index 21 `Int(11)`, while a tamed wolf, a tamed cat and a tamed parrot without one sent only the tame flags (18 = `Byte(4)`) and owner (19): no collar or variant entry.
- **Glide pose.** `AvatarRenderer.setupRotations` runs after the living turn, so it is `Plan.post_rot`. Fall-fly ticks, the movement deltas for the roll and `speedValue`, and the `ElytraAnimationState` wing angles are kept per tracked entity in `mcr-app` (`GlideTrack`, ticked with the entity) and reach the model as `EntityView.glide`. The local player uses its physics velocity; remote players use their synced velocity, which the server sends every tick while they glide. The inventory preview keeps its standing pose but shows the wings.
- **Creative flight.** mcr-physics let a creative flyer start gliding because `can_glide` lacked vanilla's `!abilities.flying`, and flight toggles never reached the server. Both are fixed; the client now sends `ServerboundPlayerAbilities` whenever its flying state changes.
- **Thrown items.** The entity scene had a camera billboard, but nothing ever set it, so every billboard (thrown items, experience orbs, bobbers, rockets) used the identity rotation and looked like a fixed dropped item. The app now passes the camera each frame. The extra 0.125 lift was removed (vanilla has none), and scale, full bright and the two-tick near-camera hide follow `EntityRenderers` and `ThrowableProjectile`/`Fireball`/`EyeOfEnder`.
- **TNT.** The swell used the raw synched fuse without partial ticks, and the white overlay did not exist. The entity shader gains a white overlay flag (75% white, the overlay texture's alpha at u = 1). Minecarts never drew their display block; while adding it, minecarts, boats and arrows turned out to float 1.5 blocks up because the living model drop (`translate(0, -1.501, 0)`) was applied to every model. `Plan.model_y` now carries that drop and is 0 for them. Other non-living part models (end crystal, shulker bullet, llama spit, wind charge, leash knot) were not checked and remain a follow-up.
- **Kelp.** mcr built mips with a plain alpha average. At mip 3 that leaves no kelp texel above the 0.5 cutout (coverage 0.000 for `kelp_plant` and `kelp`), and at mip 2 the tip's coverage swings between 0.083 and 0.174 across animation frames, so far kelp vanished and reappeared as the mip level or frame changed. Vanilla rescales each cutout mip to the base coverage and adds `alpha_cutoff_bias` (0.1 for kelp): coverage 0.70/0.90 for `kelp_plant` and 0.33/0.62 for `kelp` at mips 2/3. A fixed camera showed no culling pops (frames with and without both culling passes differ only by the water and kelp animation phase).
- **Sea floor.** Measured at an ocean column of the owner's world: the saved sky light is 15 above the surface and drops by 1 per water block to 0 at y = 48 (water, kelp and seagrass all have light opacity 1 in the 26.3 data, and so does mcr's light table). The sections below carry no sky light data, on disk and on the wire alike. mcr read them as full daylight (15), so the floor 20 to 30 blocks down was lit like the surface. Vanilla reads such a section as the bottom layer of the next section above that has data, which is 0 there. Water itself matched: texture alpha 180/255 (0.706) in both, ocean tint `#3F76E4` from the biome in both, no extra fog above water in either.

## Entity events → effects

| Event | Class | Effect |
|---|---|---|
| 0 | Arrow | 20 `entity_effect` particles in the arrow's colour, unless the colour is -1 |
| 1 | Rabbit | Sprint particle of the block underfoot, hop keyframes for 15 ticks |
| 2 | LivingEntity | Kinetic weapon hit sound of the item in use, at most every 10 ticks |
| 3 | LivingEntity | Death pose; the local player's death sound (others' come from the server) |
| 3 | Snowball | 8 `item_snowball` (or item) particles |
| 3 | Egg | 8 item particles of the egg |
| 4 | IronGolem | Arm attack swing for 10 ticks (sound from the server) |
| 4 | Hoglin, Zoglin, Ravager | Head attack animation for 10 ticks |
| 4 | Warden | `WARDEN_ATTACK` keyframes |
| 4 | Creaking | `CREAKING_ATTACK` keyframes, 15 attack ticks |
| 4 | EvokerFangs | `evoker_fangs.attack` sound; fangs appear and bite over 20 ticks, 12 crit particles 8 ticks in |
| 6 / 7 | TamableAnimal, AbstractHorse | 7 `smoke` / 7 `heart` particles |
| 8 / 56 | Wolf | Shake-off-water body roll for 2 s with `splash` drips and a darker coat; cancel |
| 9 | Player (local) | Ends the local item use |
| 10 | Sheep | Grazing head drop for 40 ticks |
| 11 / 34 | IronGolem | Holds out a poppy for 400 ticks; stops |
| 12, 13, 14, 42 | Villager | 5 `heart`, `angry_villager`, `happy_villager`, `splash` particles above the head |
| 15 | Witch | 10–44 `witch` particles above the head |
| 16 | ZombieVillager | `zombie_villager.cure` sound at the eyes |
| 17 | FireworkRocket | Firework starter from the rocket's `fireworks` component; poofs without explosions |
| 18 | Animal | 7 `heart` particles |
| 18 | Allay | 3 `heart` particles |
| 19 | Squid, GlowSquid | Tentacle phase resync |
| 20 | Mob | 20 `poof` particles (spawner spawn) |
| 21 | Guardian | Beam attack sound (already handled) |
| 22 / 23 | Player (local) | Reduced / full debug info |
| 24–28 | Player (local) | Permission level (already handled) |
| 31 | FishingHook | Pulls the hooked local player toward the angler |
| 32 | ArmorStand | `armor_stand.hit` sound at 0.3, 5-tick wiggle |
| 35 | any | Totem (already handled) |
| 38 | Dolphin | 7 `happy_villager` particles |
| 39 | Ravager | Stunned head shake for 40 ticks with grey `entity_effect` sparks, then the roar mouth |
| 40 / 41 | Ocelot | 7 `smoke` / 7 `heart` particles |
| 45 | Fox | 8 item particles of the held item from the mouth |
| 46 | LivingEntity | 128 `portal` particles |
| 47–52, 65, 68 | LivingEntity | Break sound and 5 item particles for main hand, off hand, head, chest, legs, feet, body, saddle |
| 53 | Entity | 5 honey block particles |
| 54 | LivingEntity | 10 honey block particles |
| 55 | LivingEntity | Swaps the main and off hand equipment |
| 58 / 59 | Goat | Lowers the head for ramming / raises it |
| 60 | LivingEntity | 20 `poof` particles |
| 61 | Warden | Tendril shiver for 10 ticks |
| 62 | Warden | `WARDEN_SONIC_BOOM` keyframes |
| 63 | Sniffer | Digging sound that follows the sniffer while it digs |
| 64 | Armadillo | `armadillo.peek` sound, peek keyframes restart |
| 66 | Creaking | `CREAKING_INVULNERABLE` keyframes |
| 67 | LivingEntity | 8 `bubble` particles |
| 69 | Ravager | 40 `poof` particles, knockback on the local player within 4 blocks |
| 70 | MinecartTNT | Fuse (see the TNT task) |
| 71 / 72 | AbstractBoat | Bubble column sink/launch: boat physics, not an effect; not handled |
| 1 | MinecartSpawner | Resets the display spawner's delay; spawner minecarts draw no spinning mob yet |

Camel and happy ghast have no `handleEntityEvent` overrides in 26.3. Thrown potions break through level events, not entity events.
