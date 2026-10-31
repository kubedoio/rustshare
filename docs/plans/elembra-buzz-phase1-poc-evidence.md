# Phase 1: upstream Buzz interoperability evidence

Date: 2026-10-31

## Phase 1b execution

**Decision: Phase 1 remains incomplete; Phase 2 is NO-GO.** The stock relay passed core membership, channel, kind-9, NIP-29 DM discovery, tenant separation and recent-timestamp NIP-17 checks. Standard randomized NIP-59 timestamps failed the upstream relay's timestamp limit. The unmodified desktop window launched with a verified unlocked OS keyring, but GUI input did not respond in this Xvfb session, so identity import and messaging are unverified. ADR-0044 has not received explicit human security acceptance.

### Exact sources and runtime

| Artifact | Exact revision / digest | Clean-tree or runtime evidence |
|---|---|---|
| RustShare base | `36a058112324a9da10558bc9b9fae8f2bb565ef4` | Protected `main` matched `git ls-remote origin refs/heads/main` before work. |
| `block/buzz` relay | commit `eafd77e9f6e96052476bdd88be732c046fdb7ab3`; tree `04e3a40545bd3b23510ec32b4c8cccee2acef7c5` | Exact SHA/tree verified; `git status --porcelain` empty; upstream checkout unchanged. |
| `block/buzz-app` desktop | commit `662e91800af8204ec0e492522b8f23ef23b19baa`; tree `80275983372cc8dd840e732c0bb3610e97c7c283` | Exact SHA/tree verified; audited source checkout clean and unchanged. Build ran in a separate disposable clone. |
| Locally built relay image | Final review rerun config ID and Buildx build digest `sha256:b8232baaa663c672cedc2f1ca379bd3fcc88d264e97d035338d9754e383c41eb` | Built from the exact relay source and its Dockerfile. Local image digest, not a published GHCR digest. |
| Test runtime | Docker `29.7.2`; Compose `5.5.0`; Node `v22.22.2`; npm `10.9.7`; pnpm `11.8.0`; Rust `1.98.1` | Desktop package requires Node `>=24`; the app nevertheless built/launched under Node 22 with an engine warning. |

### Reproduction and environment

Protocol suite command:

```sh
BUZZ_SOURCE_DIR=/tmp/block-buzz-phase0 POC_HOLD=0 scripts/upstream-buzz-poc/run.sh
```

The final run used two independent disposable upstream Compose stacks, each with PostgreSQL 17, Redis 7, Silo object storage, an independent relay key and closed community. Authentication and membership enforcement were enabled. Only loopback ports were published. The run completed cleanup and returned exit status 1 because the required randomized-timestamp NIP-17 acceptance check failed. Its sanitized output contained no test private keys. `config/buzz-compatibility.env`, production deployment settings, upstream source trees, custom endpoints and private database access were not used or changed.

After the review fixes, the full protocol suite was rerun from the same harness against the exact relay SHA/tree above. Docker 29.7.2, Compose 5.5.0, and Node v22.22.2 were recorded; image config/build digest was `sha256:b8232baaa663c672cedc2f1ca379bd3fcc88d264e97d035338d9754e383c41eb`. Results remained **35 PASS, 1 FAIL, 3 UNVERIFIED** across 39 checks, plus relay identity restart PASS. The NIP-19 export used the locked `nostr-tools` encoder. The command exited 1 due to the randomized-timestamp NIP-17 failure; the new denial and channel-tag assertions passed. Desktop GUI evidence remains the separate unverified attempt below.

For the GUI attempt, the pinned desktop was built and launched under Xvfb and a private D-Bus session. A real GNOME Secret Service was unlocked and verified with a create/read/delete probe before launching Buzz; no mock store was used. The app rendered its initial screen, but synthetic mouse and keyboard input did not activate the controls. The `nsec` was not imported, and no desktop event was sent or received. This is **UNVERIFIED**, not a pass. Desktop public-key match, restart history, duplicate identity and community-enrollment checks are consequently also unverified.

### Phase 1b conformance matrix

| Check | Result | Recorded evidence |
|---|---|---|
| `nsec_gui_export` | PASS | `nostr-tools` produced the disposable NIP-19 `nsec` file for the GUI attempt; GUI import was not exercised. |
| `owner_nip42`, `member_nip42_alice`, `member_nip42_bob` | PASS | Relay challenge and authenticated `OK` for owner and both admitted users. |
| `nonmember_nip42_denied` | PASS | Outsider received challenge; relay returned `accepted=false`. |
| `nip43_add_alice`, `nip43_add_bob` | PASS | Both upstream kind-9000 admission commands accepted; subsequent NIP-42 passed. |
| `nip43_add_exact_replay` | UNVERIFIED | Exact replay was observed with `accepted=true`; harness deliberately makes no idempotency claim. |
| `nip43_add_stale_signed_command` | PASS | Signed 24-hour-old add command was rejected (`accepted=false`). |
| `create_public_channel`, `create_private_channel`, `private_channel_add_alice` | PASS | Upstream NIP-29 creation/add commands accepted. |
| `channel_discovery_39000`, `channel_discovery_39001`, `channel_discovery_39002` | PASS | Each requested channel event was returned followed by `EOSE`. |
| `private_channel_alice_message`, `private_channel_alice_history` | PASS | Alice's private kind-9 message was accepted and retrieved from history. |
| `private_channel_bob_denied` | PASS | Before admission, Bob received no private metadata or messages; history request was denied/empty. |
| `private_channel_add_bob`, `private_channel_bob_member_history` | PASS | After kind-9000 admission, Bob retrieved the existing private kind-9 event. |
| `kind9_publish`, `kind9_history_bob` | PASS | Alice's public kind-9 event was accepted and retrieved by Bob with the same event ID and Alice pubkey. |
| `nip29_dm_open_and_discovery` | PASS | Kind-41010 opened a hidden DM channel; Bob discovered kinds 39000/39001/39002. This is NIP-29 channel behavior, not encrypted NIP-17. |
| `nip17_randomized_timestamp_relay_compatibility` | **FAIL** | Valid outer and seal signatures; wrap timestamp was 5,672 seconds old (within the 172,800-second NIP-59 randomized range), but relay returned `invalid: event timestamp too far from server time`. |
| `nip17_recent_timestamp_delivery_and_decryption` | PASS | Current-time kind-1059 gift wrap and kind-13 seal signatures verified; Bob retrieved and NIP-44-decrypted the kind-14 rumor with expected sender, recipient and plaintext; unrelated outsider could not decrypt. |
| `nip17_recipient_only_visibility` | PASS | Alice's own recipient filter returned zero; Alice's attempt to query Bob's recipient filter was `CLOSED` with zero events. |
| `open_pre_revocation_subscriptions` | PASS | Bob opened three subscriptions (public kind 9, public kind 7, private kind 9); all reached `EOSE` after Bob had read private history. |
| `nip43_remove_bob`, `nip43_remove_exact_replay` | PASS | Removal accepted; exact removal replay returned `accepted=false`. |
| `revoked_existing_subscription` | PASS | After Alice published a public marker, relay closed Bob's socket within the 2,500 ms observation window; marker was not observed. There were zero per-subscription `CLOSE` frames; the single WebSocket close affected all three subscriptions. |
| `revoked_existing_publish`, `revoked_existing_new_req` | UNVERIFIED | Socket was already closed before either command could be sent (`attempted=false`). The relay's handling of post-removal publish/REQ on a still-open socket was not exercised. |
| `revoked_fresh_connection`, `revoked_reconnect_after_delay` | PASS | Fresh authentication was rejected immediately and again after 1,500 ms. Private content was not requested because authentication failed. |
| `tenant_b_member_nip42` | PASS | A user admitted to isolated relay B authenticated there. |
| `cross_tenant_alice_denied`, `cross_tenant_b_user_denied` | PASS | Each relay rejected a member identity from the other community at NIP-42. |
| `wrong_host_denied` | PASS | Test DNS explicitly resolved the wrong-host name to `127.0.0.1`; the proxy recorded matching SNI and Host and forwarded the HTTP upgrade to the stock relay. The relay returned HTTP 404 before NIP-42. |
| `wrong_sni_tls_rejected_before_relay` | PASS | Test DNS resolved to loopback; client received `ERR_TLS_CERT_ALTNAME_INVALID`. No HTTP upgrade reached the relay. |
| `relay_identity_restart` | PASS | Relay A's NIP-11 `self` remained `82252a7cfb9b0096d07098a2ab20608966a7422643ef40551a32fa94429fda6b` after restart. |

Totals: **35 PASS, 1 FAIL, 3 UNVERIFIED** across 39 protocol checks; the independent relay restart identity check also passed. The suite's exit status is nonzero by design when any required compatibility assertion fails.

Private-channel denial and the open-socket post-revocation REQ require an empty event list for both `CLOSED` and `EOSE`. Built-in assertions verify that denial fails when the accumulated event list is non-empty. Public and DM discovery checks require each matching kind 39000/39001/39002 event to carry a `d` tag equal to the requested channel ID; assertions also reject a mismatched channel tag.

Combined sanitized observations from the same run (proxy log plus client protocol results; random port retained as evidence):

```json
{"source":"proxy_log","id":9,"stage":"proxy_forwarded","sni":"buzz-wrong-host.poc.invalid","host":"buzz-wrong-host.poc.invalid:46075","request":"GET / HTTP/1.1"}
{"source":"protocol_result","test":"wrong_host_denied","layer":"relay_http_upgrade","status":404,"authenticated":false}
{"source":"protocol_result","test":"wrong_sni_tls_rejected_before_relay","code":"ERR_TLS_CERT_ALTNAME_INVALID","relay_http_request":false}
```

This establishes that stock Buzz rejected the tested alternate HTTP authority at upgrade. The local TLS proxy did not reject it. No proxy policy rejection was exercised. Two-relay NIP-42 tests establish community separation but do **not** test Elembra's external tenant-to-relay host mapping; that mapping must remain an explicit fail-closed architecture/security gate.

Additional sanitized wire-level proof from this run:

```json
{"test":"owner_nip42","challenge_received":true,"authenticated_ok":true}
{"test":"nonmember_nip42_denied","challenge_received":true,"authenticated_ok":false}
{"test":"kind9_publish_and_history","event_id":"fd2d5cc6bb3de92f107b7a22c5edda535dcfc922d0462054b1a6109757ffc286","author_pubkey":"d2dd3f05ed51623e493618f1186a956163a698e329f94bc5b4930b5aabe0d19d","same_id_on_history":true}
{"test":"nip17_randomized_timestamp","outer_signature_valid":true,"seal_signature_valid":true,"wrap_age_seconds":5672,"relay_accepted":false,"relay_reason":"event timestamp too far from server time"}
{"test":"nip43_remove_bob","command_accepted":true,"already_connected_socket":"closed","active_subscriptions":3,"subscription_close_frames":0,"marker_delivered":false}
{"test":"revoked_reconnect","immediate_auth_denied":true,"after_1500ms_auth_denied":true}
```

### DM and Phase 2 decision

The live desktop GUI DM path remains unverified. Source inspection at the pinned desktop commit shows its supported DM flow is NIP-29 private channels (kind 41010 open, kind 9 messages, kinds 39000/39001/39002 discovery); no NIP-17 kind-1059 client flow was found. The recent-timestamp NIP-44/NIP-17 wire test passes, but the relay rejects a valid randomized timestamp more than 15 minutes from server time. Do not silently change wrap timestamps to bypass that relay limit. **Keep encrypted DM disabled in the initial managed-identity release unless upstream-compatible behavior is demonstrated or a reviewed product scope explicitly excludes it.**

**Phase 2 readiness: NO-GO.** Mandatory desktop evidence is missing, randomized NIP-17 compatibility failed, Elembra host mapping has not been tested or security-accepted, and explicit human acceptance of ADR-0044 is absent. Approval of PR #440 is not ADR acceptance.

### Next Phase 2 prompt (gated)

> Do not implement managed identity yet. First close the Phase 1b gates: rerun the unmodified pinned desktop GUI under a responsive graphical session and unlocked OS keyring, proving import/public-key match, both message directions with exact event IDs, restart/history, and no duplicate identity/enrollment; resolve or explicitly remove NIP-17 DM from initial scope while keeping it disabled; specify and independently review fail-closed Elembra tenant-to-relay host mapping; and obtain explicit security acceptance of ADR-0044. Once all gates are recorded, on latest protected `kubedoio/rustshare` main implement the feature-gated `upstream_managed` identity mode per ADR-0044, `contracts/managed-chat-identity-v1.md`, and `contracts/upstream-buzz-relay-adapter-v1.md`. Preserve browser-key mode, legacy client-held identities, and M4.1b2 replacement guards. Add tenant-scoped encrypted key storage and typed Chat-only signing with NIP-42 possession checks, step-up-gated export, audit, migrations, and tenant/replay/KMS/outage/rotation/restore tests. Do not enable production cutover, replacement, or migration.

## Initial Phase 1 provenance and evidence (preserved historical run)

| Artifact | Commit | Git tree |
|---|---|---|
| RustShare baseline (protected `main`) | `36a058112324a9da10558bc9b9fae8f2bb565ef4` | verified by `git ls-remote origin refs/heads/main` |
| `block/buzz` relay | `eafd77e9f6e96052476bdd88be732c046fdb7ab3` | `04e3a40545bd3b23510ec32b4c8cccee2acef7c5` |
| `block/buzz-app` desktop | `662e91800af8204ec0e492522b8f23ef23b19baa` | `80275983372cc8dd840e732c0bb3610e97c7c283` |

Both upstream checkout trees were clean. The relay was built from its unchanged Dockerfile and source. For that historical protocol run, Buildx reported local image digest/config ID `sha256:323ca8c528b6496a7bfa104c8fa1c14685bdea5944258aef63e7fa86fbde1d16`. This is a local build, not a published GHCR digest; lookup of `ghcr.io/block/buzz:sha-eafd77e` returned not found. No upstream source was modified.

### Environment and execution (historical)

- Docker Engine 29.7.2, Compose 5.5.0, Node.js v22.22.2, Rust 1.98.1 (upstream Docker build).
- Two isolated Compose projects each ran the upstream relay, PostgreSQL 17, Redis 7, and Silo object storage, with independent volumes and relay keys. Auth and relay membership enforcement were enabled. Relay URLs used loopback WSS through a disposable local TLS proxy and CA.
- Harness command from a clean Phase 1 worktree:

  ```sh
  BUZZ_SOURCE_DIR=/tmp/block-buzz-phase0 POC_HOLD=0 scripts/upstream-buzz-poc/run.sh
  ```

- The harness checked exact relay SHA/tree and a clean source checkout, built the image, installed locked frontend crypto dependencies, ran the wire-level checks, restarted relay A and compared its NIP-11 `self` identity, then removed only its temporary Compose projects and key material.
- Evidence output contained public keys, event IDs, frame outcomes, and pass/fail results only. Test private keys and the temporary GUI `nsec` were not committed or uploaded. `config/buzz-compatibility.env` was not changed. No fork-only `/api/v1/relay/...` endpoint or private database access was used.

### Protocol results (historical)

| Check | Result | Observed behavior |
|---|---|---|
| `nsec_gui_export` | PASS | `nostr-tools` produced the disposable NIP-19 `nsec` file before the GUI attempt. This does not replace desktop import verification. |
| `owner_nip42` | PASS | Challenge received; authenticated `OK` accepted. |
| `nonmember_nip42_denied` | PASS | Challenge received; authentication explicitly rejected. |
| `nip43_add_alice`, `nip43_add_bob` | PASS | Both add commands accepted. |
| `nip43_add_exact_replay` | RECORDED | Exact replay returned `OK`, `accepted=true`; no idempotency assertion. |
| `nip43_add_stale_signed_command` | PASS | A separately signed 24-hour-old add targeting a nonmember returned `OK`, `accepted=false`. |
| `member_nip42_alice`, `member_nip42_bob` | PASS | Both authenticated after admission. |
| `create_public_channel`, `create_private_channel` | PASS | Both upstream NIP-29 commands accepted. |
| `private_channel_add_alice` | PASS | Owner added Alice to the private channel. |
| `private_channel_alice_message`, `private_channel_alice_history` | PASS | Alice published and retrieved private kind-9 event `7fb6f68ea6ebbdaaf23cc76aac81cb4a4db2078ec7337bd84d48857acbf56a69`. |
| `channel_discovery_39000`, `_39001`, `_39002` | PASS | Each returned the requested channel event and `EOSE`. |
| `kind9_publish`, `kind9_history_bob` | PASS | Bob retrieved Alice's kind-9 event `15581dec1b81497b1bf1582b211efa6b6c05409895456e26e017ccd3fd796e3d` with the same Alice pubkey. |
| `private_channel_bob_denied` | PASS | Bob received no private metadata; history request was `CLOSED`. |
| `nip17_giftwrap_routing` | PARTIAL PASS | Opaque kind-1059 routing to Bob was accepted and retrieved; payload was not NIP-44 encrypted. |
| `nip17_encrypted_dm_and_discovery` | UNVERIFIED | No NIP-44 payload or DM channel creation was exercised. |
| `open_pre_revocation_subscription` | PASS | Bob's subscription reached `EOSE` before removal. |
| `nip43_remove_bob` | PASS | Owner's removal command was accepted. |
| `nip43_remove_exact_replay` | PASS | Exact replay returned `OK`, `accepted=false`. |
| `revoked_existing_subscription` | PASS | After Alice published a marker, Bob's subscription received `CLOSE`; no marker was returned. |
| `revoked_existing_publish`, `revoked_existing_new_req` | PASS | Bob's existing socket was closed before either request could proceed. |
| `revoked_fresh_connection` | PASS | Fresh connection was challenged and explicitly rejected. |
| `tenant_b_member_nip42` | PASS | A user admitted to isolated relay B authenticated on B. |
| `cross_tenant_alice_denied`, `cross_tenant_b_user_denied` | PASS | A's Alice was denied on B; B's user was denied on A. |
| `wrong_host_denied` | UNVERIFIED | `localhost` ended in transport error without a NIP-42 challenge; this does not prove host authorization behavior. |
| `relay_identity_restart` | PASS | NIP-11 relay `self` stayed identical across relay A restart: `3f93d2b022bb97f0ae2707d81ad669eeb22e0d88a7f087c52bcd0de0e09c3e24`. |

Removal closed the already connected WebSocket in this run. This is a result for the tested upstream build and path, not a substitute for broader reconnect and timing coverage.

### Initial desktop GUI attempt (historical; superseded by Phase 1b attempt)

The first desktop attempt displayed “Your identity could not be accessed in secure storage. Unlock your credential store and retry without changing keys.” The Secret Service was then found locked. No import or messaging occurred during that first attempt. Phase 1b later verified and unlocked an actual GNOME Secret Service, but the headless GUI still did not accept input; see the current Phase 1b result above.

## Phase 1c execution (2026-10-31)

### Provenance and environment

| Artifact | Exact identifier | Result |
|---|---|---|
| Relay source | `block/buzz` commit `eafd77e9f6e96052476bdd88be732c046fdb7ab3`, tree `04e3a40545bd3b23510ec32b4c8cccee2acef7c5` | Required SHA/tree verified. Source checkout had no tracked or untracked changes. |
| Relay image | `sha256:0e0c4e868254088e1096b6550cef2f99b1789b0aec98ec3469557b90e9bf5c6b` (local image ID and Buildx build digest) | Final full protocol run, built from unmodified pinned source; local build digest, not published registry digest. |
| Relay image used during GUI attempt | `sha256:6f9876d8251d5b999400c7fb9a2843ccf8c4b37318030e0d8f341611f09c4d26` (local image ID and Buildx build digest) | Same source. Hold run was stopped after the desktop welcome-screen controls did not respond; disposable stacks were cleaned and this run is not counted as the full acceptance run. |
| Desktop source | `block/buzz-app` commit `662e91800af8204ec0e492522b8f23ef23b19baa`, tree `80275983372cc8dd840e732c0bb3610e97c7c283` | Exact audited checkout and separate build clone had the required commit/tree; tracked source unchanged. |
| Desktop binary | `/tmp/buzz-app-phase1c/target/release/Buzz`, SHA-256 `a78a37c202707a2c7fe4b00c0d4f7d599602ce2cf6f5536c33e837df678a89a3` | Unmodified app build. Build used Rust 1.98.1, Node v22.22.2 and pnpm 11.8.0; package requested Node >=24, so the unsupported-engine warning is retained. |
| Relay test runtime | Docker 29.7.2, Compose 5.5.0, Node v22.22.2; RustShare host Rust 1.97.1 | Two independent disposable stock Compose stacks; each used isolated PostgreSQL 17, Redis 7, object storage, relay key and closed community. Authentication and membership enforcement were enabled. |

Harness command: `BUZZ_SOURCE_DIR=/tmp/block-buzz-phase0 scripts/upstream-buzz-poc/run.sh`. The command rebuilt the relay image, ran the protocol suite, checked relay NIP-11 identity across restart, and removed its projects, volumes and temporary private keys. `config/buzz-compatibility.env`, production deployment configuration and either tracked upstream source tree were not modified.

### Current protocol conformance run

Result: **38 PASS, 1 FAIL, 3 UNVERIFIED (42 total)**, plus the independent relay restart identity check **PASS**. The JSON report and relay restart were emitted; the run exited nonzero only because the required randomized NIP-17 timestamp assertion fails.

| Test | Result | Observed result |
|---|---|---|
| NIP-42 owner/member/nonmember authentication | PASS | Owner and admitted Alice/Bob authenticated; nonmember received a challenge and was denied. |
| NIP-43 add, stale add, removal | PASS | Alice, Bob, and the test outsider were added through signed upstream kind-9030 events; stale add was denied; kind-9031 Bob removal was accepted. Exact add replay was recorded as `accepted=true` but remains unasserted. |
| NIP-29 discovery, kind-9 and history | PASS | Public/private channel creation, kinds 39000/39001/39002, kind-9 messaging and history passed with matching event IDs and authors. |
| Native NIP-29 DM discovery and ACL | PASS (protocol clients) | Kind-41010 opened a hidden channel. Bob discovered matching kinds 39000/39001/39002. A relay member outside the DM received zero discovery events (`EOSE`) and zero kind-9 history (`CLOSED`). |
| Native NIP-29 DM two-way messaging | PASS (protocol clients) | Alice event `50144ac833fe3ec2530f2f67048ec5944ef9029609be2d17f2f3a715c1c87953` was returned to Bob with Alice's author key. Bob event `0f86614b3781485e2f657583c7145467427aa2f58a450ef50f89374fae714849` was returned to Alice with Bob's author key. This is not desktop proof. |
| NIP-17 recent-timestamp NIP-44 delivery/decryption | PASS (out of proposed v1 scope) | Valid wrap and seal were accepted; Bob retrieved and decrypted the expected sender/recipient payload; outsider decryption failed. |
| NIP-17 randomized timestamp compatibility | **FAIL (out of proposed v1 scope)** | Valid signatures; wrap age 6,259 seconds; relay rejected it with `event timestamp too far from server time`. The historical failure is retained and excluded only from the proposed NIP-29-only required v1 tally. |
| Revocation with active subscriptions | PASS, limited guarantee | Bob had three subscriptions. After removal, relay closed the socket within the 2,500 ms observation window; no marker was visible and no per-subscription `CLOSE` frame was observed. Fresh auth and reconnect after 1,500 ms were denied. Publish and new REQ on the already-closed socket were **UNVERIFIED** because the socket closed before either could be sent. No claim is made about every live session or immediate revocation latency. |
| Two-relay tenant separation | PASS | Member of relay B authenticated on B; Alice from A was denied on B and B's user was denied on A. This proves the disposable relay protocol isolation, not Elembra's deployed mapping connector. |
| Wrong host and TLS/SNI | PASS (stock relay behavior) | The local TLS proxy forwarded the request to stock Buzz; wrong HTTP host received relay HTTP 404 before WebSocket authentication. Wrong SNI failed TLS certificate hostname verification before any relay HTTP request. No proxy authorization rejection was exercised. |
| Relay identity restart | PASS | NIP-11 `self` remained `09309c87ab899e31c6a758f5df3ec45c4287dd2daa84a19052e318e820c05f49` across restart. |

The three `UNVERIFIED` checks are exact kind-9030 add replay semantics and post-removal publish/REQ on a still-open socket (the socket closed before either request could be sent). The GUI hold was started with both communities and protocol outputs, but synthetic GUI clicks/tab input did not advance the Buzz welcome screen; it was stopped before importing the `nsec` or publishing any desktop event.

### Desktop build and GUI attempt

The audited, unmodified desktop binary built and launched in Xvfb/Openbox under a private D-Bus session. An actual GNOME Secret Service was unlocked, and its normal create/read/delete credential probe passed. Buzz rendered its welcome screen. Despite the active window and synthetic pointer/keyboard input, the “Use an existing key” control did not activate. No `nsec` was imported, no public-key match was observed, and no community join, message, restart/history or desktop DM assertion was executed. **All desktop interoperability assertions remain UNVERIFIED.** The GUI did not create an identity or enroll in a community. No test key, keychain database, profile or screenshot was retained. The secure manual process and collector are in `scripts/upstream-buzz-poc/desktop-acceptance.md` and `desktop-evidence.mjs`.

### Mapping policy tests and architecture decision

`upstream_relay_mapping.rs` contains 13 isolated policy tests covering exact tenant/workspace lookup, missing/ambiguous mapping, exact canonical host/URL, SSRF/internal and special-use IPv4/IPv6 addresses, unapproved/rebound DNS, TLS/hostname evidence, pinned peer and relay identity/community, stale generation and update during connection, audited compare-and-swap, and cross-tenant relay/community/host/key reuse. All 13 mapping tests pass as part of the `rustshare-resource-auth` library result (**76 passed, 0 failed**). These are policy-model tests only: no production adapter, DNS resolver, TLS connector, proxy, session pool or mapping store invokes this module. The policy model must not be presented as production enforcement.

The proposed v1 scope is NIP-29 channels plus native private-channel DMs after desktop E2E proof. NIP-17 encrypted DMs are excluded and must remain disabled. ADR-0044 now records the distinction, Option A custody consequences and controls, and reconciliation with ADR-0034/0035/0036. Its explicit architecture/security approval remains **PENDING**; no approver has accepted it.

**Phase 2A readiness: NO-GO.** The desktop interoperability gate, human acceptance of the v1 messaging scope and tenant mapping security contract, authorized ADR-0044 acceptance, independent review, and protected CI are not all satisfied. The policy-model implementation is not a substitute for accepting and integrating the transport contract.

### Validation results

- `cargo fmt --all --check` — PASS.
- `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS.
- `SQLX_OFFLINE=true cargo test --workspace --all-features --lib` — PASS across 1,348 tests; 0 failed; 71 ignored.
- `SQLX_OFFLINE=true cargo test -p rustshare-resource-auth --lib` — PASS, 76 passed; includes all 13 mapping policy tests.
- `SQLX_OFFLINE=true cargo clippy -p rustshare-resource-auth --all-targets --all-features -- -D warnings` — PASS.
- `node --check scripts/upstream-buzz-poc/protocol.mjs` and `node --check scripts/upstream-buzz-poc/desktop-evidence.mjs` — PASS.
- `node --test scripts/upstream-buzz-poc/desktop-evidence.test.mjs` — PASS, 6 tests; includes secret/URL leak rejection, event/channel/author correlation, duplicate identity/channel rejection, and global event ID uniqueness.
- `git diff --check` — PASS.
- Protected GitHub CI has not run yet; the PR must remain draft while required checks and human acceptances are pending.

### Exact Phase 2A implementation prompt (gated; do not start while NO-GO)

> On the latest protected `kubedoio/rustshare` main, implement only the isolated `upstream_managed` Chat identity vault described by the human-accepted ADR-0044, `docs/specs/elembra-buzz-upstream-managed-identity-v1.md`, `docs/contracts/managed-chat-identity-v1.md`, `docs/contracts/upstream-buzz-relay-adapter-v1.md`, and the accepted tenant-to-relay mapping contract. Before editing, verify the Phase 1c desktop interoperability evidence, required CI/reviews, explicit NIP-29-only v1 messaging scope, mapping security acceptance and authorized ADR-0044 acceptance are recorded; stop without implementation if any gate is missing. Preserve `legacy_client_held`, current fork-backed production behavior and all M4.1b2 identity replacement/recovery guards. Add tenant/principal/identity/generation-bound KMS envelope encryption with AEAD AAD, strict typed Chat signing intents, least-privilege signing authorization, step-up/no-store audited user key export, distinct owner/admin and user keys, audited key lifecycle/recovery, and tested backup/restore. Deny plaintext persistence/logging/tracing and arbitrary caller-supplied event signing. Keep NIP-17 disabled; implement only the accepted NIP-29 channel/private-channel DM subset. Add tenant-isolation, replay, binding, KMS outage/rotation, recovery/restore, export and audit tests. Do not change production Buzz pins, migrate existing users, enable cutover, or deploy. Run the protected Rust/frontend/SQLx CI required by changed packages and request independent security review.
