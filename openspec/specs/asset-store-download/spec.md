# asset-store-download Specification

## Purpose
Downloads the launcher-layout asset index and objects needed for sounds and other runtime assets.

## Requirements

### Requirement: Asset store download
`cargo xtask assets` SHALL download the version's asset index to `.cache/assets/indexes/<id>.json` and every object to `.cache/assets/objects/<first two hash chars>/<hash>`, in parallel, verifying SHA-1 and skipping objects that already exist and verify.

#### Scenario: Object path
- **WHEN** an object has hash `abcdef...`
- **THEN** its destination is `objects/ab/abcdef...` and its URL is `https://resources.download.minecraft.net/ab/abcdef...`

#### Scenario: Existing object skipped
- **WHEN** an object file exists with a matching SHA-1
- **THEN** no download happens for it

#### Scenario: Corrupt download
- **WHEN** a downloaded object's SHA-1 does not match the index
- **THEN** the file is removed and the command fails
