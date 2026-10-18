## MODIFIED Requirements

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

## ADDED Requirements

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

