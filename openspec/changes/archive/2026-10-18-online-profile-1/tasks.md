# Tasks

## 1. Auth crypto

- [x] 1.1 `mcr_auth::chat`: signed body writer, `ChatSigner`, `ChatVerifier`; byte fixture for the signed body; sign/verify round trip with a generated test key; wrong-length and tampered signatures rejected
- [x] 1.2 `ServicesKeys` + `validate_profile_key` (SHA1withRSA over UUID, expiry, DER); `minecraft::fetch_services_keys` with bounds; tests with a test "services" key and hostile JSON
- [x] 1.3 `KeyRefresh`; key fetch failures keep unexpired keys and never fail refresh; tests

## 2. Launcher

- [x] 2.1 `OnlineSession` carries keys, services keys and a refresher; `launch_account` fetches services keys; tests (debug hides secrets, failures tolerated)
- [x] 2.2 `mcr-app` passes them into `OnlineAuth`; `local_server` and Only Show Secure Chat into `ClientConfig`

## 3. Client state

- [x] 3.1 `LastSeenTracker` and checksum; byte fixture for the ack bitset; ack over 64; ignore pending; tests
- [x] 3.2 `SignatureCache` (vanilla push order, bounds); tests with hostile ids
- [x] 3.3 Local session: start on online login with keys, reset on each login, chain index order and exhaustion; `chat_session_update`; tests
- [x] 3.4 `send_chat` signed with tracker update; `send_command` signed/plain; tests through the outbox
- [x] 3.5 `CommandTree::signable_arguments`: literals, greedy, coordinates, quotes, selectors, redirects; bounds; tests
- [x] 3.6 Remote sessions from `initialize_chat`; removal; tests with valid and forged keys
- [x] 3.7 Player chat: global index, cache unpack, key-based validation, fallback validators, trust level, filter masks, Only Show Secure Chat, tracker feed; tests for each path
- [x] 3.8 `delete_chat` through the cache; `ChatDeleted` event; start_configuration ack; insecure-server toast
- [x] 3.9 In-game key refresh job; test with a scripted refresher

## 4. UI

- [x] 4.1 `ChatHud` tags per line: bar colors, modified icon room, tooltip lookup, deleted marker with 60-tick delay; tests
- [x] 4.2 App maps `ChatTag` to the HUD and `ChatDeleted`; chat screen tooltips

## 5. Mod mode

- [x] 5.1 Bridge property `mcr.bridge.servicesKeys` for online accounts; `Minecraft` mixin re-enables Java's key-set fetch; launch-args test
- [x] 5.2 Filter test still drops Java's chat packets; relay chat goes through Rust's signing path

## 6. Verification

- [x] 6.1 Vanilla cross-check harness (`MCR_JVM_TESTS=1`): codecs, signature verify both ways, ack validator, profile key validation
- [x] 6.2 Live offline regression on port 25573: native join, chat, `/say`, `/msg`; same in mod mode with `.cache/play-mods`
- [x] 6.3 `cargo fmt`, clippy `-D warnings`, `cargo test --workspace` (capped)
- [x] 6.4 Owner test steps for online mode in the report
