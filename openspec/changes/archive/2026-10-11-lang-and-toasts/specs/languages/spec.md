## ADDED Requirements

### Requirement: Language stack
The system SHALL build translations from `en_us` overlaid by the selected language, merged key by key across all packs in order, then apply the renamed and removed key table.

#### Scenario: Fallback to English
- **WHEN** the selected language lacks a key that `en_us` defines
- **THEN** the `en_us` text is used

#### Scenario: Pack override
- **WHEN** a later pack redefines a key
- **THEN** the later pack's text wins and untouched keys keep their earlier text

### Requirement: Language list
The system SHALL list the languages declared by `pack.mcmeta` language sections, sorted by code, with name, region and bidirectional flag.

#### Scenario: Declared languages
- **WHEN** the pack metadata declares `en_us` and `ja_jp`
- **THEN** both appear in the list with their names and regions

### Requirement: Live language selection
The system SHALL provide a scrolling Language screen in Options, store the choice as `lang` in `options.txt` and re-translate the UI without a restart.

#### Scenario: Switching language
- **WHEN** the player picks a language and confirms
- **THEN** menus, tooltips and new chat messages use that language
