## ADDED Requirements

### Requirement: Key mapping sync
Vanilla key mappings SHALL agree between mcr and Java in both directions, with mcr's `options.txt` as the only persisted copy. Rebinding a vanilla mapping in any Java screen SHALL change mcr's binding; rebinding in mcr's controls SHALL change Java's `KeyMapping`. Mappings that mods add SHALL stay Java-only.

#### Scenario: Rebinding in Java
- **WHEN** a Java screen (vanilla Controls or a mod such as Controlling) binds jump to J
- **THEN** mcr's jump binding becomes J and is saved in mcr's `options.txt`

#### Scenario: Rebinding in mcr
- **WHEN** the user binds forward to the up arrow in mcr's controls
- **THEN** Java's forward mapping becomes the up arrow before Java's next frame

#### Scenario: Java's own file is stale
- **WHEN** Java's `options.txt` holds an older binding for a vanilla mapping at startup
- **THEN** mcr's binding wins and mcr's file is not changed

### Requirement: Lock keys from the keyboard
On Linux, the Caps Lock and Num Lock modifier bits sent to Java SHALL start from the keyboard's lock LEDs when the mod host starts.

#### Scenario: Num Lock on at startup
- **WHEN** mcr starts with mods while Num Lock is on
- **THEN** Java's first key events carry the Num Lock modifier

### Requirement: Menu backdrop
The native "mcr" menu drawn over Java's GUI SHALL cover the whole screen behind its panel so that no Java control shows around it.

#### Scenario: Wide window
- **WHEN** the "mcr" menu opens from Java's pause screen at 1280x720 and GUI scale 3
- **THEN** none of Java's pause buttons are visible around the panel
