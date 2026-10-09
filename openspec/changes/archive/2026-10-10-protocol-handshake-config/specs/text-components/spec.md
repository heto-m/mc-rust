## ADDED Requirements

### Requirement: Component model and network NBT
The system SHALL model text components with text, translatable, keybind, score, selector, NBT and object
contents, a style and siblings, and SHALL encode and decode them as network NBT.

#### Scenario: Plain text collapses
- **WHEN** an unstyled sibling-free text component is encoded
- **THEN** it is a bare string tag, and decoding a bare string tag yields that component

#### Scenario: Round trip
- **WHEN** a styled component with siblings, translation arguments and events is encoded and decoded
- **THEN** the decoded component equals the original

#### Scenario: List form
- **WHEN** a list of components is decoded
- **THEN** the first becomes the root and the rest are appended as siblings

### Requirement: Lenient decoding
The system SHALL ignore unknown style keys, and in lenient mode SHALL replace malformed contents or
style values with defaults instead of failing.

#### Scenario: Unknown style key
- **WHEN** a component compound contains a style key the model does not know
- **THEN** decoding succeeds and the key is ignored

### Requirement: Plain string rendering
The system SHALL render a component and its siblings to plain text without styling.

#### Scenario: Translatable arguments
- **WHEN** a translatable component with fallback `Hi %s` and argument `Bob` is rendered
- **THEN** the result is `Hi Bob`
