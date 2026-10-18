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
| Mod Menu | 21.0.0 | works | Mods button and list, update check (Modrinth is allow-listed) |
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
| Particle Rain | 4.0.2 | works | its rain and splash particles replace mcr's rain columns (`mod-camera-weather`) |
| Freecam | 1.5.0-alpha.2 | works | F4 hands the camera to Freecam: mcr renders from it, the movement keys fly it and the player stands still (`mod-camera-weather`) |
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

Freecam and Particle Rain were re-tested alone with Fabric API (and Cloth Config) at bridge ABI 10 after
`mod-camera-weather`: Freecam's F4 camera flew 20 blocks while the player stayed put and toggled back, and
Particle Rain's particles showed in a thunderless rain with mcr's columns hidden. Both left the mod policy.

## Second sweep

Sweep `mod-compat-sweep-2`, October 2026, bridge ABI 13, same setup: each mod alone with its libraries and Fabric
API 0.162.0, then everything from both sweeps together, offscreen at 1280x720 against a vanilla 26.3 server in
offline mode. The autopilot added mouse `down`/`up`/`wheel` steps for drags, a horse ride for mount HUDs and a
crafting table for Crafting Tweaks.

| Mod | Version | Status | Reason |
|---|---|---|---|
| VoxelMap | 1.16.13 | works | round minimap with coordinates; it draws through Java's GUI, not OpenGL |
| Xaero's World Map | 1.47.0 | blocked | draws with OpenGL directly |
| MiniHUD | 0.41.2 | works | info lines; its malilib hotkey (H) toggles them since key events also reach vanilla's SDL event handler |
| Status Effect Bars | 1.0.12 | works | duration bars under the effect icons |
| Armor HUD | 3.5.0 | works | armor icons left of the hotbar |
| uku's Armor HUD | 0.14.0 | works | armor icons with durability; declares a conflict with Armor HUD, so only one loads; its profile fetch fails offline |
| Better Mount HUD | 1.3.1 | works | the horse's hearts above the food bar while riding (mcr's own riding camera sits inside the horse's head, a native riding bug) |
| Overflowing Bars | 26.3.0 | works | compacted armor and health rows |
| Chat Patches | 8.0.0 | works | timestamps in front of chat lines |
| More Chat History | 2.0.0 | works | boots and joins; longer history not exercised |
| Chat Animation | 1.3.4 | works | boots and joins; the slide-in is an animation the screenshots cannot show |
| No Chat Reports | 2.21.0 | works | removes the unverified-chat toast; mcr itself never signs chat |
| Crafting Tweaks | 26.3.0.1 | works | rotate, balance and clear buttons on the crafting table (none on the 2x2 grid by default) |
| Item Scroller | 0.33.1 | works | shift-drag moves every slot dragged over; hotkeys go through malilib |
| InvMove | 0.9.6 | works | walks with the inventory open (`ISTATE_SCREEN_MOVES`) |
| Smooth Swapping | 0.9.11 | works | boots and joins; the swap animation is not visible in still shots |
| Enchantment Descriptions | 26.3.0.2 | works | description line under each enchantment |
| Legendary Tooltips | 1.6.3 | works | framed tooltips |
| Capes | 1.5.10 | works | boots and joins; the offline test player has no cape |
| WaveyCapes | 1.11.2 | works | boots and joins; no cape to wave offline |
| Ears | 2.2.1-d | works | boots and joins; the test skin has no ears |
| AmbientSounds | 6.3.10 | works | boots and joins; its sounds play through Java's sound bridge |
| Presence Footsteps | 1.14.0 | works | its footsteps replace mcr's (`FSTATE_STEPS`) |
| Sound Physics Remastered | 1.5.1 | blocked | OpenAL effects on the vanilla sound device |
| Visuality | 0.7.15 | works | boots and joins; its particles are Java's |
| Subtle Effects | 1.15.0-hotfix.1 | works | boots and joins; its particles are Java's |
| Dynamic Crosshair | 9.15 | works | its crosshair replaces the vanilla one and hides it with no target |
| LambDynamicLights | 4.13.0 | partial | policy: terrain lighting is mcr's, only Java-drawn entities could light up |
| Freelook | 1.4.4 | works | holding Alt switches Java to its default third-person view, which mcr follows (`FSTATE_PERSPECTIVE`), and the mouse turns the camera, not the player (`FSTATE_TURN`); releasing it returns to first person |
| Camera Overhaul | 2.1.2 | works | its pitch and yaw nudges reach mcr through the camera handoff; its strafe and turn roll of the bob pose rolls mcr's world (`FSTATE_VIEW_ROLL`, about 1° while strafing with the defaults) as well as the hand |
| Shoulder Surfing Reloaded | 5.2.0 | works | with the default config it starts in its shoulder view (third person back, offset sideways, decoupled camera orbiting with the mouse) and F5 cycles first person, third back, third front and the shoulder view; mcr follows Java's camera type |
| Krypton | 0.3.2 | works | boots and joins; mcr owns the socket, so its network optimisations have nothing to act on |
| Debugify | 26.3.0.0 | works | boots and joins |
| More Culling | 1.9.0 | partial | policy: terrain is rendered by mcr; no visible effect |
| Dynamic FPS | 3.11.10 | unsupported | policy: frame pacing and the window are mcr's |
| Better Block Entities | 1.3.9 | blocked | requires Sodium |

Libraries, all loading cleanly: Balm 26.3.0.3, CreativeCore 2.14.19, Forge Config API Port 26.3.1, Fzzy Config
0.7.7, GeckoLib 5.5.7, Iceberg 1.4.3.1, MaLiLib 0.30.2, PrickleMC 26.3.0.2, Prism 1.1.3, Puzzles Lib 26.3.14,
ukulib 2.2.0. Prickle's update check asks a host outside the proxy allow-list and is refused (one log line).

Freelook, Camera Overhaul and Shoulder Surfing were re-tested alone with their libraries at bridge ABI 14 after
`mod-perspective`, where Java's camera type leads mcr's perspective and mod rolls of the bob pose reach mcr's view.
Without mods F5 still cycles mcr's own first person, third back and third front.

Open items from the first sweep: Mouse Tweaks' drag-click (shift-drag moved three stacks) and wheel tweak (a
scroll moved one plank into the crafting grid) work through the new autopilot steps. Controlling's rebinding of
Jump to J reached mcr's own `options.txt`. Entity Model Features with Fresh Animations 1.10.5 animates the cow's and villager's heads, and Entity Texture
Features applied a pack's random cow texture (`optifine/random/entity/cow`).

All mods of both sweeps together (85 jars, 171 mod ids after the policy blocked or excluded eight) boot, join and
keep Java's GUI and entities composed after the REI/JEI join stall; Particle Rain and Presence Footsteps still take
over rain and footsteps, and the HUD mods share the screen (JEI's bar overlaps the hotbar at this GUI scale, as it
would in vanilla).

The block-entity section seen culled about once per frame in the first sweep is a legitimate cull: the shared
sections drop dungeon spawners and mineshaft chests underground that mcr's visibility graph never reaches (seen at
`mob_spawner@343,-33,508` with the player on the surface at y 98). The stage scenes cull nothing.

## NeoForge

Real NeoForge mods for 26.3 through `mcr --mods` (NeoForge 26.3.0.90, bridge ABI 19), all together, offscreen at
854x480 against a vanilla 26.3 server in offline mode. An autopilot joined, looked at blocks, opened the inventory
and the creative inventory, toggled F3 and third person and chatted. Change `mod-neoforge-1`, October 2026.

| Mod | Version | Status | Reason |
|---|---|---|---|
| Just Enough Items | 31.15.0.82 | works | ingredient list in the inventory and creative screens; needs MezzConfig (standalone 0.6.8 or the copy it nests) |
| Jade | 26.3.5 | works | block tooltip at the top of the screen |
| AppleSkin | 3.0.10 | works | HUD saturation overlay |
| BetterF3 | 20.0.0 | works | replaces the F3 screen; needs Cloth Config 26.3.159 |
| Chat Heads | 1.3.2 | works | player heads next to chat lines |
| 3D Skin Layers | 1.11.3 | loads | third person shows the player; layers not inspected; logs a missing optional EMF class |
| Mouse Tweaks | 2.31 | loads | boots and joins; drag-click behaviour not exercised |
| Controlling | 26.3.4 | loads | boots and joins with Searchables 1.0.3; key search not exercised |
| Freecam | 1.5.0 | loads | boots and joins; not toggled |
| EntityCulling | 1.11.3 | loads | boots and joins |
| FerriteCore | 9.0.0 | loads | boots and joins |
