# ui-text Specification

## Purpose
Vanilla-faithful font loading, glyph metrics, component flattening and word wrapping for the 2D UI layer.

## Requirements

### Requirement: Vanilla font definitions
The system SHALL load the `default`, `uniform`, `alt` and `illageralt` fonts from the client jar, supporting `bitmap`, `space`, `unihex` and `reference` providers with `uniform` filters, and SHALL fall back to a 5x8 outlined missing glyph for unknown code points.

#### Scenario: Bitmap advance
- **WHEN** the default font is queried for `i`, `W` and space
- **THEN** their advances are 2, 6 and 4

#### Scenario: Reference resolution
- **WHEN** `alt` is loaded
- **THEN** the space provider referenced by it supplies the space advance

### Requirement: Lazy glyph atlas
The system SHALL rasterize glyphs into atlas pages on first use, so that large unihex ranges cost nothing until drawn.

#### Scenario: On-demand upload
- **WHEN** a glyph is first drawn
- **THEN** it is packed into a page and only dirty regions are uploaded

### Requirement: Vanilla glyph effects
The system SHALL draw shadows offset by the glyph shadow offset with colour rgb*0.25, bold as a second draw offset by the bold offset, italic as a vertical shear, and underline and strikethrough as 1px bars.

#### Scenario: Bold widens advance
- **WHEN** a bold `i` is measured
- **THEN** its advance is 3

### Requirement: Component flattening
The system SHALL flatten a `Component` tree into styled characters, inheriting style from parents, resolving translatable content through `lang/en_us.json` with `%s`, `%n$s` and `%%`, and interpreting `§` codes (colours, formatting, `r` reset).

#### Scenario: Legacy code
- **WHEN** `§cRed§r plain` is flattened
- **THEN** the first three characters are red, the rest unstyled and the `§` pairs are not drawn

#### Scenario: Translation
- **WHEN** `chat.type.text` with arguments `Steve` and `hi` is flattened
- **THEN** the text is `<Steve> hi`

### Requirement: Word wrapping
The system SHALL break lines at the last space before overflow, at the overflowing character when no space exists, at `\n`, and SHALL always place at least one character per line, matching vanilla `StringSplitter`.

#### Scenario: Break at space
- **WHEN** `aaa bbb ccc` is wrapped to the width of seven `a`
- **THEN** the lines are `aaa bbb` and `ccc`

#### Scenario: No space
- **WHEN** a single word wider than the limit is wrapped
- **THEN** it is split at the overflowing character
