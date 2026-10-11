## ADDED Requirements

### Requirement: Java module build
`java-build` SHALL compile each module under `java/` with the provisioned JDK 25 (`javac --release 25`), package it with `jar` into `.cache/java/<module>.jar`, and skip modules whose source hash is unchanged. It SHALL never package files from Mojang jars.

#### Scenario: Incremental build
- **WHEN** `java-build` runs twice without source changes
- **THEN** the second run compiles nothing and reports every module as up to date
