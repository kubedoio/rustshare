# Migration Plan: RustShare to Elembra Application Architecture

Status: Proposed  
Date: 2026-08-07

## Principle

This is a **one-time pre-release architecture cutover**, not a compatibility-preservation exercise.

Preserve:

- user files/content;
- versions and shares where semantically valid;
- mail data/accounts/state;
- vault/sync data;
- tenant/user/workspace identity data;
- audit/provenance needed for user trust;
- existing secure behavior and regression coverage.

Do not preserve indefinitely:

- `Module` terminology/type solely because it already exists;
- legacy JSON field aliases;
- `/modules/...` user-facing routes;
- obsolete internal service graph boundaries;
- a public contract that has not been released/stabilized;
- duplicate compatibility registries/adapters after migration.

## Preconditions

Before implementation starts:

- ADR-0030 through ADR-0034 accepted or amended;
- Application Manifest, Integration Event and Connector specs reviewed;
- issue #196 rewritten to match the canonical architecture;
- current v0.7 stabilization work kept separate from architecture migration;
- database backup/restore procedure tested on representative current data.

## Stage 1 — Shared contract primitives

Introduce a small shared contract package/crate containing no Application business logic:

```text
ApplicationId
ApplicationManifest / Contribution types
TenantId
WorkspaceId
PrincipalId / PrincipalContext
ActionCapability
ResourceRef
CorrelationId / CausationId
IntegrationEvent envelope
```

Do not add a generic service locator.

## Stage 2 — Application registry

Create an Application registry responsible for:

- loading/validating first-party manifests;
- reporting available Applications;
- tenant/workspace enablement/configuration state;
- resolving an Application owner/adapter/endpoint;
- composing Contributions for the Shell;
- health/degraded state for service/bridge runtimes.

Initial first-party Applications remain compiled/known at build time. Registry metadata is not a package installer.

## Stage 3 — Module → Application database cutover

Create a migration that maps the current `modules` state into the target Application enablement/configuration model.

Recommended target tables (exact naming may be adjusted during implementation review):

```text
applications
  application_id       # stable first-party identity; definition/install-level metadata if persisted
  version
  runtime_kind
  created_at
  updated_at

workspace_applications
  tenant_id
  workspace_id
  application_id
  enabled
  config_json
  created_at
  updated_at
  primary key (...)
```

If every first-party manifest is code-owned, the `applications` definition table may be unnecessary; only configuration/enablement state must persist. Do not duplicate manifest source-of-truth into SQL without a concrete operational reason.

Migration maps existing settings such as enabled state and configuration needed by the corresponding Application.

## Stage 4 — Remove Module API compatibility

After migrated state has been validated:

- rename/remove `Module` domain model;
- remove `ModuleService` in favor of Application registry/configuration services;
- remove legacy serde aliases such as `key`, `displayName`, `rootPath`, `defaultTemplate`, `schemaVersion`, `aiIndexing`, `ui` from the new Application model;
- remove tests whose only purpose is legacy Module JSON deserialization;
- replace admin/module endpoints with Application equivalents;
- replace `/modules/:moduleKey` shell routes with `/apps/<app>/...` contributions;
- update frontend types/navigation/tests in the same cutover series.

Do not leave a permanent translator layer.

## Stage 5 — Keep file-backed content semantics where they are correct

ADR-0016 contains valuable data-design rules for file-backed knowledge:

```text
Path = human organization
Metadata = machine state
Event history = historical truth
Index = projection
Renderer = UI projection
```

Retain these principles for Notes/Decisions/Meetings/Standups/Kanban where they fit.

What changes is the platform boundary: a file-backed template feature is no longer the definition of all Elembra modularity.

Update ADR-0016 status to record partial supersession by ADR-0030.

## Stage 6 — Reduce `AppState` coupling

Do not replace `AppState` with a runtime map of `dyn Any`/generic services.

Instead:

- handlers extract the smallest typed state they need;
- Application handlers depend on owned services + explicit shared contracts;
- Files exposes contract traits/adapters for artifact/resource operations;
- authorization context is passed explicitly;
- direct construction/wiring may remain compile-time for embedded Applications.

Target smell to remove:

```text
feature handler -> broad ServiceState -> arbitrary unrelated services
```

Target:

```text
feature handler -> App-specific state
                -> explicit required contract(s)
```

## Stage 7 — Durable integration foundation

Before any important cross-process extraction:

- implement transactional outbox;
- implement integration envelope;
- implement consumer receipt/idempotency;
- implement retry/lease/DLQ metrics;
- explicitly restrict `EventBroadcaster` to ephemeral live UI notifications.

Migrate only selected cross-Application events. Do not mechanically export every internal domain event.

## Stage 8 — Prove one service/Connector boundary

Do not begin with Mail; it has recently stabilized and combines difficult IMAP/SMTP/sync state.

Preferred first proof:

1. Memory indexing worker, or
2. Shell/Local Text Connector worker.

The proof must demonstrate:

- workload auth;
- PrincipalContext propagation;
- ResourceRef use;
- durable events;
- retry/idempotency;
- health/degraded state;
- independent restart;
- no private-table access.

## Stage 9 — Elembra Memory migration

Refactor the current AI/indexing implementation into the ADR-0033 boundaries:

```text
Memory Catalog
Index/Search
Retrieval/RAG
```

Preserve current permission-aware indexing safety while adding source reauthorization/batch authorization before LLM materialization.

Existing note index data is rebuildable and may be migrated or rebuilt based on cost/simplicity. Do not retain an old index schema solely for compatibility if a clean rebuild is safer.

## Stage 10 — Elembra Chat / RustChat transition

Before merging chat backends:

- inventory RustChat features/data/client behaviors worth retaining;
- decide explicit migration/export path for any real RustChat user data;
- create/prepare `elembra-chat` target repository strategy;
- implement Principal↔Buzz pubkey and Workspace↔Buzz community contracts;
- design OIDC pairing/recovery without replacing Buzz signatures;
- build Files ResourceRef/unfurl integration;
- build Memory/search projection;
- keep Buzz delta minimal and upstream-trackable.

Do not maintain RustChat and Buzz as parallel authoritative chat backends indefinitely.

## Stage 11 — Rename repository/product

Perform the public RustShare→Elembra naming cutover only after the architecture branch establishes a buildable migration path.

Update together:

- repository name where desired;
- package/crate names in a planned series;
- Docker/container/environment names;
- user-facing branding/routes;
- docs/URLs;
- release process.

Avoid mixing mechanical global rename with the first contract implementation PR unless tooling/test coverage makes the diff reviewable.

## Database safety

Every destructive/renaming migration must be tested against:

- empty database;
- representative current database;
- database with Applications/modules disabled/enabled differently;
- rollback/restore from backup;
- restart after migration;
- tenant isolation assertions.

No migration may silently delete user content because an Application is disabled or renamed.

## API compatibility policy

Until Application contracts become stable:

- label them `v1alpha1` internally/docs;
- coordinate first-party clients in the same migration series;
- breaking changes are allowed;
- do not maintain version shims by default;
- do not claim third-party compatibility.

Once a specific API/event is declared stable, normal versioning/compatibility rules begin for that contract only.

## PR strategy

Do not land the entire architecture migration as one giant code PR.

Recommended sequence:

1. architecture/docs only;
2. shared contract primitives + registry skeleton;
3. database Module→Application migration + backend API;
4. frontend Shell/route Contribution migration;
5. remove legacy Module compatibility;
6. ResourceRef + Files authorization contracts;
7. transactional outbox/event envelope;
8. first external worker/Connector proof;
9. Memory refactor;
10. Chat/Buzz integration series.

Each PR must leave `main` buildable/testable.

## Completion criteria

The architectural cutover foundation is complete when:

- no product-level `Module` API/terminology remains where `Application` is intended;
- Application registry/manifests compose current first-party UI;
- existing user data survives migration;
- handlers no longer require broad state merely to reach unrelated services;
- ResourceRef/PrincipalContext contracts are in use;
- durable integration outbox is available before service extraction;
- at least one external worker/Connector proves the boundary;
- no permanent backward-compatibility shim remains solely for pre-release RustShare internals.
