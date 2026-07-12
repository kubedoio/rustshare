# Mail Client Phase 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a minimal, read-only mail client: backend read endpoints for message parts, raw `.eml`, and attachments; frontend module list and message detail viewer with sanitized HTML/text bodies.

**Architecture:** Extend the existing mail service with metadata queries and streaming service methods, add Axum handlers, wire routes/OpenAPI, then register a new `mail-list` frontend module renderer with a Svelte list/detail UI. HTML body parts are sanitized server-side with `ammonia` before delivery.

**Tech Stack:** Rust, Axum, SQLx, `ammonia`, SvelteKit, TypeScript, TanStack Query, Lucide icons.

---

### Task 1: Add `ammonia` dependency and `MailMessageViewed` event

**Files:**
- Modify: `backend/server/Cargo.toml`
- Modify: `backend/crates/core/src/events/types.rs`
- Test: `backend/crates/core/tests/event_type_test.rs` (existing)

- [ ] **Step 1: Add dependency**

Add to `backend/server/Cargo.toml` under `[dependencies]`:

```toml
ammonia = "4"
```

- [ ] **Step 2: Add event variant and payload**

In `backend/crates/core/src/events/types.rs`:

1. Add `MailMessageViewed` to the `EventType` enum (after `MailArchiveJobDeleted`).
2. Add the string mapping in `EventType::as_str`.
3. Add the payload struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailMessageViewedPayload {
    #[schema(value_type = Uuid)]
    pub message_id: Uuid,
    #[schema(value_type = Uuid)]
    pub viewed_by: UserId,
    pub view_type: String, // "body" or "source"
}
```

- [ ] **Step 3: Verify compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add backend/server/Cargo.toml backend/crates/core/src/events/types.rs
git commit -s -m "chore(mail): add ammonia dependency and MailMessageViewed event"
```

---

### Task 2: Add metadata query for mail message parts

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`
- Test: `backend/crates/storage/src/metadata.rs` (compile check)

- [ ] **Step 1: Add the query**

Insert after `create_mail_attachment` in `backend/crates/storage/src/metadata.rs`:

```rust
    /// List body parts for a mail message, scoped to the owning user and tenant.
    pub async fn list_mail_message_parts_by_message_id(
        &self,
        message_id: Uuid,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<MailMessagePart>> {
        let rows = sqlx::query_as!(
            MailMessagePart,
            r#"
            SELECT
                p.id, p.tenant_id, p.message_id, p.part_index, p.content_type, p.charset,
                p.blob_key, p.blob_sha256, p.size_bytes, p.is_body, p.created_at
            FROM mail_message_parts p
            WHERE p.message_id = $1
              AND p.tenant_id = $2
              AND EXISTS (
                  SELECT 1 FROM mail_messages m
                  WHERE m.id = p.message_id
                    AND m.tenant_id = $2
                    AND m.owner_id = $3
                    AND m.deleted_at IS NULL
              )
            ORDER BY p.part_index ASC
            "#,
            message_id,
            tenant_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
```

- [ ] **Step 2: Verify compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-storage`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -s -m "feat(mail): add metadata query for mail message parts"
```

---

### Task 3: Implement `MailService::list_parts` and add read helpers

**Files:**
- Modify: `backend/server/src/services/mail_service.rs`
- Test: `backend/server/src/services/mail_service.rs` (compile check)

- [ ] **Step 1: Replace the placeholder and add helpers**

Replace the existing `list_parts` placeholder in `backend/server/src/services/mail_service.rs` with:

```rust
    /// List body parts for a message, scoped to the owning user and tenant.
    pub async fn list_parts(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<Vec<MailMessagePart>, MailError> {
        self.metadata_store
            .list_mail_message_parts_by_message_id(message_id, tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    /// Fetch a single message part and its blob bytes.
    pub async fn get_message_part(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
        part_id: Uuid,
    ) -> Result<(MailMessagePart, bytes::Bytes), MailError> {
        let parts = self.list_parts(tenant_id, owner_id, message_id).await?;
        let part = parts
            .into_iter()
            .find(|p| p.id == part_id)
            .ok_or(MailError::NotFound(part_id))?;
        let blob_key = part
            .blob_key
            .clone()
            .ok_or_else(|| MailError::InvalidSource("part has no blob".to_string()))?;
        let bytes = self
            .object_store
            .get(&blob_key)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;
        Ok((part, bytes))
    }

    /// Download the original raw `.eml` source for a message.
    pub async fn download_message_source(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<(String, bytes::Bytes), MailError> {
        let msg = self.get_message(tenant_id, owner_id, message_id).await?;
        let blob_key = msg
            .blob_key
            .ok_or_else(|| MailError::InvalidSource("message has no source blob".to_string()))?;
        let bytes = self
            .object_store
            .get(&blob_key)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;
        let filename = format!("message-{message_id}.eml");
        Ok((filename, bytes))
    }
```

- [ ] **Step 2: Verify compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/services/mail_service.rs
git commit -s -m "feat(mail): implement list_parts, get_message_part, and source download"
```

---

### Task 4: Add backend handlers for parts, source, and attachments

**Files:**
- Modify: `backend/server/src/handlers/mail.rs`
- Test: `backend/server/src/handlers/mail.rs` unit/integration tests

- [ ] **Step 1: Add response types**

Add after `MailMessageResponse` in `backend/server/src/handlers/mail.rs`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessagePartResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub part_index: i32,
    pub content_type: String,
    pub charset: Option<String>,
    pub size_bytes: Option<i64>,
    pub is_body: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessagePartsResponse {
    pub parts: Vec<MailMessagePartResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessageAttachmentsResponse {
    pub attachments: Vec<MailAttachmentResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailAttachmentResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Option<Uuid>)]
    pub file_id: Option<Uuid>,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
}

impl From<MailMessagePart> for MailMessagePartResponse {
    fn from(part: MailMessagePart) -> Self {
        Self {
            id: part.id,
            part_index: part.part_index,
            content_type: part.content_type,
            charset: part.charset,
            size_bytes: part.size_bytes,
            is_body: part.is_body,
        }
    }
}

impl From<MailAttachment> for MailAttachmentResponse {
    fn from(att: MailAttachment) -> Self {
        Self {
            id: att.id,
            file_id: att.file_id,
            filename: att.filename,
            mime_type: att.mime_type,
            size_bytes: att.size_bytes,
        }
    }
}
```

- [ ] **Step 2: Add a helper to sanitize HTML parts**

Add a private function in `backend/server/src/handlers/mail.rs`:

```rust
fn sanitize_email_html(html: &str) -> String {
    ammonia::Builder::default()
        .url_schemes(
            [
                "http".into(),
                "https".into(),
                "mailto".into(),
            ]
            .into_iter()
            .collect(),
        )
        .clean(html)
        .to_string()
}
```

- [ ] **Step 3: Add the handlers**

Append to `backend/server/src/handlers/mail.rs`:

```rust
/// List body parts for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/parts",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Message parts", body = ListMailMessagePartsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_message_parts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ListMailMessagePartsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let parts = state
        .mail_service
        .list_parts(auth.tenant_id, auth.user_id, message_id)
        .await?;
    Ok(Json(ListMailMessagePartsResponse {
        parts: parts.into_iter().map(MailMessagePartResponse::from).collect(),
    }))
}

/// Get the content of a single message part. HTML parts are sanitized before delivery.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/parts/{part_id}",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail message ID"),
        ("part_id" = Uuid, Path, description = "Part ID"),
    ),
    responses(
        (status = 200, description = "Part content"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_message_part(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((message_id, part_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let (part, bytes) = state
        .mail_service
        .get_message_part(auth.tenant_id, auth.user_id, message_id, part_id)
        .await?;

    let content_type = if part.content_type.eq_ignore_ascii_case("text/html") {
        let sanitized = sanitize_email_html(std::str::from_utf8(&bytes).unwrap_or(""));
        emit_mail_message_viewed(&state, message_id, auth.user_id, "body").await?;
        return Ok((
            [(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))],
            sanitized,
        )
            .into_response());
    } else {
        part.content_type.clone()
    };

    emit_mail_message_viewed(&state, message_id, auth.user_id, "body").await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// Download the raw `.eml` source for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/source",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Raw message source"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_mail_message_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Response, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let (filename, bytes) = state
        .mail_service
        .download_message_source(auth.tenant_id, auth.user_id, message_id)
        .await?;
    emit_mail_message_viewed(&state, message_id, auth.user_id, "source").await?;

    let content_disposition = super::public_shares::build_content_disposition(&filename);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("message/rfc822"),
    );
    headers.insert(header::CONTENT_DISPOSITION, content_disposition);
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// List attachments for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/attachments",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Attachments", body = ListMailMessageAttachmentsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_message_attachments(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ListMailMessageAttachmentsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let attachments = state
        .mail_service
        .list_attachments(auth.tenant_id, auth.user_id, message_id)
        .await?;
    Ok(Json(ListMailMessageAttachmentsResponse {
        attachments: attachments
            .into_iter()
            .map(MailAttachmentResponse::from)
            .collect(),
    }))
}

async fn emit_mail_message_viewed(
    state: &AppState,
    message_id: Uuid,
    viewed_by: UserId,
    view_type: &str,
) -> Result<(), AppError> {
    use rustshare_core::events::{AggregateType, Event, EventType, MailMessageViewedPayload};
    let payload = MailMessageViewedPayload {
        message_id,
        viewed_by,
        view_type: view_type.to_string(),
    };
    let event = Event::new(
        EventType::MailMessageViewed,
        message_id,
        AggregateType::MailMessage,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        viewed_by,
    );
    state
        .event_store
        .append(&event)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: Add unit tests for the new handlers**

Add a `#[cfg(test)] mod tests` block at the end of `backend/server/src/handlers/mail.rs` (or extend the existing one from Phase 4) with:

```rust
#[cfg(test)]
mod read_tests {
    use super::{sanitize_email_html, CreateMailImportJobRequest};
    use validator::Validate;

    #[test]
    fn sanitize_email_html_strips_scripts() {
        let raw = r#"<p>Hello</p><script>alert('xss')</script><a href="javascript:bad()">click</a>"#;
        let clean = sanitize_email_html(raw);
        assert!(!clean.contains("<script>"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>Hello</p>"));
    }
}
```

- [ ] **Step 5: Verify compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add backend/server/src/handlers/mail.rs
git commit -s -m "feat(mail): add read handlers for parts, source, and attachments"
```

---

### Task 5: Wire routes, OpenAPI, and module summary

**Files:**
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/server/src/openapi.rs`
- Modify: `backend/server/src/services/module_service.rs`
- Test: `backend/tests/openapi_export_test.rs`

- [ ] **Step 1: Wire the new routes**

In `backend/server/src/routes.rs`, inside `mail_routes()`, add after the existing message routes:

```rust
    .route("/messages/{id}/parts", get(handlers::mail::list_mail_message_parts))
    .route("/messages/{id}/parts/{part_id}", get(handlers::mail::get_mail_message_part))
    .route("/messages/{id}/source", get(handlers::mail::download_mail_message_source))
    .route("/messages/{id}/attachments", get(handlers::mail::list_mail_message_attachments))
```

- [ ] **Step 2: Register handlers and schemas in OpenAPI**

In `backend/server/src/openapi.rs`:

1. Add the four new handlers to the `paths(...)` macro list.
2. Add the new response structs (`MailMessagePartResponse`, `ListMailMessagePartsResponse`, `MailAttachmentResponse`, `ListMailMessageAttachmentsResponse`) to `components(schemas(...))`.

- [ ] **Step 3: Add mail dashboard summary mode**

In `backend/server/src/services/module_service.rs`:

1. Add a `"mail"` arm in `build_summary_for_mode` before the `_ =>` fallback:

```rust
            "mail" => {
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM mail_messages WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await;
                let count = row.map(|r| r.count.unwrap_or(0)).unwrap_or(0);
                Ok(("mail-summary".to_string(), vec![], Some(json!({ "count": count }))))
            }
```

2. Update the seeded `mail` module config so the dashboard uses `summaryMode: "mail-summary"` (optional; the fallback is acceptable but a dedicated mode is cleaner).

- [ ] **Step 4: Verify OpenAPI and summary compile**

Run:
```bash
cd backend
SQLX_OFFLINE=true cargo check -p rustshare-server
SQLX_OFFLINE=true cargo test --all-features --test openapi_export_test -p rustshare-server
```
Expected: openapi test fails until regeneration.

- [ ] **Step 5: Regenerate OpenAPI contract**

Run:
```bash
cd backend
RUSTSHARE_UPDATE_OPENAPI=1 SQLX_OFFLINE=true cargo test --test openapi_export_test -p rustshare-server
```

- [ ] **Step 6: Commit**

```bash
git add backend/server/src/routes.rs backend/server/src/openapi.rs backend/server/src/services/module_service.rs docs/contracts/rustshare-api-openapi.json
git commit -s -m "feat(mail): wire read routes, openapi, and module summary"
```

---

### Task 6: Add backend tests for read endpoints

**Files:**
- Create: `backend/tests/mail_read_test.rs`
- Modify: `backend/server/src/handlers/mail.rs` (if adding unit tests inline)

- [ ] **Step 1: Write integration tests**

Create `backend/tests/mail_read_test.rs` with tests that:

1. Upload an `.eml` via the existing upload endpoint.
2. Call `GET /api/v1/mail/messages/{id}/parts` and assert at least one body part is returned.
3. Call `GET /api/v1/mail/messages/{id}/parts/{part_id}` for an HTML part and assert the response does not contain `<script>`.
4. Call `GET /api/v1/mail/messages/{id}/source` and assert `Content-Disposition: attachment` and `Content-Type: message/rfc822`.
5. Call `GET /api/v1/mail/messages/{id}/attachments` and assert the response matches the uploaded attachment.
6. Access another tenant's message and assert 403.

Use the existing `mail_upload_test.rs` and `mail_archive_job_test.rs` as references for test setup, authentication, and module enabling.

- [ ] **Step 2: Run tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test --all-features --test mail_read_test -p rustshare-server`
Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add backend/tests/mail_read_test.rs
git commit -s -m "test(mail): add integration tests for message read endpoints"
```


### Task 7: Register the mail module in the frontend

**Files:**
- Modify: `frontend/src/lib/modules/iconRegistry.ts`
- Modify: `frontend/src/lib/components/dashboard/ModuleIcon.svelte`
- Modify: `frontend/src/lib/modules/registry.ts`
- Modify: `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte`
- Modify: `frontend/src/lib/modules/modulePages.ts`

- [ ] **Step 1: Add the `mail` icon**

In `frontend/src/lib/modules/iconRegistry.ts`, add `'mail'` to `APPROVED_MODULE_ICONS`.

In `frontend/src/lib/components/dashboard/ModuleIcon.svelte`:

1. Import `Mail` from `lucide-svelte`.
2. Add `'mail': Mail` to `iconMap`.

- [ ] **Step 2: Add the predefined module definition**

In `frontend/src/lib/modules/registry.ts`, add this entry to `PREDEFINED_MODULES` (e.g., after shares):

```typescript
	{
		id: 'module_mail',
		key: 'mail',
		displayName: 'Mail',
		description: 'Import, archive, and reference email inside RustShare workspaces.',
		enabled: true,
		rootPath: '/Workspace/Mail',
		renderer: 'mail-list',
		defaultTemplate: null,
		icon: 'mail',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 65, icon: 'mail', label: 'Mail' },
			dashboard: {
				enabled: true,
				order: 65,
				widget: {
					enabled: true,
					type: 'mail-summary',
					title: 'Mail',
					description: 'Imported messages.',
					size: 'small',
					columns: { desktop: 3, tablet: 6, mobile: 12 },
					maxItems: 0,
					primaryAction: { label: 'Import mail', action: 'generic-create' }
				}
			},
			page: {
				enabled: true,
				route: '/modules/mail',
				renderer: 'mail-list',
				layout: 'list-grid',
				emptyStateTitle: 'No imported mail yet',
				emptyStateDescription:
					'No imported mail yet. Upload an .eml file or connect an IMAP account to import messages.',
				primaryAction: { label: 'Import mail', action: 'generic-create' },
				searchPlaceholder: 'Search messages...',
				filterLabel: 'All messages',
				sortLabel: 'Imported',
				itemSingular: 'message',
				itemPlural: 'messages'
			}
		},
		aiIndexing: { enabled: false },
		audit: { enabled: true }
	},
```

- [ ] **Step 3: Wire the renderer**

In `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte`:

1. `import MailModuleView from '$lib/components/modules/MailModuleView.svelte';`
2. Add `'mail-list': MailModuleView` to `rendererMap`.

- [ ] **Step 4: Add mail detail route mapping**

In `frontend/src/lib/modules/modulePages.ts`, add `mail: /modules/mail/messages/${objectId}` to `moduleRouteMap` in `getModuleObjectHref`.

- [ ] **Step 5: Verify frontend compile**

Run: `cd frontend && npm run check`
Expected: may fail until `MailModuleView` is created.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/modules/iconRegistry.ts frontend/src/lib/components/dashboard/ModuleIcon.svelte frontend/src/lib/modules/registry.ts frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte frontend/src/lib/modules/modulePages.ts
git commit -s -m "feat(mail): register mail module in frontend"
```

---

### Task 8: Create the frontend mail API client

**Files:**
- Create: `frontend/src/lib/api/mail.ts`

- [ ] **Step 1: Create the API client**

Create `frontend/src/lib/api/mail.ts`:

```typescript
import { apiClient } from './client';

export interface MailMessage {
	id: string;
	subject: string | null;
	from_address: string | null;
	from_name: string | null;
	to_addresses: unknown;
	cc_addresses: unknown;
	bcc_addresses: unknown;
	sent_at: string | null;
	imported_at: string;
	size_bytes: number;
	has_attachments: boolean;
}

export interface MailMessagePart {
	id: string;
	part_index: number;
	content_type: string;
	charset: string | null;
	size_bytes: number | null;
	is_body: boolean;
}

export interface MailAttachment {
	id: string;
	file_id: string | null;
	filename: string;
	mime_type: string | null;
	size_bytes: number | null;
}

export interface ListMailMessagesResponse {
	messages: MailMessage[];
}

export interface ListMailMessagePartsResponse {
	parts: MailMessagePart[];
}

export interface ListMailMessageAttachmentsResponse {
	attachments: MailAttachment[];
}

export const mailApi = {
	listMessages: async (): Promise<MailMessage[]> => {
		const res = await apiClient.get<ListMailMessagesResponse>('/mail/messages');
		return res.messages;
	},

	getMessage: async (id: string): Promise<MailMessage> => {
		return apiClient.get<MailMessage>(`/mail/messages/${id}`);
	},

	listParts: async (messageId: string): Promise<MailMessagePart[]> => {
		const res = await apiClient.get<ListMailMessagePartsResponse>(
			`/mail/messages/${messageId}/parts`
		);
		return res.parts;
	},

	getPartContent: async (messageId: string, partId: string): Promise<string> => {
		return apiClient.requestText(`/mail/messages/${messageId}/parts/${partId}`);
	},

	listAttachments: async (messageId: string): Promise<MailAttachment[]> => {
		const res = await apiClient.get<ListMailMessageAttachmentsResponse>(
			`/mail/messages/${messageId}/attachments`
		);
		return res.attachments;
	},

	downloadSourceUrl: (messageId: string): string => {
		return `${apiClient.getBaseURL()}/mail/messages/${messageId}/source`;
	}
};
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend && npm run check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/api/mail.ts
git commit -s -m "feat(mail): add frontend mail API client"
```

---

### Task 9: Build `MailModuleView` (message list)

**Files:**
- Create: `frontend/src/lib/components/modules/MailModuleView.svelte`

- [ ] **Step 1: Create the component**

Create `frontend/src/lib/components/modules/MailModuleView.svelte` with this structure:

```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailMessage } from '$lib/api/mail';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { Mail, Download } from 'lucide-svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	const messagesQuery = createQuery({
		queryKey: ['mail-messages'],
		queryFn: () => mailApi.listMessages()
	});

	function handleOpenMessage(message: MailMessage) {
		goto(`/modules/mail/messages/${message.id}`);
	}

	function formatAddresses(value: unknown): string {
		if (Array.isArray(value)) return value.join(', ');
		return String(value ?? '');
	}
</script>

<ModulePageShell title="Mail" subtitle={module.ui.page.emptyStateDescription}>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={() => goto('/files?folder=')}>
			<Download size={14} />
			<span>Import mail</span>
		</button>
	</div>

	{#if $messagesQuery.isLoading}
		<ModulePageSkeleton />
	{:else if $messagesQuery.isError}
		<ErrorState
			title="Failed to load mail"
			message={$messagesQuery.error?.message || 'Unknown error'}
			onRetry={() => $messagesQuery.refetch()}
		/>
	{:else if !$messagesQuery.data || $messagesQuery.data.length === 0}
		<EmptyState
			icon={'✉️'}
			title={module.ui.page.emptyStateTitle}
			description={module.ui.page.emptyStateDescription}
			actionLabel={module.ui.page.primaryAction?.label}
			onAction={() => goto('/files')}
		/>
	{:else}
		<div class="flex flex-col gap-2">
			{#each $messagesQuery.data as message}
				<button
					type="button"
					class="flex items-center gap-4 rounded-xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40"
					onclick={() => handleOpenMessage(message)}
				>
					<div
						class="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
					>
						<Mail size={22} />
					</div>
					<div class="flex min-w-0 flex-1 flex-col gap-1">
						<span class="truncate text-sm font-semibold text-base-content">
							{message.subject || '(no subject)'}
						</span>
						<span class="truncate text-xs text-base-content/55">
							{message.from_name || message.from_address || 'Unknown sender'}
							{#if message.sent_at}
								• {new Date(message.sent_at).toLocaleString()}
							{:else}
								• imported {new Date(message.imported_at).toLocaleString()}
							{/if}
						</span>
						<span class="truncate text-xs text-base-content/45">
							To: {formatAddresses(message.to_addresses)}
						</span>
					</div>
					{#if message.has_attachments}
						<span class="badge badge-sm badge-ghost">attachments</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}
</ModulePageShell>
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend && npm run check`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/modules/MailModuleView.svelte
git commit -s -m "feat(mail): add mail message list view"
```

---

### Task 10: Build the mail message detail page

**Files:**
- Create: `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`

- [ ] **Step 1: Create the detail page**

Create `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`:

```svelte
<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailAttachment, type MailMessagePart } from '$lib/api/mail';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import { sanitizeHtml } from '$lib/editor/adapter/security';
	import { ArrowLeft, Download, Paperclip } from 'lucide-svelte';

	let messageId = $derived($page.params.messageId);

	const messageQuery = createQuery({
		queryKey: ['mail-message', messageId],
		queryFn: () => mailApi.getMessage(messageId),
		enabled: !!messageId
	});

	const partsQuery = createQuery({
		queryKey: ['mail-message-parts', messageId],
		queryFn: () => mailApi.listParts(messageId),
		enabled: !!messageId
	});

	const attachmentsQuery = createQuery({
		queryKey: ['mail-message-attachments', messageId],
		queryFn: () => mailApi.listAttachments(messageId),
		enabled: !!messageId
	});

	let bodyContent = $derived.by(async () => {
		const parts = $partsQuery.data ?? [];
		const htmlPart = parts.find((p) => p.is_body && p.content_type === 'text/html');
		const textPart = parts.find((p) => p.is_body && p.content_type === 'text/plain');
		const part = htmlPart ?? textPart;
		if (!part) return { type: 'empty' as const, content: '' };
		const raw = await mailApi.getPartContent(messageId, part.id);
		if (htmlPart) {
			return { type: 'html' as const, content: sanitizeHtml(raw) };
		}
		return { type: 'text' as const, content: raw };
	});

	let previewAttachment = $state<MailAttachment | null>(null);

	function formatAddresses(value: unknown): string {
		if (Array.isArray(value)) return value.join(', ');
		return String(value ?? '');
	}
</script>

{#if $messageQuery.isLoading || $partsQuery.isLoading}
	<ModulePageSkeleton />
{:else if $messageQuery.isError}
	<ErrorState
		title="Failed to load message"
		message={$messageQuery.error?.message || 'Unknown error'}
		onRetry={() => $messageQuery.refetch()}
	/>
{:else if $messageQuery.data}
	{@const message = $messageQuery.data}
	<ModulePageShell
		title={message.subject || '(no subject)'}
		subtitle={message.from_name || message.from_address || 'Unknown sender'}
	>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={() => goto('/modules/mail')}>
				<ArrowLeft size={14} />
				<span>Back</span>
			</button>
			<a
				href={mailApi.downloadSourceUrl(messageId)}
				download
				class="btn gap-2 btn-outline btn-sm"
			>
				<Download size={14} />
				<span>Download .eml</span>
			</a>
		</div>

		<div class="flex flex-col gap-6">
			<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
				<div class="grid grid-cols-1 gap-2 text-sm">
					<div><span class="text-base-content/55">From:</span> {formatAddresses([message.from_name, message.from_address].filter(Boolean))}</div>
					<div><span class="text-base-content/55">To:</span> {formatAddresses(message.to_addresses)}</div>
					{#if message.cc_addresses && JSON.stringify(message.cc_addresses) !== '[]'}
						<div><span class="text-base-content/55">Cc:</span> {formatAddresses(message.cc_addresses)}</div>
					{/if}
					<div><span class="text-base-content/55">Date:</span> {message.sent_at ? new Date(message.sent_at).toLocaleString() : 'Unknown'}</div>
				</div>
			</div>

			<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
				{#await bodyContent}
					<ModulePageSkeleton />
				{:then body}
					{#if body.type === 'html'}
						<div class="prose max-w-none">{@html body.content}</div>
					{:else if body.type === 'text'}
						<pre class="whitespace-pre-wrap font-mono text-sm">{body.content}</pre>
					{:else}
						<EmptyState icon="📄" title="No readable body" description="This message has no text or HTML body part." />
					{/if}
				{:catch error}
					<ErrorState title="Failed to load body" message={error?.message || 'Unknown error'} />
				{/await}
			</div>

			{#if $attachmentsQuery.data && $attachmentsQuery.data.length > 0}
				<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
					<h3 class="mb-3 flex items-center gap-2 font-semibold">
						<Paperclip size={16} /> Attachments
					</h3>
					<div class="flex flex-wrap gap-2">
						{#each $attachmentsQuery.data as attachment}
							<button
								type="button"
								class="btn btn-outline btn-sm"
								onclick={() => (previewAttachment = attachment)}
							>
								{attachment.filename}
							</button>
						{/each}
					</div>
				</div>
			{/if}
		</div>
	</ModulePageShell>

	<FilePreviewModal
		open={previewAttachment !== null}
		file={previewAttachment
			? {
					id: previewAttachment.file_id ?? previewAttachment.id,
					name: previewAttachment.filename,
					mime_type: previewAttachment.mime_type ?? 'application/octet-stream',
					size: Number(previewAttachment.size_bytes ?? 0)
				}
			: null}
		onClose={() => (previewAttachment = null)}
	/>
{/if}
```

- [ ] **Step 2: Verify compile**

Run: `cd frontend && npm run check`
Expected: success (may warn about unused imports; fix them).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte
git commit -s -m "feat(mail): add mail message detail page"
```

---

### Task 11: Add frontend tests

**Files:**
- Create: `frontend/src/lib/api/mail.test.ts`
- Create or modify: `frontend/src/lib/components/modules/MailModuleView.test.ts`
- Modify: `frontend/src/lib/modules/registry.test.ts`

- [ ] **Step 1: Add API client tests**

Create `frontend/src/lib/api/mail.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { mailApi } from './mail';
import { apiClient } from './client';

vi.mock('./client', () => ({
	apiClient: {
		get: vi.fn(),
		requestText: vi.fn(),
		getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
	}
}));

describe('mailApi', () => {
	it('lists messages', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ messages: [{ id: '1', subject: 'Hi' }] });
		const result = await mailApi.listMessages();
		expect(result).toHaveLength(1);
		expect(apiClient.get).toHaveBeenCalledWith('/mail/messages');
	});

	it('fetches part content as text', async () => {
		vi.mocked(apiClient.requestText).mockResolvedValueOnce('hello');
		const result = await mailApi.getPartContent('msg-1', 'part-1');
		expect(result).toBe('hello');
		expect(apiClient.requestText).toHaveBeenCalledWith('/mail/messages/msg-1/parts/part-1');
	});
});
```

- [ ] **Step 2: Add registry test**

In `frontend/src/lib/modules/registry.test.ts`, assert that `PREDEFINED_MODULES` contains a module with `key === 'mail'` and renderer `mail-list`.

- [ ] **Step 3: Add component render test**

Create `frontend/src/lib/components/modules/MailModuleView.test.ts` that mocks `mailApi.listMessages` and asserts the component renders the subject row and navigates on click.

- [ ] **Step 4: Run tests**

Run: `cd frontend && npm run test`
Expected: all new and existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/api/mail.test.ts frontend/src/lib/components/modules/MailModuleView.test.ts frontend/src/lib/modules/registry.test.ts
git commit -s -m "test(mail): add frontend tests for mail client"
```

---

### Task 12: Regenerate SQLx metadata and final verification

**Files:**
- Modify: `backend/.sqlx/` (generated)
- Modify: `docs/contracts/rustshare-api-openapi.json` (generated)

- [ ] **Step 1: Regenerate SQLx offline query metadata**

Run:
```bash
cd backend
cargo sqlx prepare --workspace
```
Expected: `backend/.sqlx/query-*.json` files created/updated.

- [ ] **Step 2: Regenerate OpenAPI contract**

Run:
```bash
cd backend
RUSTSHARE_UPDATE_OPENAPI=1 SQLX_OFFLINE=true cargo test --test openapi_export_test -p rustshare-server
```
Expected: `docs/contracts/rustshare-api-openapi.json` updated and test passes.

- [ ] **Step 3: Run full validation matrix**

Backend:
```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --all-features --lib
SQLX_OFFLINE=true cargo test --all-features --test mail_read_test -p rustshare-server
SQLX_OFFLINE=true cargo test --all-features --test openapi_export_test -p rustshare-server
cargo sqlx prepare --workspace --check
```

Root workspace:
```bash
SQLX_OFFLINE=true cargo test --workspace --lib
```

Frontend:
```bash
cd frontend
npm install
npm run check
npm run lint
npm run test
npm run build
```

- [ ] **Step 4: Commit generated artifacts**

```bash
git add backend/.sqlx/ docs/contracts/rustshare-api-openapi.json
git commit -s -m "chore(mail): regenerate sqlx metadata and openapi contract"
```

- [ ] **Step 5: Push the feature branch**

Create a branch for the work:
```bash
git checkout -b feat/mail-phase5-client
git push -u origin feat/mail-phase5-client
```

---

## Self-Review

- **Spec coverage:** Every design requirement (parts, source, attachments, sanitized HTML, module registration, list/detail views, tests) maps to a task.
- **Placeholder scan:** No TBD/TODO placeholders; each task contains exact file paths, code, and commands.
- **Type consistency:** `MailMessagePartResponse`, `MailAttachmentResponse`, and frontend `MailMessage`/`MailMessagePart`/`MailAttachment` interfaces align with the backend domain types.
