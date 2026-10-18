## ADDED Requirements

### Requirement: Built-in registry entries
When a `registry_data` entry arrives without data because it comes from a known pack, the client SHALL fill it from the vanilla data in the user's client jar (`data/<namespace>/<registry>/<entry>.json`), converted to NBT, for every synced registry. Filled data SHALL count against the same byte budgets as server data, lookups SHALL be capped per session, entry ids SHALL be validated before they name a jar path, and entries the jar does not have SHALL stay without data.

#### Scenario: Chat types from the jar
- **WHEN** the server sends `minecraft:chat_type` with `minecraft:emote_command` and no data
- **THEN** the entry holds the jar's decoration with `chat.type.emote` and the parameters `sender` and `content`

#### Scenario: Server data wins
- **WHEN** an entry arrives with data
- **THEN** the client keeps the server's data and does not read the jar

#### Scenario: Hostile entry names
- **WHEN** the server names entries such as `minecraft:../../x` or sends 100,000 entries without data
- **THEN** no path outside the registry folder is read and the number of jar lookups stays capped

#### Scenario: Biome colours from the jar
- **WHEN** only the client jar is installed, without a data generator folder
- **THEN** vanilla biomes get their grass, foliage and water colours from the jar's biome data
