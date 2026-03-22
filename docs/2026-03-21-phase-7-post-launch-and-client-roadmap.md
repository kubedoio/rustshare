# Phase 7: Post-Launch And Client Roadmap

Date: 2026-03-21

## Goal

Define the first major post-launch phase after a successful web-first pilot gate.

Phase 7 is intentionally split between:

- cleanup and hardening that are safer after launch
- deferred client productization work

## Scope

Phase 7 covers:

- compatibility-route removal work
- broader observability improvements
- mobile productization restart
- optional desktop prioritization review

Current execution note:

- compatibility cleanup wave 1 is complete: realtime aliases now route only through the frozen `GET /api/ws` endpoint
- compatibility cleanup wave 2 is complete: legacy `/api/auth/...` aliases have been removed
- compatibility cleanup wave 3 is complete: unversioned file, folder, share, notification, and public-share aliases have been removed

Phase 7 does **not** automatically include:

- large new collaboration features
- desktop sync engine ambitions
- document editing or plugin ecosystems

## Workstreams

### 1. Compatibility Cleanup Execution

Objective:
Start removing transitional route families in a dedicated cleanup phase.

Use:

- [Compatibility Removal Plan](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-compatibility-removal-plan.md)

Expected order:

1. realtime aliases
2. legacy auth aliases
3. unversioned resource aliases

### 2. Observability Expansion

Objective:
Go beyond minimum launch alerting into more mature operational visibility.

Examples:

- dashboards
- trend views
- incident history summaries
- error tracking integration

### 3. Mobile Productization Restart

Objective:
Resume work from the aligned standalone mobile foundation and turn it into a release-quality client.

Priority order:

1. better product UI instead of diagnostics-first host shells
2. background photo backup behavior
3. websocket-assisted refresh
4. share-sheet and native capture polish

### 4. Desktop Reassessment

Objective:
Decide whether the desktop prototype becomes a real product phase.

Rule:

- only continue desktop if it has clear product priority after the web-first pilot

## Completion Criteria

Phase 7 is complete when:

- the first three compatibility-removal waves are shipped cleanly
- observability has moved beyond minimum launch thresholds
- mobile has re-entered active productization with a clearer release path

## Dependency

Phase 7 should only begin after:

- Phase 6 is complete
- the [Web-First Pilot Gate](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-launch-gate-web-first-pilot.md) has passed or conditionally passed
