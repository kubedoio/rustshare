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
| Lost device with unlocked signing key | A copy of the secret in process memory and possibly the encrypted envelope | Revoke platform credentials to stop their Elembra API access. Treat the shared Buzz key as compromised: a copied key can still sign. Arrange old Buzz admission revocation and controlled replacement.                                                                                                                                                              |
| Stolen encrypted backup               | The passphrase-encrypted envelope                                          | The server has no copy and cannot invalidate it. A strong passphrase limits offline guessing, but client retry throttling does not protect a stolen file. If the passphrase may also be exposed, treat the key as compromised.                                                                                                                                      |
| Compromised Buzz private key          | Signing authority, including copies on other devices                       | Browser/device-token revocation is insufficient. Revoke the old Elembra binding and Buzz community admission, then prove possession of a replacement key and reconcile membership before enabling it. Historical signed events remain attributed to the old key.                                                                                                    |
| Compromised SSO/browser session       | Account-level Elembra authority and access to authenticated APIs           | Revoke sessions and device tokens, disable/reset the account as appropriate, and investigate security events. This alone does not reveal a locked Buzz key. Existing identity-rotation APIs are account-authorized; until M4.1b2 provides deliberate confirmation and ordered Buzz completion, do not use them as an end-user recovery flow during an SSO incident. |
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
  serialize competing activations, rotation and explicit Principal revocation.
  The storage boundary also requires the new binding's tenant, Principal, key
  and `rotation_of` to match the verified challenge.
- Existing `rotation_of` verification proves that the proposed replacement key
  signed the NIP-42 challenge. In one Elembra transaction it revokes the old
  binding/admission record, queues Buzz revocation, and activates the new
  binding. Buzz processes the outbox asynchronously. There is no user-facing
  rotation confirmation or completed Buzz membership migration/acknowledgement
  path in this milestone, so this backend flow is not presented as a safe
  lost-device action.
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

M4.1b2 must preserve the PostgreSQL history, ownership and concurrent-rotation
guarantees and add live Buzz ordering/membership and stolen/invalid pairing
tests before exposing pairing or replacement UX.
