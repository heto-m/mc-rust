# Mod compatibility

Real Fabric mods for Minecraft 26.3 through `mcr --mods` (Fabric Loader 0.19.5, bridge ABI 8), offscreen at
1280x720 against a vanilla 26.3 server in offline mode. Each mod was booted alone with its libraries, then all
together; an autopilot joined, opened the inventory, hovered items, toggled F3, zoom and third person, chatted and
looked at blocks, taking screenshots. Sweep `mod-compat-sweep-1`, October 2026.

**Works** means it boots, joins and its main feature shows. **Partial** means it runs but part of what it does has
no effect under mcr. **Broken** means it loads but its feature does not work. **Blocked** means the mod policy
refuses to load it and the Mods screen gives the reason.

| Mod | Version | Status | Reason |
|---|---|---|---|
| Roughly Enough Items | 26.3.824 | works | item list, search and recipe screens; its join-time plugin reload stalls Java for over 2 s, after which mcr re-attaches |
| Just Enough Items | 31.10.0.63 | works | item list and recipes; same join-time stall as REI |
| AppleSkin | 3.0.10 | works | food tooltips and HUD saturation overlay; harmless warning about a missing JEI mixin target |
| Jade | 26.3.5 | works | block tooltip at the top of the screen |
| Mod Menu | 21.0.0 | works | Mods button and list; its update check is refused by mcr's HTTP proxy (Modrinth is not allow-listed), logged as one line |
| Entity Model Features | 3.3.11 | works | boots and entities render; was rejected before: its manifest has a raw newline inside a string; no CEM pack tested |
| Entity Texture Features | 7.2.5 | works | boots and entities render; no random-texture pack tested |
| Not Enough Animations | 1.12.6 | works | third-person animations; its config could not be written before Java ran from its game dir |
| 3D Skin Layers | 1.11.3 | works | 3D skin layers in third person and the inventory; same config fix |
| Falling Leaves | 2.0.8 | works | leaf particles drawn by Java into mcr's world (sparse at the default rate) |
| BetterF3 | 20.0.0 | works | replaces the F3 screen |
| Zoomify | 2.17.0 | works | zoom key narrows mcr's camera; had no effect before Java's FOV reached Rust |
| Mouse Tweaks | 2.31 | works | boots and joins; drag-click behaviour not exercised by the autopilot |
| Inventory Profiles Next | 2.3.8 | works | sort and profile buttons in containers; update check refused by the proxy, logged as one line |
| Shulker Box Tooltip | 5.4.2 | works | shulker box contents tooltip |
| Controlling | 26.3.4 | works | key search works; rebinding vanilla keys in Java rebinds mcr's (`mod-polish-2`) |
| Chat Heads | 1.3.2 | works | player heads next to chat lines |
| Durability Tooltip | 1.2.0a | works | durability line on tool tooltips |
| Continuity | 3.0.1 | partial | policy: terrain is rendered by mcr, so connected textures have no visible effect |
| Particle Rain | 4.0.2 | partial | policy: weather is drawn by mcr; its particles add to mcr's rain instead of replacing it |
| Freecam | 1.5.0-alpha.2 | broken | policy unsupported (off by default): the camera is owned by mcr, so toggling does nothing and the player still moves |
| Lithium | 0.26.2 | works | boots and joins; was silently excluded before mcr counted the loader's MixinExtras as provided |
| FerriteCore | 9.0.0 | works | boots and joins (memory savings are Java-side) |
| EntityCulling | 1.11.3 | works | culls Java's entity draws; config written once Java ran from its game dir |
| Sodium | 0.9.3-alpha.1 | blocked | replaces the OpenGL renderer; mcr renders terrain natively |
| ImmediatelyFast | 1.17.1 | blocked | replaces the OpenGL immediate-mode renderer |
| Xaero's Minimap | 26.6.0 | blocked | draws with OpenGL directly |
| JourneyMap | 6.0.10 | blocked | draws with OpenGL directly |

Libraries, all loading cleanly: Fabric API 0.162.0, Cloth Config 26.3.159, Architectury 22.0.5, YACL 3.9.7,
Fabric Language Kotlin 1.14.2, libIPN 6.9.0, Searchables 1.0.3, mezz_config 0.6.8, Placeholder API 3.2.0,
SuperMartijn642's Config Lib 1.1.8a.

Not released for Fabric 26.3 when swept: EMI, WTHIT, ModernFix.

All 28 mods and 10 libraries together (107 mod ids) boot, join and keep Java's GUI and entities composed after
the REI/JEI stall. Vanilla `minecraft:*` particles from packets and vanilla ticks are mcr's; particles of mod types, and vanilla ones
that mod code spawns, are drawn by Java.
