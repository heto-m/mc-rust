# protocol-items Specification

## Purpose
Wire codecs for item stacks, data components, component patches and hashed stacks in protocol 777.

## Requirements

### Requirement: Item stack codec
`ItemStack` SHALL encode as a VarInt count (0 for empty), a VarInt item id and a component patch; a count of zero or less SHALL decode as empty with nothing further read. `NonEmptyItemStack` SHALL reject empty stacks, and `ItemStackTemplate` SHALL encode item id, count and patch with no empty form.

#### Scenario: Plain stack
- **WHEN** a stack of 64 items with id 1 and no components is encoded
- **THEN** the bytes are `40 01 00 00`

#### Scenario: Empty stack
- **WHEN** the empty stack is encoded
- **THEN** the single byte `00` is written and decoding yields the empty stack

#### Scenario: Empty rejected
- **WHEN** `00` is decoded as a `NonEmptyItemStack`
- **THEN** decoding fails

### Requirement: Component patch
A `DataComponentPatch` SHALL encode the added count, the removed count, each added component as its type id followed by its payload, then the removed type ids. All 122 component types of 26.3 SHALL be decodable and encodable, with type ids equal to the `data_component_type` registry order.

#### Scenario: Named sword
- **WHEN** a diamond sword with a custom name and a damage value is round-tripped
- **THEN** the decoded stack equals the original

#### Scenario: Unknown component id
- **WHEN** a patch names a type id above the highest registered
- **THEN** decoding fails without panicking

#### Scenario: Registry agreement
- **WHEN** the generated registry report exists
- **THEN** every component name maps to the id listed there

### Requirement: Untrusted stacks
`UntrustedItemStack` SHALL prefix each added component payload with its byte length so that unknown payloads can be skipped.

#### Scenario: Length prefix
- **WHEN** an untrusted stack with one `damage` component of 5 is encoded
- **THEN** the payload is written as `01 05`

### Requirement: Hashed stacks
`HashedStack` SHALL encode a presence boolean, the item id, the count, a map of component type id to 32-bit hash and a set of removed type ids, each limited to 256 entries.

#### Scenario: Round trip
- **WHEN** a hashed stack with two hashes and one removal is encoded and decoded
- **THEN** the result equals the original

#### Scenario: Too many entries
- **WHEN** a hashed map declares 257 entries
- **THEN** decoding fails

### Requirement: Hostile input
Decoding SHALL fail with an error, never panic or allocate proportionally to a declared count, on oversized counts, deep nesting or truncated input.

#### Scenario: Huge count
- **WHEN** a patch declares 2 billion added components in a few bytes
- **THEN** decoding fails with an error

#### Scenario: Deep nesting
- **WHEN** a container component nests stacks thousands of levels deep
- **THEN** decoding fails with an error
