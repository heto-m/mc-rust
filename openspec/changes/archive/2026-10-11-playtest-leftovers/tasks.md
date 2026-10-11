## 1. Hurt sounds

- [x] 1.1 Verify `damage_event` effects resolution and the local hurt sound against vanilla `handleDamageEvent`
- [x] 1.2 Confirm other entities' hurt and fall sounds come from server sound packets; local fall sounds stay client-side
- [x] 1.3 Handler test: lava, fire, drowning, berry bush, freezing, fall and a custom freezing type

## 2. Underwater state

- [x] 2.1 Share vanilla `getHeightForCamera` between the physics eye check and a public camera check
- [x] 2.2 Ambience, water vision and camera fog use the physics eye state or the shared camera check
- [x] 2.3 Refresh passengers' fluid contact every ride tick
- [x] 2.4 Tests: sinking to a pool bottom keeps the eye in water and enters once; camera height rule; passengers

## 3. Item pickup

- [x] 3.1 Verify the `take_item_entity` path against vanilla `ItemPickupParticle`
- [x] 3.2 Drop pickups with the entities on world rebuild and reconfiguration
- [x] 3.3 Handler tests: whole stack flies over 3 ticks and is removed; partial stack stays

## 4. Water merge check

- [x] 4.1 Render a flat vanilla ocean with the screenshot mode, merged and pre-merge, and compare
