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
The browser API includes expired sessions; the token API returns active tokens.
Neither exposes revocation history. The UI does not add Chat
device records or claim that tokens prove Buzz-key custody. See the
[device lifecycle contract](../contracts/chat-device-lifecycle.md) for the
explicit threat model, supported revocation guarantees and M4.1b2 boundary.

## Review and validation on the review head

The first full Vitest run during cold Rust compilation had one intermittent
Mail inline-image failure. The focused Mail test then passed, and the complete
suite passed on an idle frontend run. A review of the activation transaction
found that a previously revoked Buzz key could be rebound or reassigned after
revocation. Activation now rejects any public key with prior binding history
across tenants and serializes activation with Principal revocation/disablement and
other attempts for the same key. The storage boundary requires binding scope,
key and rotation fields to match the verified challenge. The PostgreSQL
regression proves revoked-key history, ownership, wrong-key binding, replay,
competing rotation and deterministic cross-tenant same-key activation. It also
exercises same-tenant same-key assignment and explicit revocation concurrently
without forced overlap, and checks disabled-Principal rejection sequentially.
Comprehensive review also closed an admission lifecycle race: admission now
holds the same Principal lock as activation and revocation. Deterministic
PostgreSQL barriers prove admission cannot commit after explicit revocation,
disablement or rotation. A second unrotated key is rejected without consuming
its challenge. The OpenAPI contract distinguishes account-device and Vault Sync
revocation routes and operation IDs. Failed sign-out retains platform
authentication and real-time state while clearing Chat state and reporting that
the server credential may remain active.

- `npm ci --ignore-scripts --no-audit --no-fund` — passed; npm reported
  existing peer-dependency/deprecation warnings.
- Focused Settings/Chat notice tests — 53 passed; focused Mail test — 40 passed.
- Final Settings, Chat binding/no-private-key request-shape and safety-link
  tests — 28 passed across 3 files. The binding test asserts backend requests
  contain only the public key and signed NIP-42 event, never a signing-key
  field; settings tests cover refresh/revocation concurrency and same-route
  navigation from the safety notice to device tokens.
- `npm run test` — 1,352 passed across 128 files.
- `npm run check` — zero errors, 36 existing Svelte warnings.
- `npm run lint` — Prettier passed; ESLint completed with zero errors and 155
  existing warnings.
- `npm run build` — passed with existing Svelte/Vite bundler warnings.
- `cargo fmt --all --check` and `git diff --check` — passed.
- PostgreSQL against a newly created loopback-only disposable database and
  RustFS bucket: `chat_identity_lifecycle_test` — 2 passed; credential
  inventory/revocation — 1 passed. The runner dropped the database and bucket
  after completion. No application database or bucket was used.
- Test targets for both PostgreSQL regressions compile with
  `cargo test -p rustshare-server --all-features --test
chat_identity_lifecycle_test --no-run` and the matching
  `user_disable_credential_revocation_test --no-run` command.
- `SQLX_OFFLINE=true cargo test --workspace --all-features --lib` — passed.
- `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features --
-D warnings` — passed.
- `cargo deny --manifest-path Cargo.toml --all-features check` — passed;
  repository-configured ignored advisories not present in the current graph
  produce existing warnings.
- `cargo sqlx prepare --workspace --check` — passed with the existing
  potentially-unused-query warning. No migration or SQLx metadata changed.
- The existing PR CI run passed on the earlier head; protected CI must be
  checked again after the current review fixes are committed and pushed.
- Independent agent architecture/security and code-quality reviews were
  performed and their findings resolved. Explicit human approval and protected
  CI remain required before merge. No merge, deployment or Gate A execution is
  part of this work.

## Review limits

M4.1b2 (#424) must provide and test stronger authenticated pairing, per-device
Buzz signing authority, deliberate replacement confirmation, ordered
revocation and community membership reconciliation. The current async bridge
does not provide completion confirmation for safe end-user rotation UX. The
current browser-session and device-token revocation guarantees do not
invalidate copied Buzz keys.
