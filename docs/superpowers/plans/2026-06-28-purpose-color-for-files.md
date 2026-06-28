# Purpose Color for Files — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist and display purpose colors for files, fixing the note-editor color picker and adding file-list color tags.

**Architecture:** Add a `color` column to the `files` table as the single source of truth. Sync note color changes into this column, expose it through all file-list APIs, and render it as a small tag in the file list with a context-menu action to set it.

**Tech Stack:** Rust (Axum/SQLx), PostgreSQL, TypeScript/SvelteKit, Vitest.

---

## File Map

| File | Responsibility |
|------|----------------|
| `backend/migrations/20260628000001_add_color_to_files.sql` | Add `color` column to `files` table. |
| `backend/server/src/services/note_service.rs` | Update `save_note` to also write `files.color`. |
| `backend/server/src/handlers/files.rs` | Add `PATCH /files/{id}/color`, update `FileWithShares` and list queries. |
| `backend/server/src/handlers/folders.rs` | Include `f.color` in folder contents queries. |
| `backend/server/src/routes.rs` | Register new `PATCH /files/{id}/color` route. |
| `backend/server/src/openapi.rs` | Add new handler/schema to OpenAPI if needed. |
| `frontend/src/lib/api/types.ts` | Add `color?: string \| null` to `File`. |
| `frontend/src/lib/api/files.ts` | Add `setFileColor(fileId, color)` client function. |
| `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` | Pass `event.detail.color` into save mutation. |
| `frontend/src/lib/files/FileListRow.svelte` | Render color tag dot. |
| `frontend/src/lib/files/FileGridTile.svelte` | Render color tag dot. |
| `frontend/src/lib/explorer/FileBrowserContent.svelte` | Wire color-set action into item menus. |
| `frontend/src/routes/(app)/files/+page.svelte` | Add color set/update handler and invalidate queries. |

---

## Task 1: Database Migration

**Files:**
- Create: `backend/migrations/20260628000001_add_color_to_files.sql`

- [ ] **Step 1: Create migration**

```sql
ALTER TABLE files ADD COLUMN color TEXT;
```

- [ ] **Step 2: Verify migration ordering**

Ensure the migration filename is later than the most recent migration in `backend/migrations/`.

---

## Task 2: Backend Domain/Handler Updates

**Files:**
- Modify: `backend/server/src/handlers/files.rs`
- Modify: `backend/server/src/handlers/folders.rs`
- Modify: `backend/server/src/services/note_service.rs`
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/server/src/openapi.rs`

### Task 2.1: Add color to `FileWithShares`

- [ ] **Step 1: Update struct and queries**

Modify `backend/server/src/handlers/files.rs` around line 843:

```rust
pub struct FileWithShares {
    // File fields
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub current_version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    pub starred_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    // Share info
    pub is_shared: bool,
    pub share_count: i64,
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_permission: Option<String>,
}
```

- [ ] **Step 2: Select `f.color` in all file list queries**

Update every SQL query in `files.rs` that selects from `files f` to include `f.color`. Search for `SELECT\n\s*f.id, f.name` patterns and add `f.color` after `f.deleted_at`.

- [ ] **Step 3: Add `PATCH /files/{id}/color` endpoint**

Add after the existing file update handlers in `files.rs`:

```rust
#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct SetFileColorRequest {
    pub color: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/files/{id}/color",
    tag = "Files",
    params(("id" = Uuid, Path, description = "File ID")),
    request_body = SetFileColorRequest,
    responses(
        (status = 200, description = "Color updated", body = File),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
)]
pub async fn set_file_color(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    crate::handlers::ValidatedJson(req): crate::handlers::ValidatedJson<SetFileColorRequest>,
) -> Result<Json<File>, AppError> {
    let file = state
        .file_service
        .set_file_color(file_id, auth.user_id, auth.tenant_id, req.color)
        .await?;
    Ok(Json(file))
}
```

- [ ] **Step 4: Add `set_file_color` to `FileService`**

In the core file service (likely `backend/crates/core/src/services/file_service.rs` or similar), add:

```rust
pub async fn set_file_color(
    &self,
    file_id: Uuid,
    user_id: UserId,
    tenant_id: Uuid,
    color: Option<String>,
) -> Result<File, FileError> {
    let file = self.get_file(file_id, user_id).await?;
    if file.tenant_id != tenant_id {
        return Err(FileError::PermissionDenied);
    }
    sqlx::query(
        "UPDATE files SET color = $1, modified_at = NOW() WHERE id = $2"
    )
    .bind(&color)
    .bind(file_id)
    .execute(&self.db_pool)
    .await?;
    self.get_file(file_id, user_id).await
}
```

- [ ] **Step 5: Update `File` domain model and `get_file` query**

Ensure the core `File` struct and its `FromRow` query include `color`. Update `files` table selects in the file service.

### Task 2.2: Include color in folder contents

- [ ] **Step 1: Update folder contents query**

In `backend/server/src/handlers/folders.rs`, update the file selection query around line 275 to include `f.color`:

```rust
let files = sqlx::query_as::<_, crate::handlers::files::FileWithShares>(
    r#"
    SELECT
        f.id, f.name, f.path, f.size, f.mime_type,
        f.parent_folder_id, f.owner_id, f.current_version,
        f.created_at, f.modified_at, f.starred_at, f.deleted_at,
        f.color,
        ...
    "#
)
```

### Task 2.3: Sync note save to file color

- [ ] **Step 1: Update `save_note` in note_service.rs**

After updating metadata color (around line 1612), also update the file row:

```rust
if let Some(new_color) = color {
    meta.color = Some(new_color.clone());
    sqlx::query("UPDATE files SET color = $1 WHERE id = $2")
        .bind(&new_color)
        .bind(file_id)
        .execute(&self.db_pool)
        .await
        .map_err(|e| NoteError::Storage(e.to_string()))?;
}
```

### Task 2.4: Register route and OpenAPI

- [ ] **Step 1: Add route**

In `backend/server/src/routes.rs`, add:

```rust
.route("/api/v1/files/{id}/color", patch(crate::handlers::set_file_color))
```

- [ ] **Step 2: Add to OpenAPI**

In `backend/server/src/openapi.rs`, add `crate::handlers::files::set_file_color` to the paths list and `SetFileColorRequest` to components if required by utoipa.

---

## Task 3: Frontend API Client

**Files:**
- Modify: `frontend/src/lib/api/types.ts`
- Modify: `frontend/src/lib/api/files.ts`

- [ ] **Step 1: Add color to File type**

```typescript
export interface File {
    id: string;
    name: string;
    ...
    color?: string | null;
    ...
}
```

- [ ] **Step 2: Add setFileColor client function**

In `frontend/src/lib/api/files.ts`:

```typescript
export async function setFileColor(fileId: string, color: string | null): Promise<File> {
    return apiClient.patch<File>(`/files/${fileId}/color`, { color });
}
```

---

## Task 4: Fix Note Editor Color Save

**Files:**
- Modify: `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte`

- [ ] **Step 1: Pass color to save mutation**

Update `handleSave` around line 260:

```typescript
async function handleSave(event: CustomEvent<{ content: string; color?: string | null; docId?: string }>) {
    ...
    const saved = await $saveMutation.mutateAsync({
        title,
        content: saveContent,
        color: event.detail.color
    });
    ...
}
```

Update `getUpdateFunction` to accept and pass `color`:

```typescript
function getUpdateFunction(key: string, itemId: string) {
    return (data: { title: string; content: string; color?: string | null }) => {
        switch (key) {
            case 'notes':
                return notesApi.update(itemId, {
                    content: data.content,
                    color: data.color,
                    attachments: serializeNoteAttachments()
                });
            ...
        }
    };
}
```

Update `saveMutation` type:

```typescript
const saveMutation = createMutation<unknown, Error, { title: string; content: string; color?: string | null }>({...});
```

---

## Task 5: File List Color Tag UI

**Files:**
- Modify: `frontend/src/lib/files/FileListRow.svelte`
- Modify: `frontend/src/lib/files/FileGridTile.svelte`
- Modify: `frontend/src/lib/explorer/FileBrowserContent.svelte`
- Modify: `frontend/src/routes/(app)/files/+page.svelte`

### Task 5.1: Render color tag

- [ ] **Step 1: Add color dot to file list row**

In `FileListRow.svelte`, near the file name, add:

```svelte
{#if file.color}
    <span
        class="inline-block h-2 w-2 rounded-full"
        style="background-color: var(--rs-accent-{file.color}, {file.color});"
        title="Purpose color: {file.color}"
    ></span>
{/if}
```

- [ ] **Step 2: Add color dot to grid tile**

In `FileGridTile.svelte`, add similar indicator near the file name.

### Task 5.2: Add set-color action

- [ ] **Step 1: Add color palette modal/menu**

Reuse the `PURPOSEFUL_COLORS` palette from `MarkdownDocumentPage.svelte` or extract it to a shared constant. Create a small modal component `frontend/src/lib/components/files/FileColorPicker.svelte`.

- [ ] **Step 2: Add menu action**

In `FileBrowserContent.svelte`, add "Set color" to the file action menu and dispatch a `setcolor` event.

- [ ] **Step 3: Handle set-color in files page**

In `frontend/src/routes/(app)/files/+page.svelte`:

```typescript
const setFileColorMutation = createMutation({
    mutationFn: ({ fileId, color }: { fileId: string; color: string | null }) =>
        setFileColor(fileId, color),
    onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
        queryClient.invalidateQueries({ queryKey: ['all-files'] });
    }
});

function handleSetFileColor(file: File, color: string | null) {
    $setFileColorMutation.mutate({ fileId: file.id, color });
}
```

---

## Task 6: Tests

**Files:**
- Create/Modify: backend tests in `backend/tests/`
- Create/Modify: frontend tests for file color

### Backend tests

- [ ] **Step 1: Test note save updates file color**

In `backend/tests/notes_test.rs`, extend an existing save test to verify `files.color` is updated after `save_note(..., Some("blue".into()), ...)`.

- [ ] **Step 2: Test `PATCH /files/{id}/color`**

Add a test in `backend/tests/files_test.rs` or similar that calls the new endpoint and verifies the response and DB state.

- [ ] **Step 3: Test folder contents returns color**

Verify `GET /folders/{id}/contents` returns `color` in file objects.

### Frontend tests

- [ ] **Step 1: Test note save passes color**

In `frontend/src/routes/(app)/modules/[key]/[id]/page.test.ts`, update the mock to assert `notesApi.update` is called with `color` when a save event includes it.

- [ ] **Step 2: Test file list color indicator**

In `frontend/src/lib/files/FileListRow.test.ts` (create if missing), render a file with `color: 'blue'` and assert the dot element exists.

---

## Task 7: Verification

- [ ] **Step 1: Backend checks**

Run:
```bash
cd backend
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo test --workspace
```

Expected: all tests pass, 0 errors.

- [ ] **Step 2: Frontend checks**

Run:
```bash
cd frontend
npm run check
npm run test
npm run lint
```

Expected: `npm run check` 0 errors, `npm run test` all pass, `npm run lint` 0 errors.

---

## Self-Review Checklist

- [x] Spec coverage: saving note color, file-level color storage, file list display, and set-color action all have tasks.
- [x] Placeholder scan: no TBDs; all code blocks show concrete snippets.
- [x] Type consistency: `color: Option<String>` / `color?: string | null` used consistently.
