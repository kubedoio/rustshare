# RustShare Pilot Readiness Contract

This contract defines the minimum evidence required before a specific
RustShare revision can be offered to FWS/Erasmus pilot users. It extends the
existing pilot workflow and smoke scripts; it is not a second product
acceptance system.

## Evidence identity

Every pilot acceptance run must retain:

- source Git SHA and clean/dirty source status;
- image version and revision labels;
- workflow/run identity and deployment identity;
- configuration identity (a hash or equivalent that never contains secret
  values);
- UTC start/end timestamps;
- result and failing phase, if any;
- Compose status, application logs and health responses on failure.

A green result is invalid if setup, readiness, a mandatory test, evidence
collection or cleanup fails. Optional product areas must be explicitly marked
optional; they may not be silently skipped.

## Mandatory gates

### Product acceptance

The existing pilot smoke must pass against the deployed candidate and cover:

1. administrator login and logout;
2. a second permitted user login;
3. the deployed RustShare UI index page is reachable;
4. Files root access, folder creation, upload and byte-for-byte download;
5. Notes create, read and save;
6. explicit note rename independent of the first Markdown H1;
7. a permitted internal share and a protected-resource negative check;
8. audit activity visibility and permission-aware search where enabled;
9. user lifecycle: create a least-privileged account, reset its password as an
   administrator, change it as the user, disable/offboard it, and prove reset
   and disable revoke active sessions while a disabled account cannot log in.

Initial and reset passwords must be handed to users over an approved secure
channel separate from repository/evidence artifacts. RustShare does not enforce
a first-login password change, so the operator must instruct the user to
change the initial/reset password immediately.

Chat and AI/Ask are conditional product areas: they are tested only when the
deployment enables them and the run declares the requirement. Password login
is the baseline pilot authentication mechanism; OIDC is a separate security
acceptance item when FWS enables it.

### Operational acceptance

- The documented Compose deployment starts from a clean environment without
  undocumented manual fixes.
- `/health` proves process liveness and `/health/ready` proves required
  database, object storage, event delivery and auth/session readiness.
- Application startup, migrations, shutdown and dependency errors are visible
  in logs without secrets.
- Invalid password authentication is rejected with a controlled 401/403 and
  remains distinguishable from application or dependency failure.
- A restart of the application preserves representative pilot data.
- Invalid configuration fails closed and does not produce false readiness.
- Stopping PostgreSQL or RustFS makes readiness fail and restoring the service
  makes readiness recover.
- A non-developer operator can create, reset, disable and offboard users using
  the documented controls. Keep two independently controlled administrator
  accounts available before cohort start; loss of every administrator
  credential is not covered by the application-level recovery flow.

### Security acceptance

- No default or weak secrets are used in the pilot deployment.
- Session cookie, TLS termination and public URL settings match the deployment
  topology.
- Passwords, tokens, cookies, private keys and database credentials do not
  appear in logs or evidence artifacts.
- Protected Files/Notes and tenant boundaries reject unauthorized access.
- Demo accounts are intentionally configured for the pilot or disabled; their
  presence is recorded in the evidence.
- Debug-only flags and public diagnostic surfaces are reviewed before the
  pilot. `/metrics` is protected when exposed outside the private network.

### Recovery acceptance

- The backup contains PostgreSQL state, RustFS/object state, deployment
  configuration and a manifest/checksum set.
- Deployment secrets are restored from the external secret store; they are not
  regenerated during recovery. Browser-only Chat keys are handled by the
  operator/user procedure and are not claimed to be in the server backup.
- A restore drill replaces relevant application state, starts the stack, runs
  the verification journey and proves representative pilot data is present.
- A candidate upgrade runs applicable forward migrations and verifies the
  journey afterward. Migration failures must be non-zero and visible.
- Downgrade is not a supported acceptance claim. Rollback means restoring the
  pre-upgrade database/object backup and returning to the previously verified
  image/configuration.

## Readiness decision

The final report may conclude only:

- `READY` — every mandatory gate has passed with evidence tied to the exact
  revision and target environment;
- `READY WITH ACCEPTED LIMITATIONS` — every mandatory gate passes and the
  pilot owner has explicitly accepted documented non-mandatory limitations;
- `NOT READY` — any mandatory gate is missing, failed, or only indirectly
  supported by repository evidence.
