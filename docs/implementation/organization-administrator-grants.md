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

The API is installation-operator-only and has no organization-admin UI or
authorization integration. A named security owner must review recovery and
last-administrator behavior before organization-admin authorization is
connected to these records or the role is exposed to organization users.

PostgreSQL regressions are in
`backend/tests/organization_administrator_grants_test.rs`. They are ignored by
default and require a specifically named disposable `rustshare_test*` database
and a loopback S3-compatible test bucket.
