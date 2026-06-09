# SPEC-00X: Auto-Generated OpenAPI Specification for RustChat and External Integrations

## Purpose

Define how the RustShare REST API OpenAPI specification is produced, served, and kept fresh so that external consumers—starting with the RustChat AI agent—have a reliable, machine-readable contract for real endpoint paths and webhook payload schemas.

## Scope

- Main REST API surface under `/api/v1/...`
- Admin webhook management endpoints
- Chat / RustChat integration endpoints (link unfurl, webhook registration, event dispatch)
- Webhook and event payload schemas defined in `rustshare_core::events`

Out of scope for this spec:

- Vault Sync API (already covered by `docs/contracts/vault-sync-api-openapi.yaml`)
- WebSocket real-time events (documented separately)

## Generated Artifact

- **File**: `docs/contracts/rustshare-api-openapi.json`
- **Format**: OpenAPI 3.1 (JSON)
- **Generator**: `utoipa` v5 with `uuid` and `chrono` features
- **Serving**:
  - Interactive docs: `GET /api/docs`
  - Raw spec: `GET /api/docs/openapi.json`

## Generation Mechanism

### 1. Handler Annotations

HTTP handlers use `#[utoipa::path(...)]` to declare their HTTP method, path, tags, parameters, request body, and response schemas. Example:

```rust
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}",
    tag = "Files",
    params(("id" = Uuid, Path, description = "File ID")),
    responses(
        (status = 200, description = "File metadata", body = File),
        (status = 404, description = "File not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_file(...) -> Result<Json<File>, AppError> { ... }
```

### 2. Schema Derives

Request, response, and domain types implement `utoipa::ToSchema`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WebhookResponse { ... }
```

Type aliases such as `FileId`, `FolderId`, `UserId`, and `ShareId` (all `Uuid`) are rendered correctly using `#[schema(value_type = Uuid)]` on the field that uses the alias.

### 3. Recursive Types

Self-referential types such as `FolderTree` and `FolderTreeWithShares` use `#[schema(no_recursion)]` on the recursive field to prevent infinite schema expansion:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FolderTree {
    pub folder: Folder,
    #[schema(no_recursion)]
    pub subfolders: Vec<FolderTree>,
    pub files: Vec<File>,
}
```

### 4. Central OpenAPI Module

`backend/server/src/openapi.rs` declares the `ApiDoc` struct with the `#[derive(OpenApi)]` macro. It lists all annotated handlers in `paths(...)` and all reusable schemas in `components(schemas(...))`.

The module also exposes:

- `export_to_file(path)` — write the pretty-printed JSON spec to disk.
- `to_pretty_json()` — return the spec as a `String`.
- `openapi_json_handler()` — Axum handler for `/api/docs/openapi.json`.

## Freshness Enforcement

`backend/tests/openapi_export_test.rs` regenerates the spec on every run and compares it with the committed `docs/contracts/rustshare-api-openapi.json`.

- Normal run: fails if the committed file is stale.
- Update run: overwrites the committed file when `RUSTSHARE_UPDATE_OPENAPI=1` is set.

CI should run the normal mode; contributors run update mode after changing annotated handlers or schemas.

## Webhook Payload Schemas

Webhook consumers such as RustChat need to parse outbound event payloads. The following event payload schemas are registered as reusable components in the OpenAPI spec:

- `FileUploadedPayload`
- `FileModifiedPayload`
- `FileDeletedPayload`
- `FileRestoredPayload`
- `FolderCreatedPayload`
- `FolderRenamedPayload`
- `FolderMovedPayload`
- `FolderDeletedPayload`
- `ShareCreatedPayload`
- `ShareRevokedPayload`
- `ShareUpdatedPayload`
- `ShareReceivedByUserPayload`
- `SharePermissionChangedPayload`
- `ShareRevokedFromUserPayload`
- `NotificationCreatedPayload`
- `ReplicationStateChangedPayload`
- `BrainstormBoardModifiedPayload`
- `MeetingNoteModifiedPayload`
- `DecisionModifiedPayload`
- `StandupModifiedPayload`
- `KanbanModifiedPayload`
- `NoteModifiedPayload`

Inbound chat integration events are documented via `IncomingChatEvent` and `ReceiveChatEventRequest`.

## Chat Integration Endpoints

The spec documents the following chat-related endpoints, which are the primary integration surface for RustChat:

```text
POST /api/v1/integrations/chat/unfurl
POST /api/v1/integrations/chat/unfurl/public
POST /api/v1/integrations/chat/events
POST /api/v1/integrations/webhooks/dispatch
POST /api/v1/admin/integrations/chat/webhooks
GET  /api/v1/admin/integrations/chat/webhooks
```

## Running the Generator Locally

```bash
# From the workspace root
export $(grep -v '^#' backend/.env | xargs)

# Verify the spec is fresh (CI mode)
cargo test --test openapi_export_test -p rustshare-server

# Regenerate the committed JSON after handler/schema changes
RUSTSHARE_UPDATE_OPENAPI=1 cargo test --test openapi_export_test -p rustshare-server
```

## Dependencies

Workspace-level (`Cargo.toml`):

```toml
utoipa = { version = "5.0", features = ["uuid", "chrono"] }
utoipa-swagger-ui = { version = "9.0", features = ["axum"] }
```

Used in:

- `rustshare-core` — domain types and event payloads
- `rustshare-server` — handlers and server-local request/response types

## References

- ADR-0030: `docs/adr/0030-openapi-auto-generation.md`
- Generated spec: `docs/contracts/rustshare-api-openapi.json`
- OpenAPI module: `backend/server/src/openapi.rs`
- Freshness test: `backend/tests/openapi_export_test.rs`
