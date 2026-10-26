# Chat identity onboarding and recovery

M4.1a / #215; authority: [ADR-0034](../adr/0034-elembra-chat-buzz-boundary.md).

## Authority and API

Elembra authenticates the tenant/Principal and authorizes binding/admission.
Buzz owns signed communication identity. An OIDC session does not prove key
possession. The browser uses the existing endpoints, without schema changes:

1. `POST /applications/chat/identity-binding/challenge` with workspace ID and
   locally derived Buzz public key.
2. Sign the returned relay/nonce locally as NIP-42 kind 22242; send only
   `challenge_id` and signed `event` to `/identity-binding/verify`.
3. Request `/applications/chat/admission` for that workspace.

Server verification remains tenant/Principal/key/relay scoped, five-minute,
one-time and atomic. Rotation requires explicit `rotation_of`; normal onboarding
never requests it. Local recovery cannot reactivate a revoked binding or grant
admission. No backup, plaintext signing key, or key passphrase is sent to these
endpoints or included in server logs by this flow.

## Local encrypted backup contract

JSON is limited to 4,096 characters. Supported v1 envelopes contain 16-byte hex
salt, 12-byte hex IV, 48-byte hex ciphertext (32-byte secret plus GCM tag), and
optional integer PBKDF2 iterations in [100,000, 600,000]. Omitted iterations
mean legacy 100,000. The decrypted scalar must be a valid secp256k1 secret;
derive its public key locally and require exact equality with any claimed
`pubkey` and, during recovery, the current binding.

New exports additionally contain `aad: 1`, `owner` (Principal ID), and `tenant`
when available. AES-GCM additional data is the UTF-8 encoding of
`JSON.stringify([1, owner, tenant ?? null])`. Relabeling or stripping these
fields fails authentication. Owner and tenant must match the active account.
New encryption uses PBKDF2-HMAC-SHA256 with 600,000 iterations, random salt and
IV, and AES-256-GCM. The extension is backward incompatible with older clients;
use updated clients to import new backups. Existing encrypted v1 backups remain
readable. Plaintext `secret_key` backups are rejected.

Legacy backups lack authenticated account provenance. For a bound account,
its existing public key is mandatory independent evidence. Before binding,
the user must explicitly confirm ownership of a legacy backup; derived-key
verification and the existing server binding authorization still apply. This
confirmation is consent, not historical account ownership proof. A legacy
backup cannot reveal which account previously held it. Do not infer authority
from its optional public-key label.

Import decrypts and validates before committing. Failure preserves the existing
vault byte-for-byte. Scope generation guards reject logout/account switching,
including switching away and back. Concurrent vault changes abort the write.
An unbound import cannot replace a different local identity. Logout hides the
vault immediately, before the logout HTTP request completes.

## UX and accessibility

- Unbound account: offer “Create new Chat identity” and “Import existing Chat
  identity.” If a key already exists, offer “Use stored Chat identity”; retrying
  binding reuses it instead of creating a replacement.
- Import validates before any challenge request. Owner-less legacy import has
  an explicit confirmation checkbox. Mismatched known-account backups fail.
- Explain that the browser holds the private key, encrypted by the passphrase.
  Export the encrypted backup and keep the passphrase separately. A copied key
  signs as the same identity on each device.
- Recovery requires the original backup and its passphrase. Elembra account
  recovery cannot restore either. If every copy is lost, the original identity
  is unrecoverable; ask an administrator for explicit rotation. Existing signed
  history remains under the old identity.
- Unlock offers “Recover from backup” even when a local envelope exists.
  Inputs have accessible names; failures use alerts and successful backup copy
  uses a status region. Clipboard failure is reported before claiming success.
- Passphrase and pasted backup fields clear on completion/failure. JavaScript
  strings cannot promise physical memory zeroization.

## Shared client throttling

Unlock, encrypted recovery, and pre-bind import share one guard per tenant and
Principal in the running browser session. Only one verification runs at a time.
After three failed attempts, delay retries by 1, 2, 4, 8, 16, 32, then at most
60 seconds. The failure count saturates; blocked attempts do not extend the
delay. Feedback states the remaining seconds. Successful authentication resets
the counter. Account switching isolates counters; logout ends the local session.
Changing UI entry points cannot reset it.

This is a usability safeguard against repeated interactive guesses. It cannot
prevent offline brute-force attacks against a stolen backup, deliberate browser
restart, developer-tool changes, or verification in another tab. Strong
passphrases and the KDF remain necessary.

## M4.1b requirements

Device inventory and audited enrollment/removal; device-to-device pairing;
lost-device session handling; compromise/rotation and membership migration UX;
and a protocol distinguishing per-device credentials from a shared signing key.
Removing one device record cannot revoke one copy of a shared key. Do not claim
otherwise. Managed signing and server key escrow remain out of scope.
