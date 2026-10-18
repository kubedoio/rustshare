# ADR-0040: Organization, Tenant and Workspace Identity

Status: Accepted — architecture approved by the project owner on 2026-10-15. Implementation and rollout remain subject to documented evidence and operational gates.
Date: 2026-10-13; correction: 2026-10-15
Historical baseline: `1ec2ea7a887ee0ab79d7d8ce525a0e5a76e0109e`. Current reconciliation includes merged PRs [#361](https://github.com/kubedoio/rustshare/pull/361), [#362](https://github.com/kubedoio/rustshare/pull/362), [#364](https://github.com/kubedoio/rustshare/pull/364) and [#365](https://github.com/kubedoio/rustshare/pull/365).

## Problem, facts and product requirement

The [historical evidence matrix](../audits/2026-10-13-phase4-0-tenant-isolation-evidence-matrix.md) and [negative-test report](../audits/2026-10-13-phase4-0-tenant-isolation-negative-tests.md) describe their named revision, not current findings. #361 subsequently bound SCIM to operator-configured tenant scope; #362 corrected note publication authority (ADR-0041). Neither implements organization administration or workspaces.

Users/resources/groups already carry `tenant_id`. No workspace table or membership model exists. `File::workspace_id()` returns `tenant_id`; migration `20260723000000_note_index_acl_columns.up.sql` backfills the index alias. Ask/search consume this compatibility representation. Existing global `users.is_admin`, global username/group-name/external-ID constraints and nil bootstrap default are implementation facts, not permanent product contracts.

Elembra is a Sovereign Business Memory Platform with a small Platform Core and independently owned Applications. Approximately 100 users is an adoption milestone, not a scale limit. Organization administration and collaboration must remain understandable without developer/database intervention.

## Selected model and alternatives

Post-baseline correction: #364 explicitly scopes principal/group creation and credential consistency; #365 validates membership writes, scopes member rosters and validates recipient-group/resource tenant equality. Neither adds organization administrators or workspaces. #365 does not change legacy resolver expansion, field narrowing or all admin/share-management surfaces; its known 403/404 group-existence distinction is a remaining non-disclosure gap, not a completed guarantee. Historical approvals of organization=tenant and global naming remain evidence of those original scopes; they do not approve revised workspace Option B.

| Concept | Meaning |
|---|---|
| Tenant | Security and ownership boundary |
| Organization | Human organization represented by one tenant in this milestone; same ID |
| Workspace | Independently identified collaboration context within a tenant; implementation deferred |
| Group | Tenant-bound principal set usable as a grant recipient; neither tenant nor workspace |

```text
Organization = Tenant
  ├─ Users (one organization per current account)
  ├─ Groups ── membership ── Users
  └─ Workspaces [future, 0..many, independent IDs + tenant_id]
       ├─ direct/group memberships [future]
       └─ ResourceRefs → owning Application → resource authorization
```

**D1 — Organization = Tenant.** Retain one identifier and ownership boundary. No second organization identity or mapping lifecycle. Future display name/settings records can use the same tenant primary key without changing ownership or requiring a join in every authorization query. Instance configuration suffices now; metadata alone is not grounds for another boundary.

**D2 — Option B: defer workspaces, retain compatibility only.** No independent workspace membership, role, membership API or misleading UI ships this milestone. Existing `workspace_id == tenant_id` fields retain only their documented compatibility meaning. Ask must reject an alias differing from authenticated tenant; index writers derive the alias from the resource tenant. No production ACL writer may emit a `workspace:<id>` content grant. This is not permanent identity equality and does not complete #330's workspace criteria.

Option A (minimal workspaces now) has product value: #330 explicitly requests multiple workspaces and membership administration. It nevertheless adds identity/membership APIs, Calendar semantics, UI and backfill decisions before pilot without an established runtime model. Defer those costs only with the explicit bounded scope approval below. Permanent workspace=tenant is rejected as incompatible with collaboration evolution.

**D3 — Evolution contract.** Revisit before accepting a need for distinct project/team contexts, separate memberships, multiple workspaces per tenant or Calendar sharing. A future ADR introduces independent IDs, tenant-validated memberships and resource association semantics. Keep resource tenant ownership unchanged. Version/add API fields or routes distinguishing true workspace identity from the old tenant alias; never silently reinterpret persisted IDs or request fields. A bounded compatibility adapter retains old tenant scope and does not grant access to every future workspace. Inventory ask/index/enablement/Calendar/Chat consumers, approve mappings, rebuild derived index state, test old/new clients and announce retirement before migration. Do not create a permanent default-workspace ID now. Cardinality and mapping are future decisions, not invented backfills.

**D4 — Single-organization user accounts.** `users.tenant_id` remains membership source. Future cross-organization human identity/memberships need their own authentication and provenance design. Account transfer is not routine membership editing. Every HTTP/WebSocket/session/device path derives current scope from validated credentials/current user and rejects tenant drift. Headers, bodies and token hints cannot choose organization. Membership/context is shared; resource-owning Applications remain authoritative.

**D5 — Explicit provisioning and fresh identity.** Recommend generating a non-nil default tenant UUID for fresh deployment, persisting it in configuration and backup; never regenerate on restart. This needs implementation and is not current bootstrap behavior. Preserve coherent existing nil deployments. Nil is a legal historical tenant, not absence, wildcard or implicit column-default permission. Every writer explicitly resolves tenant or rejects. ADR-0043 owns read-only inventories and reviewed mappings; no ownership inference or automatic nil rewrite is authorized.

**D6 — Administration and bootstrap.** Keep installation operator (`is_admin`) distinct from explicit tenant-scoped organization administrator. Recommend additive `organization_administrators(tenant_id,user_id,granted_by,granted_at,revoked_at)` with active tenant/user grant and same-tenant validation. Record grant/revoke/regrant history in append-only administrative audit; overwriting grant state is not sufficient history. Resolve grants live. Only authenticated installation operators grant/revoke this milestone; no automatic grants to old operators. Organization administration permits bounded lifecycle/group/audit operations, never private content, impersonation, installation control or another tenant.

Bootstrap uses authenticated operator command/API to select/create a tenant member and explicitly grant administration; no public bootstrap endpoint. Before pilot, rehearse a second independently controlled admin and recovery using documented offline operator credentials. Recovery verifies tenant and records actor/reason/outcome without rewriting content ownership. Security owner approves recovery and last-admin handling before exposure. Mutable ACL groups are rejected as administrator authority.

**D7 — SCIM and naming.** Retain #361's operator-bound credential scope; API callers cannot choose it. A leaked scoped credential compromises its own organization and requires rotation/revocation. Independently scoped future IdPs require integration/tenant/issuer binding, not caller-selected scope; defer implementation. Keep and disclose existing global usernames, group names and external IDs for this milestone. These restrictions are temporary: tenant-scoped naming needs schema, login selector, lookup/API, issuer and collision/restore/client migration design. Dropping a unique index alone is not a demonstrated lossless migration.

## Security and Application contracts

Platform Core owns principal, tenant, group and global Application-grant context. Files/Notes may use PermissionResolver; other Applications own their evaluators. ResourceRef/source reauthorization, Buzz's independent signed communication and accepted unified-search ADR-0036 remain mandatory. Tenant/group/future workspace membership, search hits and general Agent execution permission grant no content access. ADR-0041 owns sharing/publication/revocation; ADR-0042 owns database context and global GC; ADR-0043 owns legacy mapping.

Disable revokes server sessions/device tokens through `20261003090000_revoke_credentials_on_user_disable.sql`. The #331 offboarding slice adds `credentials_revoked_at` in `20261018090000_revoke_stale_jwt_on_user_disable.sql`: it backfills existing principals, advances on every disable, and remains set through reactivation. HTTP and user-WebSocket JWT validation reject tokens issued at or before that marker; reactivation requires fresh authentication. The migration intentionally invalidates pre-migration JWTs once. Share-viewer WebSockets recheck the active share and both share/session expiry before each live event. The disposable PostgreSQL integration covers stale JWT rejection after reactivation, session/device-token revocation, and share-session invalidation. No unchecked 24-hour residual JWT window remains.

## Schema, API, operations, performance and rollback

No workspace or second organization schema is introduced. Admin grants are additive; scoped audit storage belongs to #332. Scoped creation uses existing tenant columns. Inventory before stricter predicates; inconsistent legacy records become unavailable with non-disclosing errors and an operator review route, never fallback grants. Preserve reviewed mapping and restore evidence under ADR-0043.

No distributed service or request-long connection pinning is required. Indexed live tenant/user grant checks, pagination and bounded bulk operations suffice; acceptance gates require measured concurrency. Persist fresh identity/admin configuration through backup/restore. Additive grant schema may remain inert on rollback; never recover access by reverting security predicates. Review downgrade compatibility for writes/grants/audit and rehearse restore first.

## Required tests and acceptance

These are requirements, not executed evidence:

1. Explicit same-tenant nil/non-nil provisioning succeeds; absent scope writes nothing; generated fresh identity survives restart/restore; existing nil deployment remains coherent.
2. Scoped SCIM cannot list/count/get/change another tenant or override credential scope (#361 regressions).
3. Tenant-mismatched groups/resources/principals deny without disclosure; organization administrators cannot read private content or another tenant's audit.
4. Bootstrap/grant/revoke/regrant/last-admin/recovery are attributable; next request after revocation denies; self-escalation denies.
5. Credential drift/disable/re-enable behavior is measured across transports; residual JWT behavior explicitly gates readiness.
6. Legacy ask alias accepts only authenticated tenant; workspace-grant attempts deny; UI/API claims no independent workspace membership.
7. Bulk naming conflicts are predictable; backup/restore retains tenant and admin scope. Future workspaces require explicit old/new client compatibility tests.

Acceptance requires candidate-revision artifacts, operator recovery rehearsal and named human approvals. Optional enforcement RLS is not a pilot prerequisite.

## Amendment, deferrals and human decisions

Historical approval is preserved: the [project-owner decision of 2026-10-14](https://github.com/kubedoio/rustshare/pull/360#issuecomment-6174911062)
approved original items 1 and 5 solely for AR-01/#364: organization=tenant,
workspace=organization (1:1) for Phase 4.0/4.1, no additional identity tables at
that stage, and global username/external-ID uniqueness. It did
not approve permanent workspace/organization equivalence or these revised Option B
evolution contracts. It authorized neither the remaining ADR package nor RLS,
legacy migration, GC or deployment. That narrow approval is not revoked by this revision.

ADR-0038 remains Proposed: preserve distinct-workspace intent, defer implementation under D2, and retain administration/content/revocation invariants. Accepted ADR-0031 and unified-search ADR-0036 retain history and behavior; no silent supersession is proposed.

The project owner's 2026-10-15 instruction to mark the revised architecture Accepted approves Option B and the bounded #330/#331/#333 milestone deferral. #330 remains open for future workspace membership requirements; issue status is not changed by architecture acceptance. ADR-0040–0043 are Accepted. Security owner still validates implemented grant/recovery/revocation behavior; operator validates fresh identity and legacy impact before rollout. CI is evidence, not the source of this acceptance.

See [roadmap](../plans/2026-10-13-phase4-0-institutional-implementation-roadmap.md) and [acceptance gates](../pilot/2026-10-13-institutional-acceptance-gates.md) for implementation/test traceability.

Accountable maintainers are CODEOWNERS @senolcolak and @zoorpha. They must name the product owner, security owner and deployment operator before Gate A; absent designation/approval blocks the affected gate. Role labels above do not imply an unnamed approval has happened.
