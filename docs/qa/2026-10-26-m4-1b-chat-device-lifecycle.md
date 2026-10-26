# M4.1b Chat device lifecycle audit and validation

Baseline: `d11722910edfda005881b1ac7ee257c49801530a` (merged PR #423).
Issues: [#215](https://github.com/kubedoio/rustshare/issues/215),
[#196](https://github.com/kubedoio/rustshare/issues/196),
[#323](https://github.com/kubedoio/rustshare/issues/323).

## Audit

Already present at baseline:

- Tenant/Principal-scoped Chat identity bindings and workspace/community
  mappings. NIP-42 challenge verification proves possession of the proposed
  Buzz key; private signing material remains client-side.
- Encrypted local key export/import, in-memory lock/unlock, account-bound
  envelope metadata and logout/account-switch cleanup.
- Explicit `rotation_of` backend semantics, admission revocation outbox,
  Principal offboarding and audited browser-session revocation.
- `GET/DELETE /api/v1/me/sessions` for browser sessions and
  `GET/DELETE /api/v1/user/devices` for Elembra device tokens.

M4.1b adds a Security warning and confirmation workflow to the existing
credential inventory. Browser sessions and device tokens are labeled by their
actual credential type; only the session API identifies the current browser.
These active-only APIs expose no revocation history. The UI does not add Chat
device records or claim that tokens prove Buzz-key custody. See the
[device lifecycle contract](../contracts/chat-device-lifecycle.md) for the
explicit threat model, supported revocation guarantees and M4.1b2 boundary.

## Validation on this revision

- `npm ci --ignore-scripts --no-audit --no-fund` — passed; npm reported
  existing peer-dependency/deprecation warnings.
- `npm run test` — 1,333 passed across 126 files. Existing test bootstrap/mock
  warnings and Excalidraw/RoughJS module-resolution stderr were emitted; no
  tests failed.
- `npm run check` — zero errors, 36 Svelte warnings.
- `npm run lint` — Prettier passed; ESLint completed with existing warnings.
- `npm run build` — passed.
- `cargo fmt --all --check` — passed.
- `SQLX_OFFLINE=true cargo test --workspace --all-features --lib` (debug
  symbols and incremental compilation disabled to fit local disk) — passed.
- `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features --
-D warnings` (debug symbols and incremental compilation disabled) — passed.
- `cargo deny --manifest-path Cargo.toml --all-features check` — passed;
  existing duplicate-dependency and ignored-advisory warnings remain.
- The new PostgreSQL inventory/revocation integration test compiled with
  `cargo test -p rustshare-server --test
user_disable_credential_revocation_test --no-run`. It was not executed: the
  available local database container uses database `rustshare`, not the
  required disposable `rustshare_test`, and the disposable-service opt-ins are
  unset. Do not target it.
- Live Chat/Buzz conformance, protected PR CI and independent
  architecture/security review remain required before approval. No merge,
  deployment or Gate A execution is part of this work.

## Review limits

M4.1b2 (#424) must provide and test stronger authenticated pairing, per-device Buzz
signing authority, deliberate replacement confirmation, ordered revocation and
community membership reconciliation. The current async bridge does not provide
completion confirmation for safe end-user rotation UX. The current browser
session and device-token revocation guarantees do not invalidate copied Buzz
keys.
