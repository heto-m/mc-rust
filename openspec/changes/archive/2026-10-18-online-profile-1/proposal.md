# Proposal

## Why

The launcher signs in with Microsoft and fetches the player's chat signing keys, but the client drops them: chat
and commands are always unsigned and incoming player chat is never verified. On online-mode servers with
`enforce-secure-profile=true` (vanilla's default) a player without a chat session cannot chat, and nothing tells
the player which messages are verified.

## What Changes

- **Profile keys reach the client.** `PlayerKeys` and Mojang's player-certificate keys (`/publickeys`) flow from
  launch into `ClientConfig.online`; a refresher re-fetches keys in-game when `refreshedAfter` passes. Key or
  key-set fetch failures no longer fail the launch. The private key is never logged. Offline accounts have none.
- **Chat session.** After the play login of an online-mode server, the client sends `chat_session_update`
  (random session UUID, expiry, public key DER, Mojang signature v2), like vanilla; again after every login
  packet (reconfiguration, rejoin) and after a key refresh.
- **Signed outgoing chat and commands.** SHA256withRSA over vanilla's signed body; the last-seen tracker with
  `chat_ack` (offset over 64 and before reconfiguration) and the offset/bitset/checksum update on every message;
  `chat_command_signed` with a signature for each `minecraft:message` argument found by walking the server's
  command tree, plain `chat_command` otherwise; the message chain index in order, reset on each login packet.
- **Incoming verification.** Sender keys from `player_info_update` (`initialize_chat`) checked against Mojang's
  keys; each sender's chain checked (signature, expiry with 8 h grace, index order); vanilla's global chat index
  and 128-entry signature cache; `delete_chat` by signature, shown as vanilla's deleted marker. Trust tags in the
  chat HUD (not secure, modified with icon and original text, chat error, system) with tooltips in open chat;
  vanilla's Only Show Secure Chat option; the insecure-server toast (as vanilla, also on offline-mode servers).
- **Mod mode.** Java already hands chat text to Rust, which now signs it. Java gets Mojang's key set (property for
  online accounts) so its own chat HUD shows the same trust tags. No ABI change.
- **Tests.** Fixed-key unit tests, byte fixtures for the signed body and ack bitset, sign/verify round trips, and
  an opt-in cross-check that runs vanilla 26.3's own chat classes (from the user's client jar) against our
  signatures and acknowledgements.

## Out of scope

Chat reporting and the chat log, blocked players and friend-only restrictions, chat delay queue, profanity
filter masks beyond vanilla's display of server-sent masks, live online-mode testing (owner steps only),
protocol translators for older versions.

## Capabilities

### New Capabilities
- `secure-chat`: chat sessions, message signing and verification, last-seen tracking, trust tags.

### Modified Capabilities
- `account-auth`: Mojang services keys; key refresh tolerant of failures.
- `launcher`: keys, services keys and a key refresher passed to the client at launch.
- `mod-net-bridge`: Java chat signed by Rust; Java verifies with Mojang's key set.
