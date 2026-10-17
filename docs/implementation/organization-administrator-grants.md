# Organization administrator grants (P1)

This slice adds tenant-scoped grant records and operator-managed grant,
revoke, bootstrap, and recovery endpoints. The existing installation operator
(`users.is_admin`) is the only principal accepted by these routes. The new
organization-administrator grant is inert for authorization: it does not grant
content access, installation control, or access to another tenant.

Each grant is a separate history row. Revocation records its operator and time;
re-granting inserts a new row. Database triggers reject row deletion and edits
other than the one-time revocation transition. Grant creation verifies the
target is enabled and belongs to the tenant in both the handler and database.

Grant and revoke mutations write their scoped `admin_actions` event in the same
transaction. Bootstrap and recovery are classified from the tenant's grant
history and active enabled administrators. The last enabled administrator
cannot be revoked, disabled, or deleted until an enabled replacement is
granted. Disabling or deleting an administrator revokes their grant in the
same transaction. Re-enabling the user does not restore the grant.
SCIM deactivate, delete, and reactivation paths also revoke a prior grant in
their user mutation transaction; because SCIM has no human principal, its audit
record identifies the actor kind as `scim` without fabricating a user ID.

The operator grant/revoke/bootstrap/recovery API remains installation-operator
only. The bounded `/api/v1/organization-admin` API is available to enabled
users with a live grant, checked against PostgreSQL on each request. It offers
tenant-filtered user list/detail, group list/create/update/delete, member
listing/add/remove, and tenant-scoped administrative audit. User pages contain
at most 100 rows and audit pages contain at most 100 events. Tenant scope
always comes from the authenticated account, including the nil default tenant.
Foreign IDs return not-found. Audit includes group and organization-admin
actions only, excluding installation-level actions, and applies the shared
allowlist detail projection.
Group names retain the existing global uniqueness rule; a name already used
by any tenant returns 409 on create or rename.

Organization admins cannot mutate user lifecycle or installation-admin state,
manage administrator grants, access operator configuration, or read private
Files, Notes, Calendar, or other application data. Group membership mutations
retain the existing tenant equality checks and post-commit index ACL refresh.
The route contract is exported in `docs/contracts/rustshare-api-openapi.json`.
Human security review is required before merge; no broader institutional
readiness or independent workspace support is implied.

PostgreSQL regressions are in
`backend/tests/organization_administrator_grants_test.rs`. They are ignored by
default and require a specifically named disposable `rustshare_test*` database
and a loopback S3-compatible test bucket.
