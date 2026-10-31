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

Both upstream checkout trees were clean. The relay was built from its unchanged Dockerfile and source. For the completed protocol run, Buildx reported local image digest/config ID `sha256:91d0032555fac809cd00200d550e1cbc462f8ca8a0b3065be63f338cedbfdcab`. This is a local build, not a published GHCR digest; lookup of `ghcr.io/block/buzz:sha-eafd77e` returned not found. No upstream source was modified.

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
| Owner NIP-42 | PASS | Challenge received; authenticated `OK` accepted. |
| Nonmember NIP-42 | PASS | Challenge received; authentication explicitly rejected. |
| NIP-43 9030 add Alice and Bob | PASS | Both accepted. Exact replay of Alice's signed add returned accepted `OK`; it did not create a second identity. A 24-hour-old add returned `OK` with `accepted=false`. |
| Member NIP-42 | PASS | Alice and Bob authenticated after admission. |
| Public/private channel creation and private Alice membership | PASS | Upstream NIP-29 commands accepted. |
| 39000 / 39001 / 39002 discovery | PASS | Each returned the requested channel event and `EOSE`. |
| Kind-9 send and history | PASS | Alice's event ID `d3210b88b00b597de5654be909e61d7ec1a8ac4bc7a3a825d7e0e36e1ff34df4` was retrieved by Bob with Alice's same public key. |
| Private-channel visibility | PASS | Alice retrieved private-channel kind-9 history; Bob received no metadata and the private history request was `CLOSED`. |
| NIP-17 kind-1059 routing | PARTIAL PASS | Opaque kind-1059 routing to Bob was accepted and retrieved. Payload was intentionally not NIP-44 encrypted. Encrypted DM and DM-channel discovery remain UNVERIFIED. |
| NIP-43 9031 removal | PASS | Bob's existing socket was closed. Publish and subsequent request on that socket could not proceed. A fresh connection received a challenge but was explicitly rejected. Exact replay of the removal was explicitly rejected (`OK`, `accepted=false`). |
| Two isolated communities | PASS | A user admitted to B authenticated on B; Alice from A was denied on B, and B's user was denied on A. |
| Wrong-host access | UNVERIFIED | The `localhost` alternate authority ended in a transport error without a NIP-42 challenge. This does not prove host-level authorization behavior. |
| Stable relay identity | PASS | NIP-11 relay `self` stayed identical across relay A restart: `a56b5970c36ecb07beab7f223e709e57c173879c6bfc504ad518b63bafc8bfa5`. |

Removal behavior is stronger than reconnect-only revocation in this run: the already connected WebSocket closed immediately after the accepted 9031 removal. This is a result for the tested upstream build and path, not a substitute for broader reconnect, subscription, and timing coverage.

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
