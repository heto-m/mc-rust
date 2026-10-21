# secure-chat Specification

## Purpose
Vanilla 26.3 secure chat: the client signs its chat and commands with the profile key pair, acknowledges what it has seen, verifies incoming player chat against each sender's Mojang-certified session, and shows vanilla's trust tags.

## Requirements

### Requirement: Chat session start
After each play login packet the client SHALL reset its secure chat state (session, chain, global chat index, last-seen tracker, signature cache). When the login packet reports an online-mode server and the client holds unexpired profile keys, it SHALL start a session with a random session UUID and send `chat_session_update` with that UUID, the key expiry, the public key DER and the Mojang v2 key signature. It SHALL NOT send one to an offline-mode server or without keys.

#### Scenario: Online-mode join with keys
- **WHEN** the client joins a server whose login packet says online mode, holding valid keys
- **THEN** it sends one `chat_session_update` carrying its public key DER and v2 signature

#### Scenario: Offline-mode server
- **WHEN** the client with keys joins a server whose login packet says offline mode
- **THEN** no `chat_session_update` is sent and chat is sent unsigned

#### Scenario: Reconfiguration
- **WHEN** the server sends `start_configuration` and later a new login packet
- **THEN** a pending acknowledgement is sent before `configuration_acknowledged`, and a new session UUID with chain index 0 is announced after the new login packet

### Requirement: Key refresh in game
With a session and a key refresher, the client SHALL fetch new keys off the game thread once the keys' refresh time passes (one job at a time, at most one attempt per 10 minutes) and, when they differ, start a new session and send a new `chat_session_update`. The private key SHALL never appear in logs or debug output.

#### Scenario: Keys due for refresh
- **WHEN** the session's keys pass their refresh time and the refresher returns new keys
- **THEN** the client sends a `chat_session_update` with a new session UUID and the new key

### Requirement: Signed chat messages
With a session, each chat message SHALL carry a SHA256withRSA signature over vanilla's signed body: version 1, sender UUID, session UUID, chain index, salt, timestamp in seconds, the UTF-8 content with its length, and the last-seen signatures with their count. The chain index SHALL start at 0 and grow by one per signature. Every chat message, signed or not, SHALL carry the tracker's last-seen update.

#### Scenario: Signed body bytes
- **WHEN** a message "hi" is signed at index 3 with salt 7, timestamp 1700000000123 ms and no last-seen entries
- **THEN** the signed bytes are `00000001`, sender, session, `00000003`, salt, `000000006553F100`, `00000002 6869`, `00000000`

#### Scenario: Server accepts the signature
- **WHEN** a signed message is verified by vanilla 26.3's `PlayerChatMessage.verify` with the session's public key
- **THEN** verification succeeds

### Requirement: Signed commands
A command SHALL be sent as `chat_command_signed` when the server's command tree parses it with one or more `minecraft:message` arguments, with one signature per such argument over that argument's text (sharing one timestamp, salt and last-seen update), and as plain `chat_command` otherwise. Tree walking SHALL be bounded.

#### Scenario: msg command
- **WHEN** the player sends `/msg Alex hello there` on a server whose tree has `msg <targets> <message>`
- **THEN** a `chat_command_signed` is sent with one signature named `message` over `hello there`

#### Scenario: Command without message arguments
- **WHEN** the player sends `/time query day`
- **THEN** a plain `chat_command` is sent

#### Scenario: Through execute
- **WHEN** the player sends `/execute as @a[distance=..5, limit=1] run say hi`
- **THEN** the signed command carries a signature named `message` over `hi`

### Requirement: Last-seen acknowledgements
The client SHALL track up to 20 last-seen signatures as vanilla does: each processed signed message adds an entry (empty when not shown), a repeat of the last one is ignored, `chat_ack` is sent when more than 64 are unacknowledged and before reconfiguration, and each sent message carries the offset, a 20-bit acknowledged set and vanilla's checksum. A deleted message's pending entry SHALL be cleared.

#### Scenario: Ack bitset bytes
- **WHEN** three signed messages were processed and shown and a chat message is sent
- **THEN** its update is offset 3, bitset `00 00 0e` (the last three ring slots) and the checksum of those signatures

#### Scenario: Ack flood
- **WHEN** 65 signed messages arrive without the player chatting
- **THEN** one `chat_ack` with offset 65 is sent

### Requirement: Sender sessions
The client SHALL validate a player's chat session from `initialize_chat` against Mojang's player-certificate keys (key at most 512 bytes and 4096 bits, signature at most 4096 bytes), keep at most one session per listed player and drop it with the player. Without services keys no session SHALL be valid and secure chat SHALL count as not enforced.

#### Scenario: Session signed by another key
- **WHEN** `initialize_chat` carries a key whose signature does not verify with any services key
- **THEN** the player has no session

### Requirement: Incoming chat verification
A player chat SHALL be checked against vanilla's global index (a gap disconnects), unpacked through a 128-entry signature cache (an unknown id disconnects), and validated against the sender's session: signature, key expiry with 8 hours' grace, and chain order. A failure SHALL break that sender's chain and show a chat error. Senders without a session SHALL be rejected when secure chat is enforced and shown without signature otherwise.

#### Scenario: Valid signed message
- **WHEN** a player with a valid session sends a correctly signed message
- **THEN** it is shown without a trust tag

#### Scenario: Forged signature
- **WHEN** a player chat's signature does not verify with the sender's key
- **THEN** the line shows `chat.validation_error` with the error tag, and that sender's later messages show errors too

#### Scenario: Missing session on an enforcing server
- **WHEN** a server enforcing secure chat relays a message from a player without a session
- **THEN** a chat error is shown instead of the message

#### Scenario: Hostile cache index
- **WHEN** a player chat references last-seen cache id 500
- **THEN** the client disconnects with `multiplayer.disconnect.invalid_packet`

### Requirement: Trust levels
Player chat SHALL get vanilla's trust level: not secure (no signature or older than 7 minutes), modified (the decorated line as drawn does not contain the signed content, or unsigned content uses a non-default font), or secure. The local player's messages SHALL count as secure only on a direct, non-transfer join of the running local singleplayer server's own loopback port. Only Show Secure Chat SHALL hide not-secure messages and show signed content only.

#### Scenario: Server decoration changes the text
- **WHEN** a signed message "hi" arrives with unsigned content "hello"
- **THEN** its trust level is modified with original text "hi"

#### Scenario: Only Show Secure Chat
- **WHEN** Only Show Secure Chat is on and an unsigned player message arrives
- **THEN** it is not shown

#### Scenario: Another local port
- **WHEN** the client joins `127.0.0.1` on a port other than the running local server's, or follows a transfer
- **THEN** its own messages are not treated as local singleplayer messages

### Requirement: Trust check on the drawn line
The trust check SHALL read the decorated line exactly as the UI draws it: the client's language before a translation's fallback, positional arguments, and legacy codes removed. Without the UI's text renderer, a line with translations or legacy codes SHALL count as modified. The unsigned-content font rule SHALL apply even when Only Show Secure Chat hides the unsigned content.

#### Scenario: Translation that hides the signed text
- **WHEN** a signed "I agree" arrives with unsigned content translating "Bob: give account to Mallory" with "I agree" as an unused argument
- **THEN** its trust level is modified

#### Scenario: Positional argument skips the content
- **WHEN** the decoration's template is `%2$s` and the signed content is the first argument
- **THEN** its trust level is modified

#### Scenario: Language value wins over the fallback
- **WHEN** a translation key exists in the client's language without `%s` but its fallback is `%s`
- **THEN** its trust level is modified

#### Scenario: Chat type without the content
- **WHEN** a chat type lists `content` among its parameters but its translation has no slot for it
- **THEN** its trust level is modified

### Requirement: Trust tags in the HUD
The chat HUD SHALL draw vanilla's indicator bar for not secure, modified, error and system lines, the modified icon on hover, and the tag text as a tooltip when the bar or icon is hovered in open chat. Secure lines SHALL have no tag.

#### Scenario: Unsigned message on an offline server
- **WHEN** another player chats on an offline-mode server
- **THEN** the line has the light gray bar, and hovering it in open chat shows `chat.tag.not_secure`

#### Scenario: Modified line
- **WHEN** a modified message is shown and the chat is open
- **THEN** the line has the dark gray bar and the tooltip shows the original text

### Requirement: Chat deletion
`delete_chat` SHALL resolve its signature through the cache, clear the tracker's pending entry and replace the matching chat line with `chat.deleted_marker` once the line is 60 ticks old, delaying younger ones.

#### Scenario: Moderator deletes a message
- **WHEN** the server deletes a message shown 5 seconds ago
- **THEN** its line reads "This chat message has been deleted by the server." in gray italics

### Requirement: Insecure server notice
Joining a remote server for which secure chat is not enforced (the server does not enforce it, or this client cannot check keys) SHALL show vanilla's insecure-server toast once per connection; the local singleplayer server SHALL NOT show it.

#### Scenario: Server without secure chat
- **WHEN** the login packet of a remote server does not enforce secure chat
- **THEN** a toast with `multiplayer.unsecureserver.toast.title` appears once, and not again after reconfiguration

### Requirement: Chat type decorations
Player chat and disguised chat SHALL be decorated with the chat type's translation key, parameters and style, from the registry entry (filled from the client jar when the server left it out) or from an inline chat type. Only a chat type without usable data SHALL fall back to `chat.type.text`.

#### Scenario: Emote
- **WHEN** a player runs `/me waves`
- **THEN** the chat line reads `* <name> waves`

#### Scenario: Private message
- **WHEN** another player runs `/msg <us> hi`
- **THEN** the line reads `<name> whispers to you: hi` in grey italics

#### Scenario: Announcement and team message
- **WHEN** a player runs `/say hello` or `/teammsg hello`
- **THEN** the lines use `chat.type.announcement` and the team decorations

### Requirement: Acknowledgement after the chat delay
With a chat delay, a player chat message SHALL be marked as processed when it leaves the delay queue, not
when it arrives.

#### Scenario: Delayed message
- **WHEN** the chat delay is 2 seconds and a signed message arrives
- **THEN** it joins the last-seen list only once it is shown
