# Phase 4.0 — Institutional Implementation Roadmap

Status: Accepted architecture and implementation sequence, explicitly approved on 2026-10-15. Implementation review and operational release gates remain mandatory.

The 2026-10-13 sequence was planned against `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`. Merged PRs #361/#362 now supply SCIM scope and owner/Admin publication checks. Preserve historical audits; do not refile their fixed observations as live defects. This replaces the fixed 19-PR scheduling plan with bounded outcomes; implementation may split a risky change, but numbering itself is not a dependency.

## Before/after decisions

| Before | Selected correction | Authority |
|---|---|---|
| Workspace = organization | Compatibility alias only; independent workspaces deferred | ADR-0040 |
| Nil resets treated as empty scope | Absence distinct from every UUID, including nil | ADR-0042/0043 |
| Single universal Files resolver | Shared context; Application-owned authorization | ADR-0041 |
| Edit may publish | Owner or resource Admin for publication/unpublication | ADR-0041, #362 |
| Definer implies global GC visibility | Privileged global-count proof mandatory; ordinary traffic has no bypass | ADR-0042 |
| Request-long pinned connections | Explicit transaction/repository experiment; optional enforcement | ADR-0042 |
| Fresh installs inherit nil | Persist generated tenant; preserve existing nil | ADR-0040/0043 |

## Minimum pilot dependency sequence

Each implementation PR requires DCO, tests, applicable security note and human review before merge. Historical audit reports and deployed gate artifacts remain separate evidence classes.

| Slice | Scope and dependencies | Required implementation/test evidence |
|---|---|---|
| P0 — Decision and candidate baseline | Architecture contracts and bounded #330 workspace deferral Accepted on 2026-10-15; pin latest main including #361/#362/#364/#365 and assign delivery owners. Optional RLS/maintenance activation needs separate later approvals | Recorded architecture acceptance; owner assignments; post-hotfix source/test reconciliation; no claim of workspace completion |
| P1 — Identity, grants and audit foundation | Explicit tenant writes, credential drift, generated fresh identity; additive admin grants/bootstrap/recovery; tenant-attributable audit. Depends P0 | Tenant/nil creation negatives; operator/admin separation; grant/revoke/recovery; secret/content redaction. Audit must exist before exposing new mutations |
| P2 — Groups and Files/Notes authorization | Tenant-bound membership/rosters, recipient validation, shared resolver within Files/Notes, effective explanations, index refresh parity. Depends P1 and read-only legacy inventory | Owner need not join recipient group; overlap/removal; cross-tenant non-disclosure; source/HTTP parity; stale search denies; #362 publication/unpublication regression |
| P3 — Bulk lifecycle and administration UI | Bounded dry-run/apply/retry, invites, disable and safe re-enable policy, groups/grants/audit UI. Depends P1/P2 | #330 non-workspace scope; #331 retry/partial-failure tests; #332 scoped pagination/export; no routine SQL access |
| P4 — Institutional journey and recovery | Calendar → meeting → linked notes/files → decisions/actions → historical reconstruction/search; parallel owner work may start after P0, release requires P2/P3 | #327/#328/#329 gap closure; linked resources independently authorize; decisions owners/state; restore/upgrade and ordinary operator journey |
| P5 — Candidate gates A/B/C | Integrate P1–P4 on exact deployment candidate; optional RLS excluded | 10–15, 30–50, approximately 100-user evidence in [acceptance gates](../pilot/2026-10-13-institutional-acceptance-gates.md); no fabricated production results |

P1 need not be a single large PR: grants/audit may ship inert first. Audit is a real exposure dependency; arbitrary ordering of documentation is not. #287 repository boundaries are improved on affected paths; a whole-repository SQL rewrite is not a pilot prerequisite. Existing #361/#362 work is regression coverage, not new duplicate PRs.

## Independent database-hardening track

1. Provision least-privilege runtime credentials, separate migration/bootstrap procedure, backup/operator compatibility and role drift checks. Runtime credentials must differ from migration/operator credentials and request pools must not execute migrations. Explicit predicates and negative tests are mandatory pilot posture; subsequent enforcement RLS remains optional.
2. Prototype explicit SQLx repository/transaction boundaries and transaction-local context on representative Files/Notes/search/worker paths. Prove missing/empty/nil/non-nil context and pool reuse behavior against actual PostgreSQL. Compare throughput/latency/pool wait against baseline; no HTTP-duration or streaming connection ownership.
3. Independently verify privileged global blob counts under FORCE RLS and lock/lifecycle safety. Cross-tenant shared blob and failure tests must pass. GC and orphan reconciliation stay disabled.
4. Rehearse policies/roles/backup/restore/downgrade in non-production; enable only per-table after ADR-0042 human gates and measured acceptance. A policy change requires its own release review.

These outcomes may consolidate the old DB-01…DB-07 sequence. Enforced RLS is not required for pilot if application-layer isolation, least privilege and operational gates pass. GC/orphan activation is a separate approval and cannot follow merely from RLS success.

## Legacy lane

Run aggregate-only, read-only inventory before stricter semantics or deployment upgrade. Operator reviews unavailable legacy records and approves mapping separately under ADR-0043. No destructive migration, tenant rewrite or inferred ownership is in this package. Restore/rollback evidence precedes any future backfill. This lane blocks changes affecting inventoried data, not unrelated audit/UI work.

## Decision → implementation → test traceability

Stable slice IDs P0–P5 replace old scheduling IDs. Crosswalk: AR-01/02 → P1 (#364/#361 baseline); AR-03/04/05/07 → P2 (#365/#362 partial baseline); AR-06/08 → P1/P3; AR-09/10 → P3; AR-11 → P5; DB-01…07 → database track steps 1–4; LG-01 → legacy lane. Historical evidence retains original IDs; this mapping neither duplicates work nor claims all original acceptance items shipped.

| Decision | Slice | Mandatory proof |
|---|---|---|
| Tenant identity/workspace compatibility, ADR0040 D1–5 | P1/P4 | Explicit writes, nil compatibility, alias mismatch denial; no membership UI; future API field not reinterpreted |
| Admin and provisioning, ADR0040 D6–7 | P1/P3 | Scoped grants, recovery, #361 regression; caller cannot select SCIM scope |
| Group/resource authorization, ADR0041 | P2 | Same-tenant grants, recipient validation, overlapping grants, immediate revoke, source parity, owner/Admin publication |
| Context/roles/global counts, ADR0042 | Database track | Real PostgreSQL nil/absence/reuse and FORCE-role cross-tenant count tests; failure blocks deletion |
| Historical nil populations, ADR0043 | Legacy lane | Aggregate inventory fixtures, reviewed mapping, restore rehearsal; no automatic mutation |
| Product workflow, #333 | P4/P5 | Meeting/resource/decision/action reconstruction, permission-aware Memory and unaided administration |

## Shipped, incomplete and deferred

Source contains Files/Notes, owner-scoped Calendar, meeting-resource/decision code, unified permission-aware search and backup/restore tooling. These are components, not proof of a functioning institutional deployment. SCIM scope and publication hotfixes are merged. Current source lacks independent workspace membership and organization-scoped administrator grants; group/bulk/audit/UI integration and the complete meeting journey still require candidate evidence. No Gate A/B/C is claimed here.

Deferred: independent workspaces (bounded deferral approved on 2026-10-15), multi-organization human identity, nested groups, independently scoped IdPs, optional RLS enforcement, universal policy engine and new distributed services. Agents/RAG and additional Applications evolve through ResourceRef/source authorization, not tenant membership or index ACL shortcuts.

## Human gates

CODEOWNERS @senolcolak and @zoorpha are accountable for assigning named product/security/operator owners before Gate A. Missing designation or approval blocks the affected gate.

Option B, the #330 bounded milestone amendment and ADR-0040–0043 architecture contracts were explicitly Accepted on 2026-10-15. Security owner still reviews implementation and residual-risk dispositions for publication/sharing, revocation/re-enable, grants/recovery and database role/context behavior. Operator approves deployment-specific legacy impact, secrets, persisted tenant identity and recovery drills. Deployment candidate evidence, operational activation and release approval remain mandatory. None is implied by architecture acceptance or green CI.
