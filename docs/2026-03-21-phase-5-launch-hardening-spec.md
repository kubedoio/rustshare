# Phase 5: Launch Hardening Spec

Date: 2026-03-21

## Goal

Take Rustshare from late-MVP / pre-release to a launch-ready web product with stronger operational confidence.

This phase intentionally focuses on the main web/backend platform and postpones active mobile delivery.

## Scope

Phase 5 covers:

- OIDC production validation
- auth and account hardening
- observability and alerting depth
- operator launch checklists and runbook quality
- compatibility cleanup planning
- frontend/runtime dependency cleanup that affects release confidence

Phase 5 does **not** cover:

- mobile product UI work
- desktop product redesign
- large new product-surface expansion

## Workstreams

### 1. OIDC Validation

Objective:
Prove that browser and mobile-oriented OIDC flows behave correctly against the intended production identity provider.

Deliverables:

- environment-specific OIDC validation checklist
- documented successful browser login/logout test flow
- documented successful mobile PKCE authorize/exchange validation
- failure-mode notes for redirect mismatch, nonce mismatch, and expired code handling

Exit criteria:

- real IdP tested successfully
- launch configuration documented
- known limitations captured explicitly

### 2. Auth and Account Hardening

Objective:
Make account/session behavior understandable and supportable for users and operators.

Deliverables:

- finalize settings/security page behavior
- confirm password, session, and security-event surfaces are coherent
- tighten auth-path rate limits and messaging where needed
- audit compatibility aliases that still matter for clients

Exit criteria:

- account settings cover the expected security lifecycle cleanly
- auth behavior is predictable under normal and failure conditions

### 3. Observability and Alerting

Objective:
Move from “inspectable” to “operationally actionable.”

Deliverables:

- clearer replication failure and degraded-state visibility
- alerting specification for:
  - replication backlog growth
  - repeated replication failures
  - auth failure spikes
  - restore-drill failure
- operator-facing summary checklist for incident triage

Exit criteria:

- operators can identify degraded conditions quickly
- the repo documents what should trigger human action

### 4. Recovery Confidence

Objective:
Make restore and recovery repeatable, not just possible.

Deliverables:

- repeatable restore-drill checklist kept current
- documented expected outcomes after restore
- explicit verification path for auth, browsing, sharing, and public links after recovery

Exit criteria:

- recovery process is documented as an operational routine
- expected smoke outcomes are written down and current

### 5. Compatibility Cleanup Plan

Objective:
Prepare the codebase for later removal of transitional routes and assumptions without destabilizing clients now.

Deliverables:

- route-by-route compatibility removal candidate list
- deprecation notes for compatibility-only endpoints
- client impact notes for each candidate removal

Exit criteria:

- transitional surface is intentionally tracked
- later cleanup work can be scheduled without rediscovery

### 6. Frontend Runtime Confidence

Objective:
Remove avoidable release risk from frontend/runtime drift.

Deliverables:

- resolve remaining frontend dependency/runtime mismatch debt
- refresh lockfile and package state intentionally
- confirm documentation reflects the real supported frontend toolchain

Exit criteria:

- production frontend build path no longer emits known avoidable mismatch warnings

## Recommended Execution Order

1. OIDC validation
2. frontend/runtime cleanup
3. observability and alerting specification
4. recovery confidence pass
5. compatibility cleanup plan finalization

## Non-Goals

Do not spend Phase 5 on:

- expanding the desktop prototype
- redesigning the product UI broadly
- returning to mobile productization
- adding large new collaboration features

## Completion Criteria

Phase 5 is complete when:

- OIDC is validated against the real IdP
- auth and account surfaces are coherent and documented
- observability guidance is actionable
- recovery docs are current and believable
- compatibility cleanup is planned, not vague
- frontend runtime debt that affects release confidence is reduced materially

## Follow-On Phase

After Phase 5, the project can choose between:

- launch-readiness closure for the web product, or
- resuming mobile productization from the existing aligned foundation
