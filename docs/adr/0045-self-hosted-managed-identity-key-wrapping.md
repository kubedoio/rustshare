# ADR-0045: Self-Hosted Managed Identity DEK Wrapping

- **Status:** Architecture selected for an inert provider foundation; independent custody acceptance and real-user use remain gated
- **Date:** 2026-11-01
- **Related:** ADR-0044, PR #444, PRs #445/#446, RustFS image pinned in `docker-compose.yml`

## Decision

Keep the existing version-1 AES-256-GCM identity envelope and its metadata in PostgreSQL. Implement the existing `DekKeyManagement` abstraction with an Elembra-owned, feature-gated local key-wrapping provider. The provider reads versioned 32-byte master keys from an operator-provisioned, read-only secret mount and retains the active and recovery versions only for the provider lifetime (best-effort zeroization; no hardware isolation is implied). It authenticates each wrapped DEK with AES-256-GCM and binds the full `KeyEncryptionContext` as AAD. Only the backend service identity receives the secret mount. No key bytes are loaded from environment variables, PostgreSQL, RustFS or normal backups.

This selection records the project owner's self-hosted architecture direction on 2026-11-01. It is not independent security acceptance, authorization for real-user custody, feature enablement or release approval. The provider has no production caller and remains disabled for real-user identities until Gate I and named independent custody acceptance.

## RustFS evaluation at the deployed pin

The production Compose service uses `rustfs/rustfs@sha256:60f4f2f41ce95216f8cac676e69f9d90c0bfec458a3bc7fd7fb9b7c2452ac57a`. Its cached OCI labels identify RustFS `1.0.0-beta.10`, source revision `f67e8a6cdcb3e08dfa3c3c390412a9815837d0b2`. The repository Compose configuration does not enable or configure RustFS KMS.

At that exact source revision, `crates/kms/src/backends/local.rs` stores RustFS master-key records on local disk and encrypts their key material with AES-GCM using a key derived from `RUSTFS_KMS_LOCAL_MASTER_KEY` and a persisted salt via Argon2id. The `rustfs/src/admin/handlers/kms_management.rs` and `kms_keys.rs` routes expose `generate-data-key`, which generates a new random key and returns plaintext plus ciphertext under the `KmsAction::GenerateDataKeyAction` permission. The pinned HTTP routes do not expose arbitrary-plaintext `Encrypt` plus corresponding `Decrypt`/unwrap for an Elembra-generated DEK; RustFS's `DecryptRequest` is an internal Rust API type in `crates/kms/src/backends/mod.rs`. Its S3 SSE-KMS path protects RustFS objects and does not grant Elembra an arbitrary DEK wrapping contract. Therefore the deployed RustFS local KMS is not a compatible `DekKeyManagement` provider. These source files are pinned at [`f67e8a6`](https://github.com/rustfs/rustfs/tree/f67e8a6cdcb3e08dfa3c3c390412a9815837d0b2).

The current official RustFS encryption documentation describes Local, Vault KV2 and Vault Transit as server-side encryption backends, not as a cross-service arbitrary-DEK API: [RustFS Data Encryption](https://docs.rustfs.com/en/security-compliance/encryption), [RustFS KMS environment variables](https://github.com/rustfs/docs.rustfs.com/blob/main/content/en/reference/environment-variables.md). The checked-out deployed artifact remains the authority for this evaluation; newer RustFS capabilities do not change the current pin.

## Storage choice

| Choice | Decision | Reason |
|---|---|---|
| A. Existing encrypted envelopes and wrapped DEKs in PostgreSQL | **Selected** | Reuses the existing tenant-RLS store and migration. The envelope is small; only the master key remains outside the database. Backup/restore has one ciphertext database plus a separately controlled key bundle. |
| B. PostgreSQL identity metadata plus ciphertext in a RustFS system bucket | Rejected for this phase | Adds cross-system atomicity, S3 IAM, object naming/versioning, orphan cleanup, deletion, and joint backup/restore coordination. It does not solve the absence of a RustFS arbitrary-DEK wrap/unwrap API or reduce master-key exposure. |

RustFS remains ordinary object storage for its existing product data. No managed identity envelopes or master keys are written to a RustFS bucket by this decision.

## Provider and key lifecycle

- Use stable key ID plus explicit immutable key-version identifiers. Mount each version as a raw 32-byte file; never permit software defaults or generate a production root key at startup.
- Read only regular, non-symlink files owned by the backend service UID, with no group/other access and no write permission. Fail provider initialization on a missing, malformed, unreadable, insecurely permissioned or unknown active key version. Secret mount is read-only and attached only to the dedicated backend service identity.
- Store `key_id`, `key_version`, nonce and authenticated wrapped-DEK ciphertext in the existing `WrappedDek` fields. Wrap each identity's independently random DEK with a fresh nonce and AAD derived unambiguously from tenant, principal, identity, generation, public key, and envelope version. Do not change the version-1 private-key envelope ciphertext or AAD.
- Rotation mounts a new version while retaining previous versions, restarts/reloads the provider with the new active version, and rewraps each identity DEK through the existing compare-and-swap storage path. Keep old keys available while any identity or retained backup references them. Do not delete an old version merely because the online rows were rewrapped.
- Fail closed on missing key versions, malformed mount contents, authentication failure, provider outage, or unknown key ID/version. Errors and logs must not include key bytes, wrapped plaintext DEKs, private keys, ciphertext payloads, or file contents.
- Do not put master-key files in PostgreSQL, the RustFS system bucket, the repository, ordinary `.env` files, application images, or routine database/object-store backups. An operator-managed, separately access-controlled offline escrow/recovery copy is required. Recovery requires both the database ciphertext and every referenced key version. Test restoration in an isolated environment before relying on it.

## Threat model and limits

This software provider separates database-only compromise from key access: a database-only attacker obtains identity envelopes and wrapped DEKs, but not the separately mounted master key. RustFS-only compromise also does not gain the key mount. The backend process and any principal able to read or execute inside that process can access plaintext master key material and may decrypt any identity envelope available to its database role. Host root, the operator controlling both mounts, memory disclosure, or compromise of the backend service identity can bypass this separation. This is **not** equivalent to an HSM or separate cryptographic process, and zeroization is best-effort rather than protection against memory inspection.

Named independent security acceptance must review the process-compromise boundary, mount and service identity, operator recovery and rotation controls, database role/RLS, audit, and backup handling before real-user key generation, signing, or export. No ordinary code review grants that authorization.

## Verification required before any production use

The provider foundation must test context substitution, ciphertext tamper, unknown key/version, missing or insecure mount, process restart, key-version rotation/rewrap, restoration from separately escrowed key versions plus database backup, and provider outage. Operational restore evidence must use disposable identities and record no secret material. The feature stays default-off; there is no server startup wiring, identity generation caller, signing route, export endpoint, or production Chat routing in this decision.
