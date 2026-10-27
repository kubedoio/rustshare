# Chat device lifecycle and lost-device contract

Status: M4.1b bounded delivery; pairing and controlled key replacement are
split to [M4.1b2 / #424](https://github.com/kubedoio/rustshare/issues/424).
Authority: [ADR-0034](../adr/0034-elembra-chat-buzz-boundary.md)
and [Chat identity recovery](chat-identity-recovery.md).

## Separate credentials and authority

| Item                    | What it proves                                             | Revocation effect                                                                        |
| ----------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Elembra browser session | The caller holds one authenticated Elembra session         | Deletes that session; subsequent use of its session credential is rejected.              |
| Elembra device token    | The caller holds an approved account device token          | Marks that token revoked; subsequent token authentication is rejected.                   |
| Buzz signing key        | The caller can produce Buzz signatures for that public key | Only Buzz binding/admission and community authorization changes affect future admission. |

Browser sessions and device tokens are independent credential types. A device
token label or activity timestamp is not proof that its client has a Buzz key.
The token API returns only active tokens. The browser-session API also returns
expired session records; the UI labels their expiry status at the last refresh.
Neither inventory provides revocation history. Browser session inventory
identifies the current session and reports last-seen time. Device token
inventory reports its name and last use when available, but has no
current-browser indicator.

The user Settings → Security view reuses these APIs. Revocation is confirmed,
tenant/user scoped by the existing handlers, and reports its credential type.
Revoking another browser session or a device token stops that credential from
authenticating to Elembra. The current browser uses Sign Out, which clears its
in-memory Chat session. Neither operation erases a copied/encrypted key backup
or remotely locks a different browser's in-memory key.

## Threat model and response

| Threat                                | What the attacker may have                                                 | Response and limit                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Lost device with locked encrypted key | Device storage and the encrypted local envelope                            | Revoke its browser session/device token. The envelope remains available for offline guessing; the passphrase and PBKDF2 cost matter. Revocation cannot erase the envelope.                                                                                                                                                                                          |
| Lost device with unlocked signing key | A copy of the secret in process memory and possibly the encrypted envelope | Revoke platform credentials to stop their Elembra API access. Treat the shared Buzz key as compromised: a copied key can still sign. Coordinate old Buzz admission revocation with an administrator. Identity replacement is currently unsupported.                                                                                                                                                              |
| Stolen encrypted backup               | The passphrase-encrypted envelope                                          | The server has no copy and cannot invalidate it. A strong passphrase limits offline guessing, but client retry throttling does not protect a stolen file. If the passphrase may also be exposed, treat the key as compromised.                                                                                                                                      |
| Compromised Buzz private key          | Signing authority, including copies on other devices                       | Browser/device-token revocation is insufficient. Revoke the old Elembra binding and coordinate Buzz community admission revocation. Replacement is currently unsupported until Buzz membership can be authoritatively verified. Historical signed events remain attributed to the old key.                                                                                                    |
| Compromised SSO/browser session       | Account-level Elembra authority and access to authenticated APIs           | Revoke sessions and device tokens, disable/reset the account as appropriate, and investigate security events. This alone does not reveal a locked Buzz key. The backend rejects identity replacement until M4.1b2 provides durable operation state and Buzz-confirmed membership reconciliation. |
| Complete loss of every key copy       | No signing material for the old Buzz identity                              | The old identity cannot be recovered. An authorized administrator can coordinate a new identity and revoke old admission; old signed history cannot be reassigned or re-signed as the new identity.                                                                                                                                                                 |

## Existing behavior and bounded delivery

- `GET /api/v1/me/sessions` lists the authenticated user's stored browser
  sessions. `DELETE /api/v1/me/sessions/{id}` only deletes a session found for
  that user; the current session must use Sign Out. The session credential is
  rejected after deletion.
- `GET /api/v1/user/devices` lists active device tokens for the authenticated
  user. `DELETE /api/v1/user/devices/{id}` updates only a non-revoked token
  belonging to that user; other users' tokens are not addressable. Token
  authentication checks revocation. These are Elembra account credentials,
  not Chat key registrations.
- Key creation, backup encryption/import and in-memory lock/unlock are
  client-side. Plaintext signing keys and passphrases do not go to the server.
  Account switching and logout clear the local in-memory Chat session.
- Activation rechecks the active Principal inside its transaction and excludes
  public keys with any previous binding. Revoked keys cannot be restored or
  reassigned to another Principal or tenant. Principal and public-key locks
  serialize competing activations and explicit Principal revocation. The
  storage boundary requires the new binding's tenant, Principal and key to
  match the verified challenge; `rotation_of` is currently rejected.
- The legacy `rotation_of` storage path was unsafe: it activated the proposed
  key in Elembra as soon as the challenge was consumed, while Buzz revocation
  remained an asynchronous outbox delivery. The verify endpoint now rejects
  replacement until the durable, relay-confirmed protocol in the
  [identity replacement contract](chat-identity-replacement.md) is implemented.
- No additional Chat device registration is created. No server-side key
  escrow or per-device Buzz signing authority is introduced.

## M4.1b2 contract: authenticated pairing and controlled replacement

The current encrypted backup is a portable copy of one shared identity. It is
second-device recovery, not device enrollment, and it cannot support
individual-device key revocation. Any stronger per-device design must satisfy
all of the following before implementation is reported complete:

1. Generate a distinct keypair on each device; private material remains on the
   client and is never uploaded or escrowed.
2. Bind enrollment proof to tenant, Principal, workspace/community, device
   public key, one-time nonce, expiry and the authenticated initiating account.
   Proof must come from an already authorized key or an explicit administrative
   recovery path. A device token by itself is not enrollment proof.
3. Define how Buzz authorizes per-device keys and how removal revokes them.
   Elembra cannot claim individual revocation while Buzz treats all copies of
   one public key as the same signer.
4. For replacement, require deliberate confirmation and proof of possession of
   the replacement key; bind `rotation_of` to the same tenant and Principal.
   Revoke old Buzz admission and reconcile community membership before the
   replacement can send. Surface bridge failure as pending/failed, never as
   completed rotation.
5. Preserve historical signatures and prevent silent reassignment of another
   Principal's key. Make replay, concurrent replacement, retry and bridge
   delivery idempotent and fail closed.

Account-device enrollment, approval and revocation auditing is explicitly
required for M4.1b2. It is not implemented by M4.1b's platform-credential
inventory/revocation work; this defers, rather than drops, the #215 requirement.

M4.1b2 must preserve the PostgreSQL history, ownership and concurrent-rotation
guarantees and add live Buzz ordering/membership and stolen/invalid pairing
tests before exposing pairing or replacement UX.
