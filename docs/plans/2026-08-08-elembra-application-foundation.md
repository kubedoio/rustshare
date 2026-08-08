# Elembra Application foundation — #210 implementation note

## Scope

- Add declarative `elembra.io/v1alpha1` Application contracts and validation to
  the shared core domain.
- Keep first-party manifests code-owned; persist only tenant/workspace
  enablement, health, and configuration.
- Migrate current tenant-level Application enablement and user preferences into
  Application-shaped records. The current schema has no authoritative
  workspace table, so the migration uses `tenant_id` as the initial workspace
  identity. User dashboard preference keys are migrated to
  `enabled_applications`/`application_order`.

## Deliberately deferred

The HTTP/frontend cutover is included: product routes use `/apps/...`, registry
routes use `/api/v1/applications/...`, and the old Module service/types/routes
are removed. Principal authorization/ResourceRef behavior remains in #211;
durable events remain in #212; Connectors remain in #213. No dynamic loader,
package manager, WASM, service extraction, or generic service locator is
introduced here.

## Safety invariant

The migration changes only Application/template references and configuration
JSON. It does not update or delete file bytes, folders, shares, mail, or other
durable content. Re-running the tested cutover path is safe; migration tests
cover a clean database, a representative legacy database, and replay of the
final cutover.
