# Phase 6: Environment Sign-Off Spec

Date: 2026-03-21

## Goal

Move from repository-level hardening to environment-specific launch sign-off for the web product.

Phase 6 is where Rustshare proves that the hardened codebase works in the real deployment context, not just in local or repository-level validation.

## Scope

Phase 6 covers:

- production-identity-provider OIDC validation
- target-environment restore drill evidence
- monitoring-stack alert wiring
- final launch-environment smoke checks
- operator sign-off inputs for a web-first pilot

Phase 6 does **not** cover:

- new product features
- mobile productization
- desktop feature work
- broad compatibility removals

## Workstreams

### 1. Production OIDC Validation

Objective:
Run the browser and mobile-oriented OIDC flows against the actual identity provider and final redirect configuration when OIDC is in launch scope.

Deliverables:

- completed [OIDC Production Validation Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-oidc-production-validation-checklist.md), or an explicit OIDC-out-of-scope decision for the current pilot profile
- final redirect URI set recorded
- known provider-specific caveats recorded

Exit criteria:

- browser login/logout proven when OIDC is enabled
- mobile PKCE authorize/exchange proven when OIDC is enabled
- failure modes captured, or OIDC explicitly disabled and not advertised in the runtime

### 2. Recovery Proof In Target Environment

Objective:
Prove that backup, restore, and post-restore behavior work in the intended deployment environment.

Deliverables:

- one recorded target-environment restore drill
- post-restore results checked against [Post-Restore Expected Outcomes](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-post-restore-expected-outcomes.md)
- operator notes captured for timing and failure points

Exit criteria:

- a real backup artifact has been restored successfully
- auth, browsing, sharing, and download work after restore

### 3. Monitoring And Alert Wiring

Objective:
Turn the alerting guidance into real operational signals in the chosen monitoring stack.

Deliverables:

- replication-health alert rules configured
- auth failure spike detection configured
- backup verification failure visibility configured
- restore drill failure visibility configured

Exit criteria:

- operators receive the right signals in the chosen monitoring system
- alert thresholds match [Alerting And Incident Thresholds](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-alerting-and-incident-thresholds.md)

### 4. Final Launch Smoke

Objective:
Verify the critical user journeys in the target environment.

Required journeys:

- login
- logout
- upload
- download
- internal share
- public link
- upload-only link
- restore/trash
- replication visibility

Exit criteria:

- all critical paths are exercised and recorded

## Completion Criteria

Phase 6 is complete when:

- OIDC is validated against the real IdP, or explicitly out of scope and disabled for the current pilot environment
- one target-environment restore drill is recorded
- alerting is wired in the actual monitoring stack
- final launch smoke is recorded successfully

## Next Step

After Phase 6, the project reaches the launch decision point described in:

- [Launch Gate: Web-First Pilot](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-launch-gate-web-first-pilot.md)
