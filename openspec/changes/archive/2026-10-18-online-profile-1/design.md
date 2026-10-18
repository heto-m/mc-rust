# Design

## Where things live

- `mcr-auth::chat` holds the crypto, no session state: `ChatSigner` (SHA256withRSA, PKCS#1 v1.5, from the
  certificate's private key), `ChatVerifier` (a sender's SPKI public key), the signed-body writer, and
  `ServicesKeys` (Mojang's `playerCertificateKeys`, SHA1withRSA) with `validate_profile_key`.
  `minecraft::fetch_services_keys` reads `https://api.minecraftservices.com/publickeys` (unauthenticated).
  `rsa` 0.9 with `sha2` 0.10 and `sha1` 0.10 (`oid` features); digests are fed piece by piece, so signing and
  verifying allocate nothing per message beyond the signature.
- `mcr-auth::keys::KeyRefresh` wraps `Arc<dyn Fn() -> Option<PlayerKeys> + Send + Sync>` so both the launcher and
  the client can name it without depending on each other.
- `mcr-client::secure_chat` holds the state: the local session, the last-seen tracker, the signature cache, remote
  sessions, the global chat index, and the key-refresh job. It lives in `ClientState` (`state.secure`), so the
  packet handler and the send paths in `actions.rs` share it.
- `mcr-protocol` is untouched: every packet already exists with vanilla's bounds (256-byte signatures, 20
  last-seen entries, 512-byte keys, 4096-byte key signatures, 8 argument signatures).

## Keys into the client

`OnlineSession` (launcher) and `OnlineAuth` (client) gain `keys: Option<PlayerKeys>`,
`services: Arc<ServicesKeys>` and `refresh: Option<KeyRefresh>`. `AccountEntry::launch_account` refreshes the token
as today; a failing certificate fetch now keeps unexpired cached keys or drops them (vanilla plays without keys)
instead of failing the launch, and a failing `/publickeys` fetch leaves the key set empty. The refresher is a
closure over the account entry, HTTP client, client ID and token file that runs `refresh_cached` and returns its
keys. `PlayerKeys` and `Secret` keep their redacting `Debug`; nothing logs the private key.

The client parses the private key once per connection (`ClientState::new`). Unparseable or expired keys mean "no
keys".

## Chat session

On every play `login` packet the state resets like vanilla's `handleLogin`: no session, unsigned encoder, global
index 0, fresh tracker and cache. When the packet says `online_mode`, keys exist and have not expired, a session
starts: a random v4 session UUID, a chain at index 0 for (profile UUID, session UUID), and a
`chat_session_update` queued as a reply. Offline-mode servers never get one: an offline server would check the
key against the offline UUID and kick the player.

Every 20 ticks the client checks `should_refresh`; with a session and a refresher it runs the refresher on its
own thread (one job at a time, at most one attempt per 10 minutes). Keys that differ from the current ones start a
new session and send a new `chat_session_update`, as vanilla's `setKeyPair`.

`start_configuration` sends a pending `chat_ack` before `configuration_acknowledged`.

## Signing

The signed bytes, big-endian, are vanilla's: `i32 1`, sender UUID, session UUID, `i32 index`, `i64 salt`,
`i64 timestamp seconds`, `i32 content byte length`, UTF-8 content, `i32 last-seen count`, each 256-byte
signature. The wire timestamp is milliseconds; the signed one is seconds. Salt is a `rand` CSPRNG `i64`.
`send_chat` always applies the tracker's update (even unsigned, as vanilla). The chain index advances per
signature; at `i32::MAX` signing stops (vanilla's `advance()` returns null).

### Signed commands

`CommandTree::signable_arguments(command)` walks the server's tree over the exact text, as Brigadier does: one
space separates arguments, literals match whole tokens, redirects continue from the target's children, and
argument nodes consume by parser — greedy parsers (`message`, greedy phrase, component, NBT, style, dialog) take
the rest, coordinates take 2 or 3 tokens, quoted strings run to the closing quote, selectors run to the matching
`]`, everything else one token. The first path that consumes the whole input and ends on an executable node
wins; each `minecraft:message` argument on it yields (name, text from its start to the end). Exploration is
bounded (256 live states, 16 redirects, 256 tokens). No signable argument means a plain `chat_command`. Otherwise
one timestamp, salt and tracker update are shared by every argument signature, as vanilla's `sendCommand`.

## Last-seen tracking

`LastSeenTracker` is vanilla's: a 20-slot ring of (signature, pending) or empty, a tail, an offset counter and
the last tracked signature (a repeat is ignored). `generate_update` walks the ring from the tail, sets bit `i` for
each filled slot, marks it acknowledged and returns the signatures plus `(offset, bitset, checksum)`, offset
cleared. The checksum is Java's `31 * h + Arrays.hashCode(bytes)` folded to a byte, 0 replaced by 1.
`mark_processed(signature, shown)` adds the entry (empty when not shown) and returns an ack when the offset passes
64. `ignore_pending` clears a deleted message's pending slot.

`SignatureCache` is vanilla's 128-slot move-to-front cache: after each player chat, the body's last-seen
signatures and the message's own signature are pushed. A packed cache id outside 0..128 or pointing at an empty
slot disconnects with `multiplayer.disconnect.invalid_packet`, as vanilla.

## Verification

`player_info_update` with `initialize_chat` validates the session: key DER within 512 bytes, parsed as an RSA SPKI
key of at most 4096 bits, key signature within 4096 bytes and valid for any services key over
`UUID ‖ expires_ms ‖ DER` (SHA1withRSA). Valid sessions are stored per player (bounded by the player list,
removed with the player); invalid or missing ones clear the player's session. Without services keys no session
is ever valid and secure chat counts as not enforced (vanilla's `canValidateProfileKeys`).

`player_chat` then follows vanilla's `handlePlayerChat`: a global index other than the expected one disconnects
with `multiplayer.disconnect.bad_chat_index`; the body is unpacked through the cache and pushed; an unknown sender
gives a chat error. A sender with a session is validated key-based: key expired beyond 8 h, a bad signature, or
an index that does not descend from the last accepted one (same message excepted) breaks the chain for good and
gives a chat error. A sender without one is rejected when secure chat is enforced and otherwise shown with its
signature stripped.

Trust: no signature or older than 7 minutes is not secure; decorated text not containing the signed content, or
unsigned content with a non-default font anywhere, is modified; otherwise secure (no tag). The local player's own
messages on the local singleplayer server are secure (`ClientConfig.local_server`). Only Show Secure Chat drops
unsigned content and hides not-secure messages. A fully filtered message is hidden; a partial mask replaces masked
characters with dark gray `#` (hover `chat.filtered`). The processed signature (shown or not) feeds the tracker.

`delete_chat` unpacks its signature (bad id disconnects), clears the tracker's pending slot, and emits
`ChatDeleted`. The HUD replaces the line with `chat.deleted_marker` (gray italic, system tag) once it is 60 ticks
old, delaying younger ones (at most 64 waiting).

## Trust tags in the HUD

`ClientEvent::Chat` carries `tag: Option<ChatTag>` and the message signature. `ChatHud` stores the tag per line and
draws vanilla's 2-pixel bar at the line's left edge: system and not secure `0xD0D0D0`, modified `0x606060`, error
`0xFF5555`. Modified lines reserve room for the 9×9 `icon/chat_modified` sprite after the text, drawn in open chat
when the message or icon is hovered. Hovering the bar or icon in open chat shows the tag text (`chat.tag.*`, the
modified tag adds the original text in gray). Server and disguised messages get the system tag, as 26.3 does.
`multiplayer.unsecureserver.toast` shows once per connection when a remote server does not count as enforcing
secure chat, as vanilla does for any multiplayer server (offline-mode servers included); the local singleplayer
server never shows it, like vanilla's integrated server.

## Mod mode

Java's `sendChat`/`sendCommand` already reach Rust as text through `OUT_CHAT`, and `chat`, `chat_command`,
`chat_command_signed`, `chat_ack` and `chat_session_update` from Java are already dropped by the forward filter, so
Rust's session signs everything and Rust alone acknowledges. Java runs with `--offlineDeveloperMode`, which leaves
its services key set empty, so its own HUD would mark every message not secure. With an online account the bridge
sets `-Dmcr.bridge.servicesKeys=true`, and a `Minecraft` constructor mixin turns key-set fetching back on (through the
loopback proxy, which already allows `api.minecraftservices.com`); Java then validates sessions and messages with
vanilla's own code. Java never gets profile keys (`EMPTY_KEY_MANAGER` stays). No ring or ABI change, so
`BRIDGE_VERSION` stays 19.

## Cross-check with vanilla

`crates/mcr-client/tests/vanilla_chat/ChatCrossCheck.java` (our code) is compiled and run by an opt-in test
(`MCR_JVM_TESTS=1`) against `.cache/versions/26.3/client.jar` and `.cache/libraries/*.jar` with `-Xmx256m`. Rust
writes a fixture (test public keys, signed messages, encoded `chat`/`chat_command_signed` packets, an ack
sequence); the harness decodes the packets with vanilla's stream codecs, verifies the signatures with
`PlayerChatMessage.verify`, replays the acks through `LastSeenMessagesValidator`, validates the profile key with
`ProfilePublicKey.createValidated` against a test "services" key, and signs its own messages with
`SignedMessageChain` for Rust to verify. Nothing from the jar is committed.

## Risks

- A command whose message argument our walker misses is sent unsigned; servers enforcing secure profiles answer
  with a red "invalid command signature" line instead of running it. Vanilla commands are covered by tests
  against the 26.3 tree shape.
- Clock skew over 5 minutes makes servers log expired chat; vanilla has the same behavior.
