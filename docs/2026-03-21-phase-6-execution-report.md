# Phase 6 Execution Report

Date: 2026-03-21

## Environment

This execution report covers the current Docker Compose deployment profile running on:

- base URL: `http://localhost`
- API base URL: `http://localhost/api/v1`
- profile: password-login web pilot

## Fixes Made During Phase 6

- Corrected browser OIDC config detection so empty OIDC environment variables no longer advertise OIDC as enabled in [backend/server/src/oidc.rs](/Users/scolak/Projects/x/rustshare/backend/server/src/oidc.rs)
- Added a reusable launch smoke script in [scripts/final-launch-smoke.sh](/Users/scolak/Projects/x/rustshare/scripts/final-launch-smoke.sh)

## Executed Evidence

### 1. Runtime Auth Profile

Observed live auth config:

```json
{"password_login_enabled":true,"oidc_enabled":false,"oidc_login_label":null,"oidc_mobile_enabled":false}
```

Interpretation:

- password login is in scope
- browser OIDC is not configured in this environment
- mobile OIDC is not configured in this environment

### 2. Fresh Backup Artifact

Created:

- `/Users/scolak/Projects/x/rustshare/backups/20260321T103018Z`

### 3. Restore Drill

Executed successfully with:

- [scripts/run-restore-drill.sh](/Users/scolak/Projects/x/rustshare/scripts/run-restore-drill.sh)

Recorded evidence:

- `/Users/scolak/Projects/x/rustshare/restore-drill-reports/20260321T103036Z-restore-drill.env`

### 4. Final Launch Smoke

Executed successfully with:

- [scripts/final-launch-smoke.sh](/Users/scolak/Projects/x/rustshare/scripts/final-launch-smoke.sh)

Recorded evidence:

- `/Users/scolak/Projects/x/rustshare/launch-smoke-reports/20260321T103231Z-final-launch-smoke.env`

Covered flows:

- admin password login
- viewer password login
- root listing
- folder creation
- private file upload
- private download URL
- internal share visibility
- public file link download
- upload-only public folder upload
- replication summary endpoint
- logout

### 5. Replication Visibility

Current operator check result:

- replication health: healthy
- enabled targets: `0`
- required targets: `0`

This is acceptable for the current local pilot profile, but it is not equivalent to a replicated production deployment.

## OIDC Scope Decision

OIDC is explicitly out of scope for this current pilot environment because:

- no OIDC issuer or client values are configured
- the runtime now reports OIDC as disabled correctly
- `/api/v1/auth/oidc/login` returns `404 OIDC is not configured`

This means the OIDC production validation checklist is `not in scope` for this environment, not “passed.”

## Remaining Gaps

- no real external identity provider is configured in this environment
- no external monitoring / alerting stack is wired here
- no replication targets are configured here

## Outcome

Phase 6 is executed for the current Docker-based password-login pilot profile.

This execution supports a **conditional** launch-gate result for a narrow web-first pilot, not a broad production sign-off.
