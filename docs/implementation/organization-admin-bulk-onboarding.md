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
`delivery_failed`, and `delivery_retried`.
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

Workspaces, offboarding, reactivation, resend/cancel controls, and independent
workspace membership remain outside this slice. PostgreSQL/HTTP
regressions live in `backend/tests/organization_admin_bulk_invites_test.rs` and
are guarded for a disposable local database and object store.
