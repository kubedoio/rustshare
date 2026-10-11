# ADR-0038: Institutional Organization and Workspace Access

Status: Proposed
Date: 2026-10-04

## Context

Issue [#330](https://github.com/KubedoIO/RustShare/issues/330) requires
organization groups, roles, and workspace membership while preserving
resource-level authorization. Issue [#328](https://github.com/KubedoIO/RustShare/issues/328)
also requires calendar events to relate to workspaces and meeting resources.
The Issue #333 implementation plan makes these decisions prerequisites for
schema or authorization changes.

The repository currently has tenants and tenant IDs on users and resources,
but no distinct organization-role or workspace-membership model. Existing
user groups are not a safe substitute:

- `user_groups.tenant_id` was added with a zero UUID default by
  `20260329170001_add_tenant_id_to_tables.sql`.
- Group names remain globally unique, while `group_members` has no tenant
  key (`20260322000002_create_user_groups.sql`,
  `20260322000003_create_user_group_members.sql`).
- Resource group-share resolution is application authorization behavior and
  must continue to be preserved (`backend/crates/infrastructure/src/repositories/permission_resolver.rs`).
- Group administration uses the global `AdminUser` guard and group-ID-only
  lookup paths (`backend/server/src/handlers/admin/groups.rs`).
- The existing `is_admin` flag is installation-wide, not an
  organization-scoped role (`backend/server/src/handlers/extractors.rs`).
- Calendar is PostgreSQL-backed and owner-scoped; its spec defers workspace
  sharing pending an authorization design
  (`docs/adr/0037-calendar-application-and-external-sync.md`,
  `docs/specs/calendar-application-v1alpha1.md`).

The application ownership and ResourceRef rules in
[`0030-elembra-application-model.md`](0030-elembra-application-model.md) and
[`0032-resource-refs-and-authorization.md`](0032-resource-refs-and-authorization.md)
are themselves marked Proposed. Their status and applicability must be
confirmed before relying on them as accepted policy.

## Decision (proposed for review)

1. **A tenant is the institutional security boundary.** For the institutional
   pilot, one tenant represents one organization. Do not add a second
   organization identity or silently reinterpret existing `tenant_id` values.
2. **A workspace is a distinct collaboration scope inside one tenant.** A
   workspace and every direct/group membership must be tenant-bound. Reject a
   membership if the principal, group, or workspace does not belong to the
   same tenant. Workspace existence and membership must be explicit; an
   `application_enablements.workspace_id` value alone is not membership.
3. **Keep installation administration distinct from organization
   administration.** The existing global `is_admin` continues to mean
   installation operator. Add explicit tenant-scoped organization roles with
   only `administrator` and `member` initially. An organization administrator
   may manage that tenant's users, groups, workspaces, and memberships but
   receives no implicit access to private resources.
4. **Use tenant-scoped organization groups for new administration.** Do not
   broaden or rewrite legacy resource-ACL groups in place until an inventory
   proves a lossless mapping. Preserve current ACL behavior. A future migration
   may map a legacy group only when all memberships and ACL references have an
   unambiguous tenant mapping; mixed, orphaned, or ambiguous groups require
   explicit resolution and must not be auto-assigned. Group membership and
   workspace membership remain distinct concepts.
5. **Workspace membership is an access-administration input, not a universal
   content grant.** It controls discovery/eligibility and workspace-level
   administration only. Each resource-owning application continues to
   authorize reads and writes using its own grants. Cross-application links
   are references, not grants. Confirm the proposed ADR-0030/0032 ownership
   boundary before implementing cross-application workflows.
6. **Direct and group-derived membership are additive and explainable.** The
   API/UI reports each current grant path and its role. Removing one path
   removes only that grant; access remains if another valid path or
   resource-level grant still applies. Authorization is recalculated at
   access time; cached membership does not outlive revocation.
7. **Do not change Calendar sharing in this ADR.** Preserve per-user calendar
   ownership and per-user read-only external sync. Workspace-shared calendars,
   delegation, or shared provider identities require a separate accepted
   decision and application-owner authorization design. Keep PostgreSQL as
   canonical event storage unless a reviewed requirement establishes a safe
   migration; clarify whether #327's date-structured JSON refers to meeting
   artifacts or canonical events.

## Existing-data and migration constraints

- No migration may infer an institutional tenant for existing users,
  resources, groups, shares, or audit rows from a zero UUID or a display name.
- Before migration, produce a read-only inventory of tenant IDs, group
  memberships, resource ACL references, workspace-like identifiers, and
  orphaned rows. Preserve an export/snapshot and rehearse restore.
- New constraints and tables must be additive first. Backfill only mappings
  proven unambiguous; fail the migration on unresolved references rather than
  dropping, merging, or granting access.
- Test cross-tenant create/list/read/update/delete denial, direct and
  group-derived access, overlapping grant paths, and immediate revocation
  against PostgreSQL before enabling the new administration surface.
- Any conversion of existing groups or changes to `is_admin`, session
  claims, or application authorization requires human security review before
  merge.

## Alternatives considered

- **Treat global groups as organization groups immediately:** rejected; tenant
  membership and handler scoping are not established, and current ACL behavior
  could change.
- **Make organization administrators content superusers:** rejected; it
  conflates administration with user access and conflicts with #330's
  explicit boundary.
- **Treat workspace membership as an automatic grant to every resource:**
  rejected; it would create a second authorization authority and bypass
  resource-owner policy.
- **Add a separate Organization table now:** not recommended for the pilot
  unless a concrete requirement shows tenant identity cannot serve as the
  institutional boundary; it would create parallel lifecycle and mapping
  state.

## Consequences if accepted

- New workspace and organization-membership schema must be tenant-bound and
  covered by database-backed isolation tests.
- Existing global groups and installation admins remain compatibility
  concerns until their mapping and migration behavior are explicitly proven.
- Calendar and other owner-scoped resources remain private unless their owning
  application adds an explicit sharing contract.

## Approval required

This ADR is a proposal, not implementation authorization. Before Phase 0 can
exit, the product/security owner must approve or amend the tenant mapping,
legacy-group treatment, role semantics, additive grant behavior, and Calendar
boundary. No schema or authorization migration should proceed while these
choices are unresolved.
