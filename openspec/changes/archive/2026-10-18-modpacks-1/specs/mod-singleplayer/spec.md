## ADDED Requirements

### Requirement: Fabric server for content mods
When the active Fabric mods include a mod that runs on the server and ships content (block states or data other than tags), mcr's managed server SHALL be a Fabric server (Fabric's server launcher over the same server jar, libraries from the cache) with those jars listed in `mcr-mods.txt`, in the same memory-capped scope with the same pid-file proof. It SHALL count as ready only once it answers a `list` command on its console. Without such mods the vanilla launch SHALL be unchanged.

#### Scenario: Echoes of the Sift world
- **WHEN** the player opens a world with Echoes of the Sift's mods active
- **THEN** the log says "Fabric with 12 mods" and the player joins once the server answers `list`

### Requirement: Fabric server behind the egress fence
The managed Fabric server SHALL get the client JVM's egress fence: a localhost-only `jdk.net.hosts.file` and `http(s).proxyHost`/`proxyPort` pointing at the bridge's loopback proxy with the same allow-list. Without a running proxy the Fabric server SHALL not start.

#### Scenario: Server command line
- **WHEN** a modded world starts
- **THEN** the server's command line carries the hosts file and proxy properties before the main class
