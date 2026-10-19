## ADDED Requirements

### Requirement: ZGC gives memory back
With ZGC, the JVM SHALL run with a soft maximum heap of two thirds of `-Xmx` and an uncommit delay of five
seconds, so the heap a load grew is returned to the system once steady play needs less. A collector chosen through
`MCR_JVM_EXTRA` SHALL get neither option.

#### Scenario: Steady play after the join
- **WHEN** Medieval Immersed (2304 MiB heap) plays for a minute after the join
- **THEN** the JVM commits about 1.5 GiB of heap instead of 2.3 GiB

### Requirement: Parallel startup scans
The mod folder scan and the AOT cache key's jar hashes SHALL run on up to eight threads and keep the jars' order.

#### Scenario: Large pack
- **WHEN** mcr starts with 102 mod jars
- **THEN** the scan and the key take under 0.1 s each instead of 0.35 s and 0.16 s
