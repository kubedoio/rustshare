# Mobile Postponement Decision

Date: 2026-03-21

## Decision

Rustshare mobile work is postponed as an active delivery phase.

This does **not** mean the mobile foundation is being discarded. It means the current aligned standalone mobile client is considered a preserved foundation, not the next immediate product milestone.

## What Exists Today

The standalone mobile workspace at `/Users/scolak/Projects/x/rustshare-mobile` now has:

- Android and iOS trees aligned to `/api/v1` and `/api/ws`
- native mobile OIDC callback handling
- secure token storage
- folder browsing
- picker upload
- explicit offline downloads with local tracking
- queued photo/video backup into a dedicated remote folder

That work is valuable and remains the correct base for future client work.

## Why Mobile Is Postponed

The highest-risk remaining work for Rustshare is no longer core file-sharing behavior. It is launch hardening and operational confidence around the main web product.

The project now benefits more from:

- end-to-end OIDC validation against the intended production identity provider
- alerting and observability improvements
- cleanup of compatibility and historical drift
- operator-facing launch readiness and runbook quality
- tighter production confidence around restore, replication, and auth

Continuing mobile immediately would expand product surface before the launch path for the main platform is fully stabilized.

## New Priority Order

1. Web launch hardening and operator readiness
2. OIDC production validation
3. Observability and alerting depth
4. Compatibility cleanup and technical-debt reduction
5. Resume mobile productization later from the existing aligned foundation

## What Is Deferred

Deferred mobile work includes:

- replacing diagnostics-first host shells with product UI
- background execution and retry hardening for photo backup
- websocket-assisted mobile refresh behavior
- share-sheet and native capture polish
- release packaging and store readiness

## Resume Conditions

Mobile should be resumed when these are true:

- the frozen `/api/v1` contract remains stable in practice
- OIDC is validated against the real launch identity provider
- replication and restore operations have stronger production confidence
- launch-readiness work for the web product is materially complete

## Status Implication

Phase 4 is still considered complete at the mobile-foundation level.

What changes is the roadmap priority:

- mobile is no longer the next active delivery phase
- it becomes the next deferred productization phase after launch hardening
