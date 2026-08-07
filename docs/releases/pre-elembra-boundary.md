# Pre-Elembra Release Boundary

> Last updated: 2026-08-08

## The boundary

**RustShare v0.7.0 is the final release of the current RustShare architecture.**

```
RustShare <= v0.7.0
    final pre-Elembra architecture

---------------- RELEASE BOUNDARY ----------------

Elembra migration
    Application architecture
    #196 / #209 / #210+
```

Everything from the next development line onward is the **Elembra** architecture
transition: `Module` boundaries become `Application` boundaries, cross-module
relationships move to `ResourceRef` and contract-driven authorization, and the
platform is intentionally re-cut around owned Applications (Files, Notes, Mail,
Memory, Chat, Agents).

## What v0.7.0 represents

- The final stable, reproducible baseline of the legacy RustShare architecture.
- The last release where the codebase still speaks the current Module-era
  vocabulary and storage model.
- The last version an operator may safely treat as "old architecture" without
  Elembra compatibility expectations.

## Tracking future work

Future breaking architecture work is tracked by:

- **#196** — the Elembra epic (roadmap).
- **#209** — the Elembra Application architecture definition (docs only, not
  yet merged into `main`).
- **#210+** — implementation phases: Module → Application cutover, ResourceRef /
  PrincipalContext contracts, transactional outbox / Integration Events,
  Connector foundation, Buzz-powered Chat, Object Spaces, Elembra Memory.

## Compatibility warning

- **Upgrades beyond v0.7.0 may include deliberate compatibility-breaking
  architecture migrations.** Do not assume a future v0.8.0/1.x upgrade is a
  drop-in replacement for a v0.7.0 deployment.
- **User data migration will be addressed by the Elembra migration work.** Data
  created under the v0.7.0 Module-era schema is expected to be carried forward
  by the migration plan; the mechanism and schedule are Elembra scope, not
  v0.7.0 scope.

## Operator guidance

- **v0.7.0 remains reproducibly buildable and tagged** as the last
  old-architecture baseline: the `v0.7.0` tag marks the release, and the release
  pipeline publishes binaries, container images, and SBOMs for it.
- Pin to `v0.7.0` (or a later release tag) explicitly; do not track `latest`
  across the Elembra cutover unless the Elembra migration plan says otherwise.

This note is intentionally brief. The authoritative architecture definition for
the Elembra transition lives with #209 and its repository documents; do not
duplicate them here.
