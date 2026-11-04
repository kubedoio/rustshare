# CONTRACT: Managed Chat Identity and User Credential Export v1

**Status:** Target identity contract accepted for planning; interfaces are **not** currently implemented. Normative words MUST/SHOULD specify intended implementation. See ADR-0044 and ADR-0046 (Option A selected by the project owner; independent security acceptance and upstream fix pending).

## Phase boundaries

Phase 2A may add an isolated vault record and KMS-backed encryption/decryption
primitives with a test-only key provider. The feature is disabled by default;
there is no runtime route to provision a real Principal, sign a user event,
export a key, enroll in Buzz, or activate `upstream_managed`. The API and
provisioning sections below describe later Phase 2B/R target behavior and are
not Phase 2A implementation authorization. Existing browser-held and
fork-backed records remain unchanged.

Phase 2B.2 managed provisioning merged through PR #456. Its six-test Cilium
qualification passed at source commit `f349605a13f3a1c076e47e246660fe9c1c370210`,
with all 48/48 sanitized evidence manifest entries verified. PR #457 later
merged the feature-gated, internal, one-shot managed NIP-29 history reader at
merge commit `a6ec3d98c1d397e59596585f1badab716165691b`; it has no production
caller. Project-owner `senolcolak` accepted only that reader's limited reliance
on Buzz's relay authorization and deferral of its Cilium test. This is not
independent security acceptance and does not authorize production provisioning,
user-message signing, key export, or activation. The reader's Cilium test remains
UNVERIFIED and is tracked by issue #458 before managed Chat activation. Issue
#442 remains the separate stock-desktop interoperability gate.

## Identity invariant

```
Elembra authenticated (tenant_id, principal_id)
    -> one current managed Buzz signing identity (generation, pubkey, encrypted key reference)
    -> exactly one configured Buzz community (host, immutable instance identity/pin)
```

One public key MUST NOT be active for two unrelated Principals or tenants. The same human may have a separate managed key in each tenant if acting in multiple tenant contexts. An exported credential is a copy of the private Nostr key; it is not a login password, a bearer API token, or a WebAuthn passkey.

## Server storage — proposed schema, not executable SQL

`chat_managed_identities`:

- `tenant_id, principal_id, identity_id, generation, pubkey` (unique per identity; immutable public-key attribution)
- `custody_mode` = `managed` / `legacy_client_held` (no silent mode switch)
- `status` = `pending_provisioning | ready | suspended | revoked | failed`
- `community_mapping_id`, `relay_host`, `creation_version`, `created_at`, `updated_at`, `revoked_at`
- `key_envelope_ref`, `wrapped_dek`, `kek_version`, `nonce`, `ciphertext`, `aad_version` (only for managed keys)
- `provision_attempt_id`, `last_error_code`, `provisioned_at`, `last_membership_verified_at`

`chat_managed_identity_operations`: unique scoped logical requests for `provision | suspend | remove_membership | credential_export | legacy_import | replace`; stable idempotency identifier, actor, worker lease/retry, state, bounded reason, timestamps and audit event reference. Avoid duplicating existing operation/outbox tables if safe reuse is demonstrated.

Any migration MUST be forward-only and tenant-aware; test legacy zero-tenant/nil semantics per ADR-0043. Associate key envelopes cryptographically to `[version, tenant, principal, identity_id, generation, pubkey]` as AEAD additional authenticated data. This is independent of Buzz schema and may live in Elembra-owned PostgreSQL with a KMS-backed wrapping key.

## Provisioning contract

**Future Gate I trigger:** an explicitly enabled tenant and active Principal; it is not automatic in Phase 2A. **Idempotency:** repeat activation or worker restart returns the same identity/key. **Success:** a fresh authenticated NIP-42 session to the mapped relay can access native member-only operations. A NIP-43 command ACK alone MUST NOT change status to `ready`. **Failure:** `pending/failed`, no signing or credential export until valid provisioning and policy permit.

Required transitions:

```
missing -> pending_provisioning -> ready -> suspended -> revoked
                    |             |
                    +-> failed <-+
```

`failed` is retryable by authorized operation with bounded error codes; `revoked` is terminal for this key. Restoring the Elembra Principal does not silently reinstate a revoked Buzz key; replacement requires an explicitly reviewed new identity flow. Existing identity replacement protocol remains disabled in the upstream-only profile until it has its own stock-relay acceptance contract.

## Proposed Elembra user API (names/shapes MUST be reconciled with current routing/OpenAPI)

### GET `/api/v1/applications/chat/managed-identity`

Authorized self only; no privileged administrative representation in the self endpoint. Example response:

```json
{
  "custody_mode": "managed",
  "status": "ready",
  "pubkey": "<64-hex-pubkey>",
  "npub": "npub1...",
  "relay_url": "wss://chat.example.org",
  "export_allowed": true,
  "externally_exported": false
}
```

This response MUST NOT contain secret, key ciphertext, signing capability tokens, KMS material, or operator credentials. `externally_exported` only tracks known successful exports; it does not guarantee no other copies exist.

### POST `/api/v1/applications/chat/managed-identity/exports`

Initiate explicit export, with `format: "nsec"`; optionally future `ncryptsec` after compatibility proof. Requires active session, fresh step-up, CSRF protection, explicit user intent, single-use challenge and rate limit. User must be entitled to the active managed identity and must not be disabled. Do not export a tenant admin or service key through this route. Replay and cross-tenant requests deny without disclosing existence. Avoid a generic impersonation/decrypt API.

Successful one-time response after completed challenge (illustrative):

```json
{
  "format": "nsec",
  "private_key": "nsec1...",
  "relay_url": "wss://chat.example.org"
}
```

Over TLS only; `Cache-Control: no-store`, `Pragma: no-cache`, `Referrer-Policy: no-referrer`, restrictive CSP. Never emit the key in redirect URLs, asynchronous job payloads, audit text, analytics, traces, error strings, exception snapshots or test fixtures. Frontend must show masked value with a deliberate reveal/copy and clear it on navigation, account switch, timeout and logout. Key export produces an audit event (actor, tenant, identity ID, destination format, timestamp; NOT the key). Confirmation must explicitly state exports can be copied and cannot be remotely erased.

### POST `/api/v1/applications/chat/messages` (illustrative only; not authorized)

This example is a future interface sketch, not an implemented or approved route. Trusted request fields would be a mapped channel or DM identifier, message content, optional authorized Files ResourceRef/metadata and an idempotency key. No caller-provided pubkey, `nsec`, arbitrary event kind, private event tags or signing-key selection is permitted. The project owner selected Option A in [PR #459](https://github.com/kubedoio/rustshare/pull/459#issuecomment-6640798406): a typed internal signer performs Elembra-side authorization while unmodified Buzz remains final ingest authority. Independent security acceptance remains pending. No route or signing API is authorized, and publisher implementation is blocked by the privately reported upstream membership-cache/role-transition defect tracked publicly in #460 without reproduction details. Native Buzz event wire formats MUST be determined by conformance against a fixed, official upstream pin.

## Managed key custody and signing contract

- Strong CSPRNG generation, curve and public-key validation, KEK provided by a configured KMS/HSM/secret manager, per-identity AEAD envelope, KEK rotation, backup+restore. No reusable root KEK embedded in the repository, normal `.env`, public Compose, or database dump.
- Caller is a current authenticated, non-revoked user, tenant context and mapped identity; the server verifies exact requested Chat intent and current Chat application grant. Never allow user-supplied arbitrary `kind`, signer, role or relay URL to be signed; if new kinds are needed, add typed per-kind policies/tests.
- Key access is a narrow service boundary. Ordinary background workers, Memory search, system administrators and frontend cannot obtain decrypted keys just because they can access the DB. Define service authorization and audit; administrator recovery/export on behalf of users is prohibited in v1.
- All sensitive operations fail closed if KMS, tenant configuration, user authorization, Buzz member admission or relay resolution fails. No client-signing fallback that silently recreates a second identity.
- Use separate service/relay-owner/admin keys; cryptographic revocation is impossible after an `nsec` export, so membership removal and documented relay session semantics are the only access limits.

## Accepted initial messaging scope

The target initial release supports NIP-29 channel messages and native
NIP-29 private-channel DMs. Private-channel DMs rely on Buzz relay ACLs and
are not end-to-end encrypted. NIP-17/NIP-44 encrypted gift-wrap DMs are
excluded and must remain disabled. No kind-9 publishing behavior is
implemented. The project owner selected Option A and accepted its signed
artifact portability, plaintext visibility, replay and revocation tradeoffs
in PR #459. Typed signing still requires an independent security ACCEPT, the
upstream membership-cache fix tracked with publisher conformance in issue
[#460](https://github.com/kubedoio/rustshare/issues/460), and the
implementation/live acceptance gates in ADR-0046. The Phase 2B.3 history
reader is not a publisher or a publish authorization proof.

## NIP-29 publishing authorization status

At pinned stock Buzz commit `eafd77e9f6e96052476bdd88be732c046fdb7ab3`, kind-9
write authorization is performed after an already signed event reaches
`ingest_event_inner`. Kind-39000/39002 relay-signed snapshots and successful
NIP-42 private-channel reads are advisory/read authorization only; they are not
fresh, operation-bound write permits. Buzz's ingest-time membership cache,
role, channel state and persistence are not transactionally bound to an
Elembra pre-sign check. See ADR-0046 for the source audit, Option A/Option B
analysis, replay consequences and durable operation design.

Option A is the owner-selected target, not an implementation authorization.
The typed precheck is conservative but advisory; Buzz remains final authority
and can reject a valid signed event. Managed publishing stays blocked until
the privately reported upstream membership-cache/role-transition defect is
fixed at an official
unmodified pin and independent security acceptance is recorded. Issue #458
remains the separate managed-history Cilium gate; #460 tracks publisher
conformance. Any later implementation must preserve exact signed-event
recovery, encrypted durable
storage with operation-bound AEAD context, scoped idempotency, authorization
revalidation before send/retry, body-free audit, separate
relay-accepted/observed/delivered states and the tests in ADR-0046. Option B's
atomic pre-sign permit is unavailable in the pinned source and remains a
deferred alternative.

## Must-pass security tests

Distinct tenant/principal/key, replay, repeat provisioning, duplicate worker, failure before/after encrypted-key insert, KMS outage/rotation, DB-only dump, account disabled during send/export, stale login cookie/account switch, CSRF failure, absent step-up, export replay and secret logging sentinel. Verify correct public key after standard Buzz 1.0 import and bidirectional send. Cover old `legacy_client_held` identities without forced migration.
