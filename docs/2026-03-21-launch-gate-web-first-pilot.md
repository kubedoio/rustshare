# Launch Gate: Web-First Pilot

Date: 2026-03-21

## Gate Name

**Web-First Pilot Gate**

This is the formal launch-decision gate for the current Rustshare product.

## Purpose

Decide whether Rustshare is ready for a careful web-first pilot or whether it must remain pre-release.

## Gate Inputs

All of these must be available before passing the gate:

- completed [Phase 6: Environment Sign-Off Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-6-environment-signoff-spec.md)
- completed [OIDC Production Validation Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-oidc-production-validation-checklist.md), or an explicit OIDC-out-of-scope decision for the current pilot profile
- target-environment restore drill evidence
- monitoring/alerting wiring confirmation
- final launch smoke evidence

## Pass Criteria

Rustshare passes the Web-First Pilot Gate only if:

- browser auth works with the real IdP when OIDC is in scope, or OIDC is correctly disabled for the pilot profile
- backups are trusted and restore evidence exists
- operators can see and respond to critical failure modes
- critical file-sharing journeys work in the target environment
- no unresolved launch-blocking auth, restore, or replication issue remains

## Fail Criteria

The gate fails if any of these are true:

- OIDC is required for the pilot but unvalidated or unstable
- restore drill evidence is missing or failing
- alerting is not wired for critical failure classes
- a critical user journey is broken in the target environment

## Outcome Options

### Pass

Recommendation:

- proceed with a careful web-first pilot
- keep scope narrow
- do not market the broader platform as fully complete

### Conditional Pass

Recommendation:

- allow only limited internal or design-partner rollout
- record explicit follow-up items with deadlines

### Fail

Recommendation:

- remain pre-release
- return to the specific blocking workstream

## Explicit Non-Claims At Gate Pass

Passing this gate does **not** mean:

- mobile is release-ready
- desktop is release-ready
- compatibility cleanup is finished
- long-term mature ops dashboards are complete

It means only that the web product is acceptable for a careful pilot.
