# M4.1a Chat identity audit and validation

Repository: kubedoio/rustshare. Issues: #215, #196, #323.
Protected-main baseline: `315ad4127c1fcd05e02e4416651e26437d7766c8`.

## Audit and scope

Audited `BindingPanel.svelte`, `ChatIdentityUnlock.svelte`,
`ChatApplicationView.svelte`, `chat/keys.ts`, `chat/session.ts`, auth scoping,
existing key/session/UI tests, identity challenge/verify handlers,
resource-auth challenge tests, ADR-0034, issue #215, and the maturity roadmap.

Already implemented: client-held PBKDF2/AES-GCM keys, local encrypted export,
bound-device recovery UI, in-memory signing sessions, NIP-42 challenge/verify,
atomic binding/admission, explicit rotation, Principal offboarding and relay
revocation infrastructure. These backend mechanisms were reused unchanged.

Gaps addressed: no pre-bind import choice; import trusted a claimed public
key; recovery committed before checking the bound key and deleted the vault on
mismatch; unbounded KDF inputs; no shared retry/concurrency guard; asynchronous
import/save could outlive account or binding changes; unlock failures retained
passphrase input; clipboard failure could incorrectly report backup success.

The implementation remains within the frontend identity and auth boundary;
there are no migrations, SQLx metadata changes, dependencies, server signing,
new identity APIs, or changes to Buzz authority.

## Implemented flows

- Create locally, encrypt, then prove the same public key through the existing
  challenge/verify/admission flow. Retries reuse a stored key.
- Import an encrypted backup before requesting a challenge, derive its public
  key locally, and bind that key. Existing local identities are not silently
  replaced. Legacy owner-less imports require explicit ownership confirmation.
- Recover a bound identity only after validated format, bounded KDF, successful
  decryption, valid curve scalar and exact claimed/derived/bound key checks.
- New exports authenticate account/workspace metadata using AES-GCM additional
  data; relabeling and stripping it fail authentication.
- Shared tenant/Principal session guard serializes verification across all UI
  entry points and delays retries exponentially after three failures.
- Logout/account switching, rotation and view teardown cancel pending recovery
  writes; observed binding removal/rotation locks the old signing session.

Full contract, backup compatibility, UX copy and M4.1b requirements are in
[chat-identity-recovery.md](../contracts/chat-identity-recovery.md).

## Security tests and evidence

Frontend tests use real WebCrypto and curve operations for custody, onboarding
and recovery; API endpoints are mocked to inspect exact request bodies.
The invalid decrypted scalar test stubs only WebCrypto decryption to exercise
the post-decryption trust boundary. Tests cover fresh creation, pre-bind import,
shared retry delays and reset/isolation, concurrent unlock/import, malformed and
oversized backups, corrupt ciphertext, unsafe KDF counts, invalid scalars,
claimed/derived/bound key mismatch, foreign Principal/tenant, failed-import
vault preservation, account switching including ABA, legacy compatibility,
authenticated metadata tampering, binding rotation/removal, and pending-import
rotation/unmount cancellation. API assertions prove that plaintext keys,
passphrases and encrypted backups are absent from the onboarding requests.

Existing Rust `chat_identity` tests cover NIP-42 proof, replay, expiry, wrong
key/relay/tenant, invalid signature, disabled Principal, revoked admission and
bridge-service signing. The server also checks the Principal in challenge
scope verification; no server implementation changed.

Local validation commands:

```sh
cd frontend
npm install --ignore-scripts --no-audit --no-fund
npm run test
npm run check
npm run lint
npm run build
cd ..
cargo fmt --all --check
SQLX_OFFLINE=true cargo check -p rustshare-server
SQLX_OFFLINE=true cargo test -p rustshare-resource-auth --lib chat_identity
SQLX_OFFLINE=true cargo clippy -p rustshare-resource-auth --all-targets --all-features -- -D warnings
```

Rust server check, five identity tests, identity Clippy and workspace formatting
passed. All 1,319 Vitest tests in 124 files passed. Frontend check reports zero errors and 36 pre-existing warnings outside
the changed identity UI. Lint and production build passed. The exact delivery
SHA and protected CI status are recorded on the PR.

## Independent review and limits

A separate reviewer audited security, authority boundaries and static
accessibility. Findings fixed: unauthenticated ownership metadata, legacy
pre-bind compatibility, premature clipboard success, missing-workspace cleanup,
blank-backup passphrase retention, tenant isolation, and binding-change/unmount
recovery races. Final re-review reported no outstanding findings. Live browser
and assistive-technology testing was not performed.

Client throttling cannot prevent offline brute force or deliberate browser
restart/new-tab bypass. Strong passphrases and the KDF remain essential. Legacy
backups cannot attest historical account provenance; explicit consent and the
existing binding API establish the new association. New authenticated exports
require an updated client. JavaScript cannot guarantee memory zeroization.

No live server-log capture or PostgreSQL/relay acceptance environment was
started locally. The existing PR CI `Chat Product Acceptance` job owns the
disposable bundled deployment and live relay/read/offboarding tests. CI status
must be checked independently; local unit tests do not substitute for it.
Full Rust workspace test/Clippy gates were not rerun locally because Rust code
is unchanged; protected CI remains required.

## Delivery gate and M4.1b

One PR: **M4.1a: safe Chat identity onboarding and recovery**. Human security
review and green protected CI are required before merge. No automatic merge,
deployment, or Gate A execution is authorized by this change.

M4.1b retains device inventory, audited enrollment/removal and pairing,
lost-device sessions, compromise/rotation and membership migration UX, and
per-device credential/admission design. A shared signing key cannot support
cryptographic revocation of just one copied device key. Principal offboarding
and explicit binding/admission revocation already exist and remain in force.
