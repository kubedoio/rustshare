# Beta Tester Onboarding Runbook (Operator)

> **Audience:** maintainers/operators onboarding testers onto the hosted Elembra
> Public Beta.
> **Plan:** `docs/plans/2026-09-30-beta-readiness-plan.md`
> **Gate:** `docs/releases/beta-gate.yaml` (`beta_tester_cohort`,
> `beta_data_policy`)

This is the supported procedure for giving a tester access to the hosted beta
instance. It replaces ad-hoc account sharing. Every step happens through the
supported UI/API — never raw SQL (consistent with the Customer Alpha runbook
rule).

## Prerequisites

- The hosted beta environment gate is PASS (TLS, OIDC or accepted
  password-login mode, monitoring live).
- `docs/beta/tester-guide.md` and `docs/beta/known-limitations.md` are current.
- The beta agreement template (see §2) has been reviewed and, where required
  by your organization, signed off.

## 1. Screen the tester

- Confirm they fit the beta scope (web-only usage; no self-hosting support).
- Record: name, email, organization/team, role, what they intend to test,
  preferred login method (OIDC if the IdP is live, otherwise password).
- Set expectations up front using the tester guide: this is a beta, data may
  be reset on major incidents, there is no data-import guarantee, and
  feedback goes through the issue tracker with the feedback template.

## 2. Beta agreement essentials

At minimum the agreement (email reply or signed doc, per your process) must
state:

1. The service is a **beta**: features can change or break without notice.
2. **Do not store real secrets or production data** in the beta instance.
3. Data is hosted by the operator, backed up per the backup/restore runbook
   (RPO < 24h), and **deleted on offboarding** (see §6).
4. Testers report security issues privately per `SECURITY.md`, never in
   public issues.
5. The beta ends on a communicated date; testers get an export window before
   final teardown.

## 3. Create the account

Through the admin UI (or `POST /api/v1/admin/users`):

1. Create the user with a strong initial password (or the OIDC identity once
   the IdP mapping is configured). Deliver the initial secret through a
   separate channel from the welcome email; force the password change.
2. Assign the tester to a **beta-testers group** (create one if this is the
   first tester) so cohort-wide permission changes and offboarding are one
   operation, not N.
3. Do **not** grant admin rights unless the tester explicitly tests admin
   flows; record which testers hold admin.

## 4. Welcome the tester

Send the welcome email containing:

- the instance URL and login instructions;
- `docs/beta/tester-guide.md` (link or attachment);
- `docs/beta/known-limitations.md`;
- the feedback channel (issue tracker with
  `docs/beta/feedback-template.md`) and the SLA;
- the support contact for Sev-1 reports.

Verify first login (admin UI: last-login check, or the audit log) and that
the tester completed one end-to-end task: **upload a file → share it → open
it as a recipient**. This is the Phase 2 exit criterion per tester.

## 5. During the beta

- Triage incoming feedback at the weekly slot; first response within the
  published SLA (Sev-1 ≤ 4h, else ≤ 2 business days).
- Include the tester in the weekly digest (what shipped, what broke, what's
  next).
- After every deploy to the beta host, run `scripts/run-beta-smoke.sh` and
  save the report under `./beta-smoke-reports/`.
- Never debug tester data with raw SQL; use the supported admin endpoints.

## 6. Offboard the tester

Offboarding must prove the `beta_data_policy` gate. For each departing
tester:

1. Revoke Elembra access: disable the account via admin UI/API (never raw
   SQL). Confirm login now fails.
2. Revoke Buzz/Chat access: use the admin Chat revocation endpoint/UX so the
   Buzz binding is revoked fail-closed.
3. Revoke any shares the tester created that point at their owned content,
   if the content is being deleted.
4. Export (if requested): the tester downloads their own files/notes through
   the UI before deletion; confirm completion.
5. Delete or anonymize the account per the agreement, and confirm their
   content is gone from the UI.
6. Record the offboarding in the beta gate evidence (`beta_data_policy`:
   executed at least once, with the date).

## 7. Cohort records

Keep (outside the repository — it contains personal data):

- the tester list (email, org, start date, agreement status);
- admin-rights flags;
- offboarding dates and data-deletion confirmations.

The repository's gate file records only counts and dates, never tester
identities.
