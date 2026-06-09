# ADR-0030: Auto-Generated OpenAPI Specification for the RustShare REST API

## Status

Accepted.

## Context

RustShare exposes a growing REST API used by the web client, mobile clients, and third-party integrations such as the RustChat AI agent. Today, external consumers have no authoritative, machine-readable contract for the main REST surface. The Vault Sync API already ships a hand-written OpenAPI YAML (`docs/contracts/vault-sync-api-openapi.yaml`), but the main `/api/v1/...` surface is undocumented, forcing integrators to read handler source code.

Maintaining a hand-written OpenAPI document is expensive and drifts out of sync quickly. We need a single source of truth that:

1. Is generated automatically from the code that actually runs in production.
2. Requires no manual triggers to update.
3. Is reviewable in pull requests as a committed artifact.
4. Is served at runtime for discovery and interactive exploration.
5. Covers real endpoint paths *and* webhook/event payload schemas so that RustChat and other consumers can rely on them.

## Decision

Use [`utoipa`](https://github.com/juhaku/utoipa) to generate the OpenAPI 3.1 spec at compile time directly from Axum handler functions and request/response types.

Key choices:

- **Compile-time generation** via `#[derive(utoipa::ToSchema)]` and `#[utoipa::path(...)]` so the spec is never out of sync with the implementation.
- **Runtime serving** via `utoipa-swagger-ui` at `/api/docs` with the raw JSON at `/api/docs/openapi.json`.
- **Committed artifact** at `docs/contracts/rustshare-api-openapi.json` that is refreshed by an integration test (`backend/tests/openapi_export_test.rs`). The test fails CI if the committed file diverges from the generated spec.
- **No manual triggers**: updating the spec is a side effect of `cargo test` when `RUSTSHARE_UPDATE_OPENAPI=1` is set; otherwise the test enforces freshness.
- **Webhook payload schemas** are included as `components/schemas` from `rustshare_core::events::*` so external consumers can validate inbound webhook bodies without running the server.

### Scope of annotated handlers (Phase 1)

Representative handlers across all major API surfaces are annotated:

- Admin webhooks (`/api/v1/admin/integrations/webhooks/*`)
- Chat / RustChat integration (`/api/v1/integrations/chat/*`, `/api/v1/admin/integrations/chat/webhooks`)
- Files, folders, notes, AI, auth, public shares

New handlers should be added to `backend/server/src/openapi.rs` as they are implemented.

### Handling recursive types

Domain types with self-referential schema trees (e.g., `FolderTree`, `FolderTreeWithShares`) use `#[schema(no_recursion)]` on the recursive field so utoipa terminates schema collection instead of overflowing the stack.

## Consequences

### Positive

- A single `cargo test` command can verify that the committed OpenAPI contract matches the code.
- RustChat and other integrators receive a real, versioned JSON contract they can import into code generators.
- Swagger UI is available in every deployment for interactive exploration.
- Webhook payload schemas are documented alongside REST endpoints.

### Negative / Mitigations

- Adding `utoipa::ToSchema` to domain types increases compile-time work slightly. Mitigated by only deriving it on types that cross the HTTP boundary.
- Recursive domain types require explicit `no_recursion` annotations. Future recursive types must follow the same pattern.
- The committed JSON is large (~150 KB). It must be regenerated and committed alongside handler/schema changes.

## Acceptance Criteria

```text
- `cargo check -p rustshare-server` succeeds.
- Swagger UI renders at /api/docs.
- /api/docs/openapi.json returns the generated OpenAPI 3.1 JSON.
- docs/contracts/rustshare-api-openapi.json is committed and kept fresh by
  backend/tests/openapi_export_test.rs.
- The test fails CI when the spec is stale and passes after
  RUSTSHARE_UPDATE_OPENAPI=1 cargo test --test openapi_export_test -p rustshare-server.
```
