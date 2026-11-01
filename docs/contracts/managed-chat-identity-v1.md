# CONTRACT: Managed Chat Identity and User Credential Export v1

**Status:** Target contract accepted for planning; interfaces are **not** currently implemented. Normative words MUST/SHOULD specify intended implementation. See ADR-0044.

## Phase boundaries

Phase 2A may add an isolated vault record and KMS-backed encryption/decryption
primitives with a test-only key provider. The feature is disabled by default;
there is no runtime route to provision a real Principal, sign a user event,
export a key, enroll in Buzz, or activate `upstream_managed`. The API and
provisioning sections below describe later Phase 2B/R target behavior and are
not Phase 2A implementation authorization. Existing browser-held and
fork-backed records remain unchanged.

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

### POST `/api/v1/applications/chat/messages` (illustrative)

Trusted request fields: mapped channel or DM identifier, message content, optional Files ResourceRef/metadata, idempotency key. No caller-provided pubkey, `nsec`, arbitrary event kind, private event tags or signing-key selection. Resolve identity from session, validate channel/tenant and attachment authorization, sign canonical upstream Nostr kind for this message type, send to relay with bounded retries, return `accepted_by_relay` distinct from `observed`/`delivered`. Native Buzz event wire formats MUST be determined by conformance against pinned upstream.

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
excluded and must remain disabled. No signing behavior is implemented in
Phase 2A; typed signing is a separate Gate I/Phase 2B change after review.

## Must-pass security tests

Distinct tenant/principal/key, replay, repeat provisioning, duplicate worker, failure before/after encrypted-key insert, KMS outage/rotation, DB-only dump, account disabled during send/export, stale login cookie/account switch, CSRF failure, absent step-up, export replay and secret logging sentinel. Verify correct public key after standard Buzz 1.0 import and bidirectional send. Cover old `legacy_client_held` identities without forced migration.
