# ADR-0043: Legacy Nil-Tenant Compatibility and Non-Destructive Migration

Status: Proposed; maintainer, security-owner and operator approval pending. No data rewrite authorized.
Date: 2026-10-13; corrected 2026-10-15
Historical baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`

## Problem, factual behavior and product requirement

Historical deployments use zero UUID as a real tenant; defaults/legacy writers also produce nil rows. This does not
prove all nil rows have coherent ownership. Preserve customer data and predictable authorized access without nil fallback.
The [inventory plan](../migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md),
[evidence matrix](../audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md) and
[negative report](../audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md) retain their original baseline.
Later SCIM/publication fixes do not prove legacy ownership or retroactively erase historical observations.

## Selected design, alternatives and authority

[ADR-0040](0040-organization-tenant-and-workspace-identity.md) owns provisioning/workspaces;
[ADR-0041](0041-groups-and-authorization.md) owns authorization;
[ADR-0042](0042-postgresql-tenant-isolation-rls-staging.md) owns RLS/context/GC. This ADR owns compatibility/migration safety.

| Meaning | Contract |
| --- | --- |
| Nil historical tenant | Legal exact identity for coherent existing deployments. |
| Absent/uninitialized context | SQL NULL/empty setting, never zero UUID; ADR-0042. |
| Implicit nil column default | Historical behavior to retire after writer inventory and explicit assignment. |
| Nil authorization wildcard | Always forbidden. |

Choose preservation plus explicit writes/reviewed classification. Reject blanket nil rejection (breaks customers),
nil permissiveness (exposure), inference from names/email/groups (unreliable), automatic remapping (data loss) and
permanent legacy bypass mode.

Fresh deployments provision an explicitly generated tenant UUID; nil only preserves verified historical/import
contracts. This is target behavior, not a claim current bootstrap implements it. Organization configuration supplies
authoritative tenant; do not invent another organization identity for display metadata. Writes require trusted explicit
assignment and related resource/grant/recipient consistency checks. Retire database defaults only after every writer,
worker, import and test assigns tenant explicitly. A type signature alone does not prove validation.

## Classification, inventory and security invariants

Run aggregate-only read-only inventory on a consistent snapshot with independently verified all-tenant visibility.
Filtered inventory cannot establish completeness. Preflight role/visibility and fail on unknown scope, missing required
tables or permission errors. Report counts/mismatch categories, never content, tokens, names/emails or ownership guesses.
Store detailed operator evidence outside public reports where necessary.

Classify coherent nil-only deployments, consistent non-nil data, ambiguous/mixed nil rows and cross-tenant related-row
inconsistencies. No class authorizes widening. Coherent nil data retains normal exact-tenant/resource authorization.
Ambiguous data remains stored and excluded from stricter user/public/search/AI exposure until reviewed; public links
cannot bypass this. Quarantine means denied reachability and an operator review queue/report, not an implemented
quarantine schema/UI.

Stricter checks may turn previously reachable data into 403/404 or remove search hits. Preserve non-disclosure, audit
aggregate denial categories, notify authorized operators and document recovery via reviewed mappings. Do not restore
old exposure or require destructive cleanup to pass pilot gates. Stale search ACLs never grant source access.
No automatic ownership transfer or caller-selected tenant.

## Data/schema and API impact

No data/schema changes here. A future migration needs table-by-table source→target mapping, related-row/grant checks,
conflict policy, SQLx metadata where applicable and reviewed reversible steps. Explicit writes are internal changes;
nil remains legal for existing customers. Inconsistent grants remain denied, not translated into a public API.
Organization-facing errors expose only authorized recovery guidance. No compatibility flag grants privilege.

## Operations, performance, migration and rollback

Operator/maintainer approve mappings, security owner approves authorization and data owner approves ownership decisions.
Name actual people in each deployment change record. Verify backup and rehearse on a restored clone; retain aggregate
before/after counts, authorization results and rollback artifacts/checksums in access-controlled storage. Review locks,
batching, indexes and write coordination against actual data before choosing windows. No scale benchmark is claimed.

Rollback restores related rows, grants and object references, not only tenant columns. Never overwrite new writes with
a stale restore without approved recovery-window/reconciliation steps. Restore tests include nil/non-nil data, shares,
grants and object reachability. Destructive rewrites/backfills/orphan cleanup remain outside this decision. Failed
rehearsal blocks migration, not strict checks. Preserve historical reports with dated supplements.

## Required negative tests and acceptance criteria

- Coherent nil users/resources/shares work under exact tenant/effective permission; non-nil and absent DB context cannot
  reach them (ADR-0042 SQL semantics verified; SQLx end-to-end gate pending).
- Nil never acts as wildcard; absent assignments fail; fresh bootstrap uses configured generated UUID.
- Mixed/cross-tenant joins, grants, imports and SCIM cannot assign caller-selected ownership.
- Ambiguous data stays stored while direct/public/search/Memory access is denied; recovery never widens unrelated grants.
  Group names/email/display metadata do not auto-remap data.
- Inventory stays aggregate/read-only and fails on filtered/unknown visibility or missing tables.
- Clone migration and rollback/restore preserve row/reference totals and authorized reachability including nil fixtures.

Writer inventory, explicit assignment coverage and fresh/legacy provisioning tests gate default removal. Deployment
inventory and human mapping approval gate backfill. Restore proof gates migration. ADR-0042 approval gates FORCE.
These are pending implementation/deployment gates, not executed evidence. The pilot may proceed with coherent data and
application isolation independently of optional RLS.

## Human approval, deferred work and revisit triggers

Maintainer approves writer/default retirement; security owner approves quarantine/recovery and isolation; operator/data
owner approves mappings/recovery windows. CI is not approval. Deferred: destructive changes, automated backfill,
tenant-scoped naming/cross-organization identities (ADR-0040), optional RLS (ADR-0042). Postponed mapping retains denied
ambiguous data, not a bypass. Nil stays legal until authorized migration; it is not the implicit context or new-install
default. Revisit only on verified compatibility demand or an explicit reviewed mapping, never aesthetic renumbering.
