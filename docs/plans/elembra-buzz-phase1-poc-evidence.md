# Phase 1: upstream Buzz interoperability evidence

Date: 2026-10-31

## Decision

**Phase 1 is partial; Phase 2 managed identity custody is not ready to begin.** The real relay confirmed the basic NIP-42, NIP-43, NIP-29, kind-9, private-channel, and tenant-membership behavior below. The audited Buzz desktop client was built and launched, but its secure-storage prerequisite could not be unlocked in this environment, so `nsec` import and desktop bidirectional messaging remain unverified. Encrypted NIP-17 DM discovery and a reachable wrong-host test also remain unverified. ADR-0044 requires independent security acceptance before custody work in any case.

## Provenance

| Artifact | Commit | Git tree |
|---|---|---|
| RustShare baseline (protected `main`) | `36a058112324a9da10558bc9b9fae8f2bb565ef4` | verified by `git ls-remote origin refs/heads/main` |
| `block/buzz` relay | `eafd77e9f6e96052476bdd88be732c046fdb7ab3` | `04e3a40545bd3b23510ec32b4c8cccee2acef7c5` |
| `block/buzz-app` desktop | `662e91800af8204ec0e492522b8f23ef23b19baa` | `80275983372cc8dd840e732c0bb3610e97c7c283` |

Both upstream checkout trees were clean. The relay was built from its unchanged Dockerfile and source. For the latest protocol run, Buildx reported local image digest/config ID `sha256:323ca8c528b6496a7bfa104c8fa1c14685bdea5944258aef63e7fa86fbde1d16`. This is a local build, not a published GHCR digest; lookup of `ghcr.io/block/buzz:sha-eafd77e` returned not found. No upstream source was modified.

## Environment and execution

- Docker Engine 29.7.2, Compose 5.5.0, Node.js v22.22.2, Rust 1.98.1 (upstream Docker build).
- Two isolated Compose projects each ran the upstream relay, PostgreSQL 17, Redis 7, and Silo object storage, with independent volumes and relay keys. Auth and relay membership enforcement were enabled. Relay URLs used loopback WSS through a disposable local TLS proxy and CA.
- Harness command from a clean Phase 1 worktree:

  ```sh
  BUZZ_SOURCE_DIR=/tmp/block-buzz-phase0 POC_HOLD=0 scripts/upstream-buzz-poc/run.sh
  ```

- The harness checked exact relay SHA/tree and a clean source checkout, built the image, installed locked frontend crypto dependencies, ran the wire-level checks, restarted relay A and compared its NIP-11 `self` identity, then removed only its temporary Compose projects and key material.
- Evidence output contained public keys, event IDs, frame outcomes, and pass/fail results only. Test private keys and the temporary GUI `nsec` were not committed or uploaded. `config/buzz-compatibility.env` was not changed. No fork-only `/api/v1/relay/...` endpoint or private database access was used.

## Protocol results

| Check | Result | Observed behavior |
|---|---|---|
| `nsec_encoding_checksum` | PASS | The generated disposable `nsec` passed the encoder's length/checksum self-check before writing. This does not replace desktop import verification. |
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

## Desktop GUI result

The unmodified audited desktop source built and its Tauri window launched under Xvfb. The test did **not** pass: the client displayed “Your identity could not be accessed in secure storage. Unlock your credential store and retry without changing keys.” The isolated Secret Service collection remained locked in this container. No key was imported, and no desktop send/receive event ID comparison was made. Therefore desktop `nsec` import and bidirectional same-identity messaging are **UNVERIFIED**, not passed.

## Gaps and Phase 2 gate

Do not start managed identity vault or signing implementation from this POC. First obtain independent human security acceptance for ADR-0044 and close these interoperability gaps:

1. Repeat the GUI test in an environment with a working isolated Secret Service: import the harness user's `nsec`, confirm the displayed pubkey, then demonstrate both send directions against the same channel and compare observed event IDs.
2. Exercise a valid NIP-44 encrypted NIP-17 exchange and the documented DM discovery subset.
3. Make the wrong-host probe reach the relay and record whether the stock relay accepts the alternate authority; separately prove Elembra's tenant-to-host mapping fails closed.
4. Repeat removal against multiple active subscriptions and record timing/reconnect behavior.
5. Preserve this upstream-only evidence in a draft PR and obtain independent review before Phase 2.

### Next Phase 2 implementation prompt (gated)

> Begin only after independent security acceptance of ADR-0044 and closure or explicit acceptance of the Phase 1 interoperability gaps. On latest protected `kubedoio/rustshare` main, implement custodial Chat identity as a new feature-gated `upstream_managed` mode per ADR-0044 and `contracts/managed-chat-identity-v1.md`. Keep browser-key mode functional for existing tenants. Add KMS-wrapped per-identity encrypted Nostr key storage with exact tenant/Principal/generation AAD, idempotent provisioning, and a state machine. Add a scoped signing service for typed Chat intents only, NIP-42 possession checks, audit, and OIDC app grants. Generate an identity automatically on active-user Chat provisioning; retries must not regenerate it. Add Settings > Chat connection status, relay URL, pubkey, and masked `nsec` export after fresh step-up, CSRF, one-time grant, and audit. Never return plaintext in normal APIs, logs, outbox, or workflow payloads. Add migrations, SQLx metadata, tenant/replay/KMS/outage/rotation/backup-restore tests, and security review. Preserve M4.1b2 rotation guards and legacy client-held identities; do not enable production cutover, replacement, or migration.
