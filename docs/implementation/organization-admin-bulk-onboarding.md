# Organization-admin bulk onboarding (Phase 4 P3, #331)

`POST /api/v1/organization-admin/users/bulk` accepts 1–100 structured rows and
an explicit `dry_run` flag. The request body is capped at 1 MiB before JSON
deserialization. Each row contains `email`, `display_name`, and optional
existing `group_ids`. The caller must have a live organization-admin grant;
tenant scope comes from the authenticated principal.

Dry-run returns row numbers and outcomes without writing invitations,
assignments, or audit events. Apply revalidates each row under a tenant and
normalized-identifier advisory lock. Valid rows commit independently, so
invalid rows do not prevent other valid rows from applying. Each invitation,
pending group assignment, and `organization_admin.user_invited` audit event
commit in one PostgreSQL transaction. Concurrent retries reuse the existing
invitation; they do not create another token or assignment. A recent delivery
attempt is reported as `delivery_pending`; a failed or stale attempt retries
the same token.

Possible row outcomes include `ready`, `invalid_email`,
`invalid_display_name`, `duplicate_in_request`, `duplicate_group`,
`too_many_groups`, `already_exists`, `identifier_unavailable`,
`group_unavailable`, `already_pending`, `pending_invite_mismatch`,
`delivery_pending`, `apply_failed`, `apply_pending`, `invited`,
`delivery_retry`, `delivery_failed`, and `delivery_retried`.
Foreign users and groups use the
same generic outcomes as unavailable identities/groups. The API never returns
email addresses, invitation tokens, or invite links. Email delivery happens
after the database commit, so `delivery_failed` means the invite and audit are
committed and a later apply can retry delivery with the same one-use link.
SMTP cannot guarantee exactly-once delivery; a retry can send the same link
again if the provider accepted a message but the response was lost.
Delivery work is limited to 10 concurrent rows per server process, and the
bulk endpoint has a separate per-IP limit of 6 requests per minute by default
(`RUSTSHARE_RATE_LIMIT_BULK_INVITE_PER_MINUTE` can override it). Both limits
are process-local; deployments with multiple replicas multiply the concurrency
and per-IP request limits.
Database errors before a row's invitation transaction commits return
`apply_failed`; that row rolls back while other rows continue. If the database
connection fails while commit is being acknowledged, the outcome is
`apply_pending` because the transaction may have committed. Retrying safely
reconciles either state. A failure recording delivery status returns
`delivery_pending` because the provider may already have accepted the message;
retries reuse the same token.

An existing invitation created by the legacy invite route cannot be retrofitted
with bulk group assignments. A bulk row requesting groups for that address
returns `pending_invite_mismatch` and leaves the invitation untouched.

Group assignments are held against the pending invitation and become ordinary
tenant-checked group memberships in the same transaction that accepts the
invite. Acceptance writes an attributed audit record with the assigned group
IDs in that same transaction. If a referenced group was deleted or moved,
acceptance revokes the invitation, records the unavailable IDs, and returns a
conflict; the administrator can then submit a corrected batch.

If the mail provider accepts a message but the subsequent delivery-status or
audit transaction fails, the invitation remains in `sending`; after five
minutes a retry may deliver the same one-use link again. No retry creates a
second invitation or membership.

## Invitation lifecycle

Organization administrators can list tenant invitations at
`GET /api/v1/organization-admin/invitations?page=1&per_page=20`. Pages are
capped at 100 rows. Each row exposes recipient/sender metadata and one of
`pending`, `sent`, `failed`, `expired`, `accepted`, or `cancelled`; invitation
tokens and links are never included. Legacy invitations without bulk delivery
metadata are listed as sent.

`POST /api/v1/organization-admin/invitations/{id}/resend` requires an
`Idempotency-Key` header (1–128 ASCII letters, digits, `-`, `_`, `.`, or `:`).
The server stores only its SHA-256 digest. A repeated key replays the prior
delivery result and does not send another message. A new key starts a new
attempt after the previous attempt is complete. Stuck sending attempts become
retryable after five minutes; because SMTP cannot promise exactly-once delivery,
a recovery retry may send a link again if the provider accepted the earlier
message and the server crashed before it recorded the result. Resend reuses the
existing token and expiry and does not return either to the caller. SMTP
failures set delivery state to failed and are returned without provider error
details.

`POST /api/v1/organization-admin/invitations/{id}/cancel` is idempotent. It
shares the invitee-email advisory lock and invitation row lock used by
acceptance, marks the token revoked, deletes pending group assignments, and
records the change in the same transaction. A cancelled link cannot be
accepted. If acceptance commits first, cancellation returns a conflict; if
cancellation commits first, acceptance returns gone. A delivery already in
flight may still reach the mailbox, but its link is revoked and cannot create
an account.

Invitation listing, resend attempt state, delivery state, cancellation, and
their mandatory tenant-attributed audit events do not include token material.
If the initial resend audit insert fails, the attempt and delivery-state
changes roll back before SMTP is contacted. Final delivery state and its audit
event commit together. Resend/cancel PostgreSQL/HTTP regressions live in
`backend/tests/organization_admin_invitation_lifecycle_test.rs` and require a
disposable database and object store.

## #331 acceptance matrix and deferred work

- Complete: validate a bounded structured import before apply; report invalid,
  duplicate, existing, and unavailable rows; invite valid users in one bounded
  operation; assign initial existing groups; list, resend, and cancel pending
  invitations; safely offboard users; preserve content/provenance; and audit
  mutations transactionally.
- Deferred: CSV-file upload, bulk group assignment/removal for existing users,
  independent workspace identities/membership, and administration UI. ADR-0040
  keeps organizations tenant-scoped and explicitly defers first-class
  workspaces; this lifecycle slice introduces no workspace or role system.
