# Organization-scoped audit foundation (#332)

This foundation extends the existing `admin_actions` history with the owning
tenant, an immutable actor principal snapshot, outcome, and optional validated
`X-Request-ID`. User lifecycle and group/membership handlers insert audit rows
in the same PostgreSQL transaction as the change. Share storage does the same
for creation and revocation. Audit detail returned by the existing read route
continues to use an action-specific allowlist; descriptions, credentials,
request headers, link tokens, and resource content are excluded.

The installation operator remains the only principal authorized by the admin
audit route and retains the existing installation-wide view. `tenant_id` can
bound that view to one organization. This change does not grant an organization
administrator role or turn a tenant filter into authorization. Rows predating
the migration keep a NULL tenant when their organization cannot be established;
the nil UUID remains a concrete, queryable tenant value. Audit rows cannot be
updated by application SQL; the existing retention worker may delete expired
rows according to `RETENTION_AUDIT_LOG_DAYS` (365 days by default).

## Gate A coverage in this change

- User create, update of administrative attributes, disable, re-enable, and
  delete.
- Group create, rename/update, delete, and membership add/remove.
- Public share create and revoke.
- Actor, tenant, action, target, timestamp, outcome, and request ID when the
  request supplies a valid `X-Request-ID`.
- Bounded pages (maximum 100), deterministic ordering, tenant filter,
  cross-tenant filter regression, nil-tenant regression, transaction rollback,
  immutable-update regression, and secret/content redaction regression.

## Remaining for full #332 completion

- Invitation cancel/resend/activation events and organization-administrator
  grant, revoke, bootstrap, and recovery records once those flows exist.
- Workspace and role administration, permission changes, public/capability link
  lifecycle coverage beyond the current public share service, and high-value
  Calendar administration events.
- Organization-admin authorization and UI/API exposure. The schema and action
  contract support future grant events, but this PR exposes no such role.
- Actor/action/resource filters, single-event lookup, bounded JSON/CSV export,
  and retention behavior evidence beyond the existing retention worker/default.
- Candidate deployment evidence, operator rehearsal, named security-owner
  review, and Gate A operational sign-off.
