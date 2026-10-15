# PR #360 architecture correction and final review

Date: 2026-10-15. Status: Proposed; human architecture approval outstanding.
Repository: kubedoio/rustshare. PR: https://github.com/kubedoio/rustshare/pull/360.
Reviewed main: `cb31eff928db1dd04d5811b9def3d3140eff9878`.
Original PR head: `46433c042e8b45274e54991e054a81c968f12433`.
Historical audit baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`.

## A. Executive verdict

The corrected proposal is intended for human architecture review, not production
acceptance. ADR-0040–0043 remain Proposed. No architecture approval, institutional
workflow acceptance, SQLx enforcement proof or customer deployment evidence is
conferred by this documentation package. Outstanding implementation and human
gates are explicit in the roadmap and institutional acceptance gates.

## B. Before/after decisions and traceability

| Original problem | Corrected decision | Authority and verification |
| --- | --- | --- |
| Workspace permanently equated with organization | Option B: defer workspace functionality; current 1:1 fields are compatibility aliases only | ADR-0040; ADR-0038 amendment; #330 scope approval remains required |
| Reset context to legal nil tenant | Absence is SQL NULL/empty, never a UUID sentinel; explicit nil is valid | ADR-0042; disposable PostgreSQL verification supplement |
| Ordinary definer assumed to bypass FORCE | Privileged maintenance requires proven global visibility and independently gated credentials | ADR-0042; PostgreSQL FORCE/definer tests |
| Edit described as publication authority | Owner or effective resource Admin for either visibility toggle | ADR-0041; merged #362, `backend/server/src/services/note_service.rs::toggle_visibility` |
| Files resolver becomes universal engine | Shared principal context, Application-owned resource decisions | ADR-0041; ResourceRef and indexing contract supplements |
| RLS marketed beyond its trust boundary | Protect omitted predicates; app-settable GUC cannot resist arbitrary SQL by same role | ADR-0042 threat model |
| Request-long pinning and fixed pool assumed sufficient | Pilot least privilege + predicates; optional future transaction-local context | ADR-0042; actual PgPool repository interfaces |
| Historical bypasses presented as current | Preserve dated audits, add merged-hotfix reconciliation | #361 `3c27353c`, #362 `fa091ec0`, #364 `2c609cc7`, #365 `cb31eff9`; post-hotfix supplement |
| Sharer must join recipient group | Grant authority and recipient membership are separate checks | ADR-0041; merged #365 group/share services and tests |
| Fresh installations forced into legacy nil | Persist generated UUID at first setup; existing nil remains legal | ADR-0040/0043; implementation and operator gates |
| Nineteen planned slices treated as dependencies | Consolidate by real delivery dependencies; independent database-hardening lane | Revised implementation roadmap |

## C. Product fit

Platform Core owns durable principal/tenant/group/Application-grant context.
Files, Notes, Mail, Chat/Buzz, Memory and Agents retain resource authority. This
keeps Elembra a Sovereign Business Memory Platform with a small operational core.
The institutional workflow, rather than the user count alone, defines pilot
acceptance: onboarding, calendar/meeting, linked evidence, decisions/actions,
permission-aware rediscovery, revocation and operator recovery.

Deferring workspaces does not declare #330 complete or remove future collaboration
contexts. Multiple tenant-bound workspaces require independent IDs and versioned
contracts; membership cannot grant content access implicitly. Memory and Agents
must reauthorize source access before protected content is materialized. An
initial deployment of approximately 100 users is an adoption target, not a scale
ceiling. Features missing from the real workflow remain delivery gates, never
claims of successful production use.

## D. Security assessment

ADR-0042 owns database semantics and threat assumptions. Missing/empty context
must match no tenant, including the valid zero UUID; malformed UUID context raises
and aborts. Explicit nil requests match only nil. Session state cannot be made safe
merely by a release hook: checkout initialization, transaction cleanup, failed
queries, cancellation and uncertain connections all require verified behavior.
Future enforced RLS uses an explicit transaction/repository unit, rather than
holding a connection throughout response streaming or unrelated external work.

FORCE subjects ordinary table owners and their definer functions to policies.
BYPASSRLS requires explicit role attributes and table privileges; `row_security =
off` detects filtering but grants no bypass. Request credentials cannot execute
privileged maintenance or assume its role. Global counts include every reference
class and tenant; filtered, partial or failed counts cannot authorize deletion.
Object GC and orphan reconciliation remain prohibited pending separate proof,
including concurrent reference creation/deletion and the existing blob-lock
protocol. Backup and inventory require complete visibility too.

Application tenant checks, resource ownership and permissions remain mandatory.
Revocation is checked on the next authorization decision; content already
delivered cannot be recalled. Search ACL projections never grant access. SCIM
scope comes from server credential configuration, never caller input. Publication
and unpublication via the visibility operation require owner/Admin; #362 also
documents a remaining Edit-sidecar public-availability limitation, which must not
be confused with authority to create public exposure.

## E. Simplification

Removed: permanent workspace/tenant identity conflation, universal Files policy
engine, pilot dependency on enforced RLS, request-long connection pinning, guessed
connection sufficiency, mandatory maintenance infrastructure before demand, and
one-PR-per-document scheduling.

Retained: explicit tenant predicates, live Application authorization, independently
auditable administrator grants, a stable tenant identity, aggregate compatibility
inventory and a gated database-hardening path. Each supports a current security,
data-safety or operator requirement without distributed infrastructure.

## F. Implementation sequence

The [roadmap](../plans/2026-10-13-phase4-0-institutional-implementation-roadmap.md)
is authoritative for PR grouping, dependencies and minimum pilot delivery:

1. P0: approve pilot decisions, assign owners, pin candidate and agree workspace deferral.
2. P1: explicit identity, administrator grants/bootstrap/recovery and audit foundation.
3. P2: remaining group/Files/Notes evaluator agreement, explanations and refresh parity.
4. P3: bulk lifecycle and administration UI, following P1/P2.
5. P4: complete institutional journey and operator recovery; parallel work may start after P0.
6. P5: exact-candidate Gates A/B/C after P1–P4 and mandatory least-privilege credentials.

Already merged hotfixes are prerequisites satisfied by source review, not work to
reimplement. Identity/admin and residual isolation corrections precede managed
onboarding and collaboration; end-to-end workflow and recovery acceptance precede
pilot release. Least-privilege deployment is on that path. Transaction-bound RLS,
privileged global maintenance and legacy row mapping have separate evidence and
approval gates; optional enforcement cannot delay a pilot meeting the application
isolation and operational contract.

## G. Independent review results

Three independent reviewer agents used product/architecture, security/database
and implementation/operations focuses, then cross-reviewed the revised documents.
The principal reviewer challenged the revised normative text again. Findings
below summarize concrete defects; closure means a corrected design or an explicit
owner/blocking gate, never a claim that future implementation already passed.

| ID | Severity | Finding | Disposition and evidence |
| --- | --- | --- | --- |
| B1 | BLOCKER | Legal nil tenant also used as absent RLS context | ADR-0042 exact NULLIF policy; PG16.15 absent/reset/explicit-nil probes |
| B2 | BLOCKER | Ordinary definer under FORCE may produce deletion-authorizing zero | ADR-0042 dedicated maintenance boundary; PG16.15 ordinary-definer zero vs bypass two vs lost-bypass error; GC stays gated |
| H1 | HIGH | Permanent workspace/organization conflation | ADR-0040 Option B and future API model; #330 bounded-deferral approval blocks Gate A |
| H2 | HIGH | Universal Files resolver violates Application ownership | ADR-0041 D3/D4 and indexing contract supplement retain source authority/Buzz boundary |
| H3 | HIGH | Edit publication and uniform unpublication claims disagree with merged source | ADR-0041 D5; #362 owner/Admin toggle verified; sidecar availability residual expressly blocks pilot without owner disposition |
| H4 | HIGH | Historical security findings presented as current after hotfixes | Dated post-hotfix supplement, current main SHA and archived audit attribution |
| H5 | HIGH | Old normative ADR/containment rules override corrected proposal | ADR-0041 rewritten; containment old sections explicitly non-authoritative history; roadmap replaces old scheduling |
| M1 | MEDIUM | Tenant-scoped naming called a lossless index-only migration | ADR-0040 D7 requires selector/issuer/client/collision/restore contract before change |
| M2 | MEDIUM | Least-privilege role mandatory in one document, optional or absent in gates | ADR-0042, roadmap database step 1 and Gate A role/boot/restore evidence agree |
| M3 | MEDIUM | Audit foundation deferrable despite attributable new mutations | Roadmap P1 and Gate A minimum audit foundation required before exposure |
| M4 | MEDIUM | Obsolete AR/DB decision IDs and blanket all-decisions prerequisite | Roadmap P0 and scheduling crosswalk; historical containment labels archived |
| M5 | MEDIUM | First administrator and recovery insufficiently specified | ADR-0040 D6 authenticated operator bootstrap, grant history and second-admin/recovery rehearsal gate |
| M6 | MEDIUM | Account counts replace institutional workflow acceptance | Roadmap P4/P5 and gates distinguish existing components, incomplete journey and deferred capabilities |
| M7 | MEDIUM | Newer #364/#365 partial coverage risks duplicate implementation or overstated fixes | Source supplement and roadmap current baseline distinguish merged guards from remaining evaluator/non-disclosure work |
| M8 | MEDIUM | Authentication lookup/context initialization circularity omitted | ADR-0042 bounded bootstrap lookup contract and policy-coverage gate |
| M9 | MEDIUM | Duplicate ADR numbers caused ResourceRef status to be called Accepted | ADR-0041 now identifies ResourceRef ADR-0032 as Proposed and unified-search ADR-0036 as Accepted by full identity |
| M10 | MEDIUM | Target non-disclosure presented as uniformly implemented | ADR-0040/0041 distinguish #365 foreign/missing group status residual; normalization or explicit security-owner disposition gates release |
| L1 | LOW | ADR index titles and local references drift | Index synchronized; local-link, anchor and whitespace checks required before commit |

The security reviewer rechecked the exact policy, privilege boundaries and legacy
rules; the implementation reviewer rechecked repository/pool feasibility and gate
dependencies; the product reviewer rechecked workspace scope and real workflow.
Final re-review reported no unaddressed documentation-level BLOCKER/HIGH/MEDIUM.
The consolidated ledger contains 2 BLOCKER, 5 HIGH, 10 MEDIUM and 1 LOW findings;
each has a correction or a specifically owned implementation/approval gate.
Runtime and human gates listed below remain intentionally unfulfilled.

## H. Quality rubric

**91/100, not 10/10.** Design quality is separate from implementation readiness;
a documented future test is not executed evidence.

| Dimension | Score / weight | Evidence and residual gap |
| --- | --- | --- |
| Product alignment | 18 / 20 | Sovereign memory, independent Applications and real journey explicit; Option B needs product-owner scope approval and the full journey needs candidate evidence |
| Security correctness | 18 / 20 | Nil/FORCE defects reproduced and corrected design bounded; sidecar residual, SQLx and all-reference concurrency tests remain gated |
| Architecture consistency | 14 / 15 | One ADR owns each concern; archived history and scheduling crosswalk; accepted-contract owner approvals pending |
| Simplicity and maintainability | 14 / 15 | No universal policy engine, mandatory RLS or pinned-request pool; privileged maintenance still adds justified operational separation |
| Implementation feasibility | 9 / 10 | Current PgPool shape acknowledged; minimal pilot separated; executor/role rollout requires implementation proof |
| Backward compatibility and migration safety | 9 / 10 | Nil preserved, mappings reviewed, aggregate inventory and restore gates; real deployment population not measured |
| Testability and acceptance evidence | 5 / 5 | Exact positive/negative requirements, source vs executed evidence and candidate traceability explicit; future passes not asserted |
| Operational readiness | 4 / 5 | Role/load/restore/admin recovery gates defined; candidate operator rehearsals remain outstanding |

Closing the residual gap requires named owner approval and candidate-bound
bootstrap, privilege, workflow, workload and recovery evidence. Enforced RLS/GC
need their separate SQLx/global-count/concurrency proofs if activation is proposed;
they do not become prerequisites for the application-isolated pilot.

## I. Human decisions

Designated maintainers are repository CODEOWNERS `@senolcolak` and `@zoorpha`.
They must explicitly approve the Proposed ADRs and identify accountable product,
security and deployment owners before dependent implementation begins.

Required decisions: workspace deferral and #330 acceptance scope; first-admin
bootstrap/recovery and installation-operator separation; persistent generated
tenant identity for fresh setups; strict rejection and communication of inconsistent
legacy grants; least-privilege rollout and migration credentials; any future RLS
enforcement; privileged maintenance access and independent global-count proof;
workflow and backup/restore/upgrade acceptance for the actual pilot. Postponing a
decision blocks its dependent capability or rollout, not unrelated documentation.
No issue is closed, ADR Accepted, legacy row remapped or GC enabled by this report.

Existing narrow human approvals remain recorded on PR #360: ADR-0040 original
items 1/5 authorized AR-01/#364; ADR-0041 original items 2/3 unblocked AR-03/#365.
Those approvals preserve naming and roster/membership directions within their
stated scope; they do not approve revised workspace evolution, remaining
authorization changes or the whole package. The corresponding ADRs link the
original decision comments explicitly.

## J. GitHub and validation

Corrections are documentation-only. Final commit and live PR check status are
reported through the existing PR. Local validation passed: `git diff --check`;
19 changed/new Markdown files only; 69 relative links and 3 local anchors resolve;
balanced code fences; ADR-0040–0043 remain Proposed; original executed-report and
evidence-matrix bodies retain every original line with additive supplement links.
The five baseline audits were not edited. Disposable PostgreSQL 16.15 semantics
probes passed as reported in the verification supplement; no existing database
was modified. No dedicated Markdown lint job was found in repository workflows.
Full Rust/frontend baselines are
not substitutes for architecture approval and are unnecessary for prose-only
edits; production integration tests specified by the ADRs remain release gates.

The complete PR retains its original `scripts/phase40-tenant-isolation-negative-tests.sql`
evidence harness unchanged by this correction. Thus the correction is Markdown-only;
the full PR additionally contains that historical synthetic SQL evidence artifact.
No production Rust, migration, schema or runtime configuration differs from main.

Publication outcome: the existing branch and PR #360 were updated, with signed
correction commit `86675e0a` and main reconciliation `39d0d02d`; later signed review
clarifications preserve the original narrow approvals. The final immutable head
SHA is supplied with this report and is visible on PR #360. At the first live
check after publication, DCO passed; Terminology & Trademark Scan and Plugin Unit
Tests were in progress. No all-green result is claimed. The `gh pr edit` command
hit GitHub's retired classic-projects GraphQL field; the REST pull-request update
succeeded. The PR remains OPEN, unmerged, with Proposed ADRs.

Changed files in the correction:

- `CHANGELOG.md`, `README.md`.
- `docs/adr/0038-institutional-organization-and-workspace-access.md` (explicit proposed amendment).
- `docs/adr/0040-organization-tenant-and-workspace-identity.md`.
- `docs/adr/0041-groups-and-authorization.md`.
- `docs/adr/0042-postgresql-tenant-isolation-rls-staging.md`.
- `docs/adr/0043-legacy-nil-tenant-compatibility.md`, `docs/adr/README.md`.
- `docs/architecture/elembra-platform.md` (labelled proposed interpretation).
- `docs/audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md` (supplement link only).
- `docs/audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md` (supplement link only).
- `docs/audits/2026-10-15-phase4-0-post-hotfix-verification.md`.
- `docs/audits/2026-10-15-phase4-0-postgresql-design-verification.md`.
- `docs/audits/2026-10-15-pr360-architecture-final-review.md` (this report).
- `docs/contracts/permission-aware-indexing-contract.md` (proposed amendment only).
- `docs/migrations/2026-10-13-phase4-0-nil-tenant-inventory-plan.md` (dated role/visibility correction).
- `docs/pilot/2026-10-13-institutional-acceptance-gates.md`.
- `docs/plans/2026-10-13-phase4-0-institutional-implementation-roadmap.md`.
- `docs/plans/2026-10-14-phase4-0s-decision-record.md` (current gates plus archived history).
