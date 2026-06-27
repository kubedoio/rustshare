# OKF Notes — Frontend Conflict Resolution UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users resolve OKF note conflicts (title_mismatch, identity_mismatch, duplicate_id) directly from the note detail page instead of dismissing a read-only banner.

**Architecture:** Add a `POST /api/v1/notes/{id}/resolve-conflict` endpoint that wraps `NoteService::resolve_note_conflict`, expose it through the existing `notesApi`, and render resolution actions inside the conflict banner when the conflict kind supports them.

**Tech Stack:** Rust 1.95, Axum, SvelteKit 5, TypeScript.

---

## Files

- Modify: `backend/server/src/handlers/notes.rs`
- Modify: `backend/server/src/routes.rs`
- Modify: `frontend/src/lib/api/notes.ts`
- Modify: `frontend/src/lib/api/types.ts`
- Modify: `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte`
- Test: `frontend/src/lib/api/notes.test.ts` (create if missing)
- Test: `frontend/src/routes/(app)/modules/[key]/[id]/+page.test.ts` (create if missing)

---

## Task 1: Add the backend resolve-conflict endpoint

**Files:**
- Modify: `backend/server/src/handlers/notes.rs`

- [ ] **Step 1: Add request/response types**

Append after the `DuplicateNoteResponse` block:

```rust
// ============================================================================
// Resolve Conflict
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "strategy")]
pub enum ResolveConflictRequest {
    PreferYaml,
    PreferFolder,
    Custom { title: String },
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/resolve-conflict",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body = ResolveConflictRequest,
    responses(
        (status = 200, description = "Conflict resolved", body = GetNoteResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
        (status = 409, description = "No conflict to resolve", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn resolve_conflict(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    Json(req): Json<ResolveConflictRequest>,
) -> Result<Json<GetNoteResponse>, AppError> {
    use crate::services::note_service::NoteConflictResolution;

    let resolution = match req {
        ResolveConflictRequest::PreferYaml => NoteConflictResolution::PreferYaml,
        ResolveConflictRequest::PreferFolder => NoteConflictResolution::PreferFolder,
        ResolveConflictRequest::Custom { title } => NoteConflictResolution::Custom(title),
    };

    let note = state
        .note_service
        .resolve_note_conflict(note_id, auth.user_id, auth.tenant_id, resolution)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

    Ok(Json(GetNoteResponse {
        id: note.id,
        okf_id: note.okf_id,
        name: note.name,
        path: note.path,
        content: note.content,
        metadata: note.metadata,
        parent_folder_id: note.parent_folder_id,
        current_version: note.current_version,
        created_at: note.created_at.to_rfc3339(),
        modified_at: note.modified_at.to_rfc3339(),
        public_url,
    }))
}
```

- [ ] **Step 2: Add unit test for request deserialization**

In the same file's test module (or add a `#[cfg(test)]` block at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_conflict_request_deserializes() {
        let json = r#"{"strategy":"prefer_yaml"}"#;
        let req: ResolveConflictRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req, ResolveConflictRequest::PreferYaml));

        let json = r#"{"strategy":"custom","title":"My Title"}"#;
        let req: ResolveConflictRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req, ResolveConflictRequest::Custom { title } if title == "My Title"));
    }
}
```

- [ ] **Step 3: Compile handler**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: PASS.

---

## Task 2: Wire the route

**Files:**
- Modify: `backend/server/src/routes.rs`

- [ ] **Step 1: Add the route**

Find the notes route group (where `.route("/api/v1/notes/:id/duplicate", ...)` is registered) and add:

```rust
.route(
    "/api/v1/notes/:id/resolve-conflict",
    post(handlers::notes::resolve_conflict),
)
```

Use the exact Axum route syntax already in use (some files use `:id`, others use `{id}`).

- [ ] **Step 2: Verify routes compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: PASS.

---

## Task 3: Add the frontend API helper

**Files:**
- Modify: `frontend/src/lib/api/notes.ts`
- Modify: `frontend/src/lib/api/types.ts`

- [ ] **Step 1: Add the TypeScript conflict type**

In `frontend/src/lib/api/types.ts`, ensure `NoteConflict` is exported:

```typescript
export interface NoteConflict {
	kind: string;
	message: string;
	yaml_title?: string;
	folder_name?: string;
	manifest_title?: string;
	yaml_id?: string;
	sidecar_id?: string;
}
```

- [ ] **Step 2: Add `resolveConflict` to the notes API**

In `frontend/src/lib/api/notes.ts`, add:

```typescript
export type ConflictResolutionStrategy =
	| { strategy: 'prefer_yaml' }
	| { strategy: 'prefer_folder' }
	| { strategy: 'custom'; title: string };

export async function resolveConflict(
	noteId: string,
	resolution: ConflictResolutionStrategy
): Promise<Note> {
	const response = await fetch(`/api/v1/notes/${noteId}/resolve-conflict`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(resolution)
	});

	if (!response.ok) {
		const error = await response.json().catch(() => ({ message: 'Failed to resolve conflict' }));
		throw new Error(error.message || 'Failed to resolve conflict');
	}

	return response.json();
}
```

- [ ] **Step 3: Add a unit test for the API helper**

Create or update `frontend/src/lib/api/notes.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { resolveConflict } from './notes';

describe('resolveConflict', () => {
	it('sends the resolution payload', async () => {
		global.fetch = vi.fn().mockResolvedValue({
			ok: true,
			json: async () => ({ id: 'note-1', metadata: {} })
		});

		await resolveConflict('note-1', { strategy: 'prefer_yaml' });

		expect(global.fetch).toHaveBeenCalledWith(
			'/api/v1/notes/note-1/resolve-conflict',
			expect.objectContaining({
				method: 'POST',
				body: JSON.stringify({ strategy: 'prefer_yaml' })
			})
		);
	});
});
```

- [ ] **Step 4: Run frontend unit tests**

Run: `cd frontend && npm run test -- src/lib/api/notes.test.ts`
Expected: PASS.

---

## Task 4: Render resolution actions in the note detail page

**Files:**
- Modify: `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte`

- [ ] **Step 1: Import the new API helper**

Change:

```typescript
import { notesApi, renameNote, moveNote, deleteNote, duplicateNote } from '$lib/api/notes';
```

to:

```typescript
import {
	notesApi,
	renameNote,
	moveNote,
	deleteNote,
	duplicateNote,
	resolveConflict
} from '$lib/api/notes';
```

- [ ] **Step 2: Add resolution handler**

Add a new state variable and handler after `isDuplicating`:

```typescript
let isResolvingConflict = $state(false);
let showCustomConflictTitle = $state(false);
let customConflictTitle = $state('');

async function handleResolveConflict(
	resolution: import('$lib/api/notes').ConflictResolutionStrategy
) {
	if (isResolvingConflict || !item) return;
	await withToastLoading(
		(v) => (isResolvingConflict = v),
		() => resolveConflict(id, resolution),
		{
			successMessage: 'Conflict resolved',
			errorMessage: 'Failed to resolve conflict',
			onSuccess: () => {
				showCustomConflictTitle = false;
				customConflictTitle = '';
				$query.refetch();
			}
		}
	);
}
```

- [ ] **Step 3: Replace the read-only conflict banner**

Find the conflict banner block (around line 575) and replace it with:

```svelte
{#if conflict}
	<div class="alert alert-warning mb-2 rounded-lg" role="alert">
		<AlertTriangle size={18} />
		<div class="flex-1 min-w-0">
			<strong class="font-semibold">Conflict: {conflict.kind}</strong>
			<p class="text-sm">{conflict.message}</p>
		</div>
		<div class="flex flex-wrap gap-2 items-center">
			{#if conflict.kind === 'title_mismatch'}
				{#if conflict.yaml_title}
					<button
						class="btn btn-xs btn-ghost"
						disabled={isResolvingConflict}
						onclick={() => handleResolveConflict({ strategy: 'prefer_yaml' })}
					>
						Use YAML title
					</button>
				{/if}
				{#if conflict.folder_name}
					<button
						class="btn btn-xs btn-ghost"
						disabled={isResolvingConflict}
						onclick={() => handleResolveConflict({ strategy: 'prefer_folder' })}
					>
						Use folder name
					</button>
				{/if}
				<button
					class="btn btn-xs btn-ghost"
					disabled={isResolvingConflict}
					onclick={() => {
						showCustomConflictTitle = true;
						customConflictTitle = conflict.yaml_title || conflict.folder_name || title;
					}}
				>
					Custom title…
				</button>
			{:else if conflict.kind === 'identity_mismatch'}
				<button
					class="btn btn-xs btn-ghost"
					disabled={isResolvingConflict}
					onclick={() => handleResolveConflict({ strategy: 'prefer_yaml' })}
				>
					Use frontmatter ID
				</button>
			{:else if conflict.kind === 'duplicate_id'}
				<span class="text-xs">Open the file browser to remove duplicates manually.</span>
			{/if}
			<button
				class="btn btn-ghost btn-xs"
				onclick={() => (dismissedConflict = true)}
				aria-label="Dismiss conflict warning"
			>
				<X size={14} />
			</button>
		</div>
	</div>
{/if}

{#if showCustomConflictTitle}
	<PromptModal
		open={showCustomConflictTitle}
		title="Resolve conflict"
		message="Choose the resolved title"
		defaultValue={customConflictTitle}
		confirmLabel="Resolve"
		isLoading={isResolvingConflict}
		onConfirm={(newTitle) => {
			const trimmed = newTitle.trim();
			if (!trimmed) return;
			handleResolveConflict({ strategy: 'custom', title: trimmed });
		}}
		onCancel={() => {
			showCustomConflictTitle = false;
			customConflictTitle = '';
		}}
	/>
{/if}
```

- [ ] **Step 4: Verify Svelte type check**

Run: `cd frontend && npm run check`
Expected: PASS.

---

## Task 5: Verify end-to-end

- [ ] **Step 1: Backend tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test --workspace --lib --bins`
Expected: PASS.

- [ ] **Step 2: Frontend tests and type check**

Run: `cd frontend && npm run check && npm run test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/handlers/notes.rs \
       backend/server/src/routes.rs \
       frontend/src/lib/api/notes.ts \
       frontend/src/lib/api/types.ts \
       frontend/src/lib/api/notes.test.ts \
       frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte
git commit -s -m "feat(notes): resolve OKF conflicts from the UI"
```
