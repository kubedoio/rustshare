# FWS pilot qualification checklist

This checklist separates repository evidence from evidence that only an authorized FWS operator can produce. A green hosted workflow is not a deployment qualification. Keep the pilot **NO-GO** until every required gate below has dated evidence for the exact candidate commit and image digest.

## 1. Implementation readiness

- [ ] Select the exact protected-main commit and immutable backend image digest. Record both in the evidence bundle.
- [ ] Confirm the candidate's required branch-protection checks are green, including Format, Clippy, Cargo Deny, integration tests, frontend, Chat Product Acceptance, Buzz conformance and identity E2E where applicable.
- [ ] Run the disposable Pilot Release workflow and retain its sanitized report and artifact checksums.
- [ ] Run `scripts/run-restore-drill.sh` against disposable volumes; verify the safety test and restore result.
- [ ] Review `docs/pilot/runbook.md`, Compose security configuration, environment variables and rollback instructions for the exact candidate.
- [ ] Record unresolved security alerts and their authorized dispositions. Do not treat an open alert as accepted without its documented decision.

## 2. Operator prerequisites

The FWS owner supplies and records:

- [ ] Pilot hostname, DNS owner, certificate issuer/renewal owner, HTTPS endpoint and expected redirect/callback URLs.
- [ ] Host OS/version, Docker Engine and Compose versions, private interface/bind address, storage capacity and maintenance window.
- [ ] Access method: dedicated protected self-hosted runner, or an operator-managed SSH account/key and verified host key. No shared runner with production credentials.
- [ ] Registry reachability and least-privilege read credentials if the candidate image is private. Record the allowed registry/repository; never place credentials in the repository.
- [ ] Authentication decision: password baseline or OIDC. For OIDC, provide issuer, client registration, exact callback/logout URLs, claims/groups mapping and a disposable test tenant.
- [ ] Protected `fws-pilot` environment, branch restrictions and named human approver for deployment jobs.
- [ ] Operator secret-store owner and custody procedure for JWT, encryption, database, RustFS and selected OIDC secrets. Confirm backup/rotation contacts and recovery access.
- [ ] Two authorized synthetic users: independent administrators and a least-privilege user. No real institutional identities or content.
- [ ] Durable backup destination outside the application host, retention, encryption, access control and a named restore operator.
- [ ] Monitoring/alerting destination, on-call/support owner, escalation route and log retention/redaction policy.
- [ ] Pilot cohort size, duration, entry/exit criteria, incident contact and explicit stop authority.

If any item is unavailable, record its owner and keep deployment qualification blocked. Do not substitute CI secrets or developer accounts for operator custody.

## 3. Disposable qualification (engineering-owned)

Run against an isolated disposable deployment with synthetic data and unique Compose project, ports, database and object-store bucket:

- [ ] Clean install from the exact candidate artifact; record checksums and redacted configuration hash.
- [ ] Health and readiness checks pass; verify no public PostgreSQL or object-store admin port.
- [ ] Browser/API journey: sign in as admin, create a second admin and least-privilege user, verify workspace access boundaries, Files upload/download, Notes, Search/Memory citations, Chat and audit activity.
- [ ] Verify user disable/revocation and that another tenant/user cannot open the revoked source or citation.
- [ ] Restart services and host-equivalent containers; verify persisted data and readiness.
- [ ] Upgrade from the previous pilot candidate with a pre-upgrade backup; run the same acceptance journey.
- [ ] Roll back to the previous image/configuration and prove data remains readable. Record any one-way migration limitation.
- [ ] Restore to new disposable volumes from the backup, verify Files/Notes/Chat references and login, and compare expected counts/checksums.
- [ ] Capture sanitized browser report, workflow URLs, exact commit/image digest, timestamps and operator identities. Exclude credentials, source text and personal data.

## 4. FWS deployment acceptance (operator-owned)

Only after Sections 1–3 pass and the environment approval is recorded:

- [ ] Verify FWS DNS/TLS/HTTPS from an authorized external client; confirm redirects and any OIDC callbacks.
- [ ] Verify the deployment host, image provenance/digest, secret source, bind addresses, volumes, backup target, monitoring and alerts against approved values.
- [ ] Perform a clean installation in the approved pilot environment with synthetic data.
- [ ] Run the institutional browser/API acceptance journey with only authorized synthetic users.
- [ ] Restart and restore from the actual configured backup destination into an approved disposable recovery target.
- [ ] Rehearse candidate upgrade and rollback using the documented operator procedure.
- [ ] Have an independent operator verify recovery, access control, alerts and support handoff.
- [ ] Record exact deployment commit/digest, evidence report, deviations, approvals and go/no-go decision.

Do not import institutional data or invite the cohort during qualification. Cohort entry requires explicit owner approval after this section passes.

## 5. Readiness record

| Evidence class | Owner | Required evidence | Status |
| --- | --- | --- | --- |
| Implementation | Engineering | Exact commit, review, tests, open-risk dispositions | Pending per candidate |
| Hosted CI | Engineering | Required workflow run IDs and sanitized artifacts | Candidate-specific |
| Disposable integration | Engineering | Clean install, browser/API, upgrade, rollback, restore | Candidate-specific |
| FWS deployment | Pilot operator | Host/HTTPS/config/backup/monitoring evidence | Blocked on operator access |
| Institutional acceptance | Product/security owner | Synthetic-user browser journey and approval | Blocked on operator decision |

## References

- [`runbook.md`](runbook.md)
- [`pilot-readiness-report.md`](pilot-readiness-report.md)
- [Issue #333](https://github.com/kubedoio/rustshare/issues/333)
- [Issue #335](https://github.com/kubedoio/rustshare/issues/335)
