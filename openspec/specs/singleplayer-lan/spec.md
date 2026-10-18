# singleplayer-lan Specification

## Purpose
Open to LAN for a local world: a login-gated forwarder in front of the loopback server, the LAN announcement, and the host's choices for other players.

## Requirements

### Requirement: LAN forwarder
Opening a local world to LAN SHALL keep the server bound to loopback and SHALL start a TCP forwarder listening on the chosen port of all IPv4 interfaces (any free port when none is given), piping each connection to the loopback server with blocking threads, fixed-size buffers and at most 16 live connections. It SHALL stop, closing every forwarded connection, when the world is saved and quit. `MCR_LAN_BIND` SHALL override the bind address; automated tests SHALL only bind loopback.

#### Scenario: Guest joins through the forwarder
- **WHEN** the host opens the world to LAN on port 25601 and a second client connects to that port
- **THEN** the second client joins the host's world through the loopback server

#### Scenario: Connection flood
- **WHEN** more than 16 connections are open at once
- **THEN** further connections are closed at once and the live ones keep working

### Requirement: LAN opening checks
Before piping a LAN connection, the forwarder SHALL decode its handshake and, for login or transfer, its login start, reading at most 1 KiB with capped frame lengths and within 5 seconds. Status pings SHALL pass unchanged. Anything else SHALL be closed without reaching the server.

#### Scenario: Slow or garbage opening
- **WHEN** a LAN client sends garbage, an oversized frame, or only part of its handshake for 5 seconds
- **THEN** the connection is closed without reaching the server, and other clients can still join meanwhile

### Requirement: No host impersonation over LAN
A LAN login whose name matches the host's name ignoring case, whose UUID is the host's offline UUID, or whose name is already online through the forwarder SHALL get a login disconnect and be closed. `ops.json` SHALL list only the host, and Allow Cheats SHALL apply only to the host.

#### Scenario: Guest uses the host's name
- **WHEN** a LAN client logs in as `HOST` while the host is `Host`
- **THEN** it receives a login disconnect and the server never sees the connection

#### Scenario: Guest sends the host's UUID
- **WHEN** a LAN client logs in as `Other` with the host's offline UUID
- **THEN** it receives a login disconnect

#### Scenario: Duplicate guest name
- **WHEN** `Guest` is online through the forwarder and another LAN client logs in as `guest`
- **THEN** the second client receives a login disconnect and the first stays connected

### Requirement: LAN announcement
While a world is open to LAN, mcr SHALL send `[MOTD]<motd>[/MOTD][AD]<port>[/AD]` to `224.0.2.60:4445` every 1.5 seconds, with `<player> - <world name>` as the MOTD stripped of control characters and square brackets and cut at 64 characters, and SHALL stop when the world is saved and quit. With a loopback bind address the announcement SHALL go to `127.0.0.1:4445` instead.

#### Scenario: Hostile world name
- **WHEN** the world is named `x[/MOTD][AD]25565[/AD]` and opened to LAN on port 25601
- **THEN** the announcement advertises only port 25601

### Requirement: LAN screen
Open to LAN SHALL offer game mode (survival, creative, adventure, spectator), allow cheats and a port (empty for any free port, or 1024 to 65535), refusing ports that cannot be bound. In hardcore both choices SHALL be locked.

#### Scenario: Port taken
- **WHEN** the host enters a port another program listens on
- **THEN** the screen shows vanilla's port unavailable message and Start LAN World is inactive

### Requirement: LAN game mode and cheats
Starting LAN SHALL send `defaultgamemode <mode>` and `gamemode <mode> <host>` to the server console, and `op <host>` or `deop <host>` when allow cheats differs from the world's setting; chat SHALL show vanilla's "now open to LAN" line with the port, or vanilla's failure line.

#### Scenario: Creative with cheats
- **WHEN** the host of a survival world without cheats opens it to LAN with creative and cheats on
- **THEN** the console receives `defaultgamemode creative`, `gamemode creative <host>` and `op <host>`, and chat shows the port

### Requirement: No pause while open to LAN
While the world is open to LAN, the pause menu SHALL NOT freeze the server tick.

#### Scenario: Pause with a guest
- **WHEN** the host opens the pause menu while the world is open to LAN
- **THEN** the console receives no `tick freeze`

### Requirement: LAN in mod mode
In mod mode Java's World Options screen SHALL show an Open to LAN button in a local world that opens the same choices in Java and sends them to mcr; mcr SHALL answer with the port or the failure, and Java SHALL show vanilla's chat line. Once published the button SHALL be inactive and show the port.

#### Scenario: Java opens LAN
- **WHEN** the player opens World Options, clicks Open to LAN and starts it with the defaults
- **THEN** mcr starts the forwarder and announcer and Java's chat shows the port
