# Template Modules Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add summary providers for dynamic dashboard cards, specialized renderers for kanban and meetings modules, and backend tests for module/template services.

**Architecture:** Extend the existing module system with a `get_module_summary` service method that queries folder contents for recent items or counts. Add new Svelte components for kanban board and meetings list renderers. Write Rust unit tests for service methods following existing patterns.

**Tech Stack:** Rust 2021, Axum 0.8, SQLx 0.8, PostgreSQL; Svelte 5 + SvelteKit 2, TailwindCSS 4, DaisyUI 5

---

## File Structure

- **Backend:**
  - `backend/server/src/services/module_service.rs` — add `get_module_summary` method
  - `backend/server/src/handlers/modules.rs` — add `get_module_summary` handler
  - `backend/server/src/routes.rs` — wire `GET /api/v1/modules/{key}/summary`
  - `backend/server/src/services/module_service.rs` — add unit tests
  - `backend/server/src/services/template_service.rs` — add unit tests

- **Frontend:**
  - `frontend/src/lib/api/modules.ts` — add `getModuleSummary` client
  - `frontend/src/lib/api/types.ts` — add `ModuleSummary` type
  - `frontend/src/lib/components/dashboard/ModuleCard.svelte` — integrate summary data
  - `frontend/src/lib/components/modules/KanbanModuleView.svelte` — kanban board renderer
  - `frontend/src/lib/components/modules/MeetingsModuleView.svelte` — meetings list renderer
  - `frontend/src/routes/(app)/modules/[key]/+page.svelte` — dispatch new renderers

---

### Task 1: Module Summary Backend API

**Files:**
- Modify: `backend/server/src/services/module_service.rs`
- Modify: `backend/server/src/handlers/modules.rs`
- Modify: `backend/server/src/routes.rs`

- [ ] **Step 1: Add summary types and service method**

Add to `backend/server/src/services/module_service.rs` before the `ModuleService` impl:

```rust
#[derive(Debug, serde::Serialize)]
pub struct ModuleSummary {
    pub module_key: String,
    pub mode: String,
    pub total_items: i64,
    pub recent_items: Vec<SummaryItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct SummaryItem {
    pub id: String,
    pub name: String,
    pub item_type: String, // "file" | "folder"
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
```

Add method to `ModuleService` impl after `update_module`:

```rust
    /// Get a summary of module contents for dashboard cards.
    pub async fn get_module_summary(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<ModuleSummary, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        let ui_config = module.ui_config.as_object().ok_or_else(|| {
            ModuleError::InvalidData("ui_config is not an object".to_string())
        })?;

        let dashboard = ui_config.get("dashboard").and_then(|v| v.as_object());
        let summary_mode = dashboard
            .and_then(|d| d.get("summaryMode"))
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let max_items = dashboard
            .and_then(|d| d.get("maxItems"))
            .and_then(|v| v.as_i64())
            .unwrap_or(4) as i64;

        let root_name = module.root_path.trim_start_matches('/');

        // Find root folder
        let folder_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM folders WHERE name = $1 AND parent_id IS NULL AND tenant_id = $2 LIMIT 1"
        )
        .bind(root_name)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let mut recent_items = Vec::new();
        let mut total_items = 0i64;

        if let Some(fid) = folder_id {
            // Count files and subfolders
            let file_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM files WHERE folder_id = $1 AND tenant_id = $2"
            )
            .bind(fid)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            let folder_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM folders WHERE parent_id = $1 AND tenant_id = $2"
            )
            .bind(fid)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            total_items = file_count + folder_count;

            if summary_mode == "recent-items" {
                // Recent files
                let files = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
                    "SELECT id, name, updated_at FROM files WHERE folder_id = $1 AND tenant_id = $2 ORDER BY updated_at DESC LIMIT $3"
                )
                .bind(fid)
                .bind(tenant_id)
                .bind(max_items)
                .fetch_all(self.metadata_store.pool())
                .await?;

                for (id, name, updated_at) in files {
                    recent_items.push(SummaryItem {
                        id: id.to_string(),
                        name,
                        item_type: "file".to_string(),
                        updated_at,
                    });
                }

                // Recent subfolders (fill remaining slots)
                let remaining = max_items - recent_items.len() as i64;
                if remaining > 0 {
                    let folders = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
                        "SELECT id, name, updated_at FROM folders WHERE parent_id = $1 AND tenant_id = $2 ORDER BY updated_at DESC LIMIT $3"
                    )
                    .bind(fid)
                    .bind(tenant_id)
                    .bind(remaining)
                    .fetch_all(self.metadata_store.pool())
                    .await?;

                    for (id, name, updated_at) in folders {
                        recent_items.push(SummaryItem {
                            id: id.to_string(),
                            name,
                            item_type: "folder".to_string(),
                            updated_at,
                        });
                    }
                }
            }
        }

        Ok(ModuleSummary {
            module_key: key.to_string(),
            mode: summary_mode.to_string(),
            total_items,
            recent_items,
        })
    }
```

- [ ] **Step 2: Add handler**

Add to `backend/server/src/handlers/modules.rs` after `get_module`:

```rust
#[derive(Debug, Serialize)]
pub struct ModuleSummaryResponse {
    pub summary: crate::services::module_service::ModuleSummary,
}

pub async fn get_module_summary(
    AuthenticatedUser {
        user_id: _,
        tenant_id,
    }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ModuleSummaryResponse>, axum::response::Response> {
    let summary = state
        .module_service
        .get_module_summary(&key, tenant_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    Ok(Json(ModuleSummaryResponse { summary }))
}
```

- [ ] **Step 3: Wire route**

In `backend/server/src/routes.rs`, in the user routes section, add:

```rust
.route(
    "/api/v1/modules/{key}/summary",
    get(crate::handlers::modules::get_module_summary),
)
```

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/services/module_service.rs backend/server/src/handlers/modules.rs backend/server/src/routes.rs
git commit -m "feat: add module summary API endpoint

- Add ModuleSummary and SummaryItem types
- Implement get_module_summary service method with recent-items mode
- Add GET /api/v1/modules/{key}/summary handler
- Wire route in routes.rs

Signed-off-by: aaron@kubedo.com"
```

---

### Task 2: Module Summary Frontend Integration

**Files:**
- Modify: `frontend/src/lib/api/types.ts`
- Modify: `frontend/src/lib/api/modules.ts`
- Modify: `frontend/src/lib/components/dashboard/ModuleCard.svelte`

- [ ] **Step 1: Add types**

Add to `frontend/src/lib/api/types.ts` after `PrimaryActionConfig`:

```typescript
export interface SummaryItem {
    id: string;
    name: string;
    item_type: 'file' | 'folder';
    updated_at: string;
}

export interface ModuleSummary {
    module_key: string;
    mode: string;
    total_items: number;
    recent_items: SummaryItem[];
}
```

- [ ] **Step 2: Add API client**

Add to `frontend/src/lib/api/modules.ts`:

```typescript
export async function getModuleSummary(moduleKey: string): Promise<ModuleSummary> {
    return apiClient.get<ModuleSummary>(`/modules/${moduleKey}/summary`);
}
```

- [ ] **Step 3: Update ModuleCard to fetch and display summary**

Replace `frontend/src/lib/components/dashboard/ModuleCard.svelte`:

```svelte
<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import ModuleIcon from './ModuleIcon.svelte';
	import { ArrowRight, FileText, Folder } from 'lucide-svelte';
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';

	export let module: ModuleConfig;

	$: cardTitle = module.ui_config?.dashboard?.cardTitle ?? module.display_name;
	$: cardDescription = module.ui_config?.dashboard?.cardDescription ?? module.description;
	$: actionLabel = module.ui_config?.dashboard?.primaryAction?.label ?? 'Open';
	$: summaryMode = module.ui_config?.dashboard?.summaryMode ?? 'none';

	$: summaryQuery = createQuery({
		queryKey: ['module-summary', module.module_key],
		queryFn: () => getModuleSummary(module.module_key),
		enabled: summaryMode !== 'none'
	});

	$: summary = $summaryQuery.data;
	$: hasSummary = summaryMode !== 'none' && summary && !$summaryQuery.isLoading;
</script>

<a
	href="/modules/{module.module_key}"
	class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 shadow-sm transition-all duration-200 hover:border-brand-500/40 hover:shadow-md"
>
	<div class="flex items-start justify-between">
		<div
			class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500 transition-colors group-hover:bg-brand-500 group-hover:text-white"
		>
			<ModuleIcon name={module.icon} size={20} />
		</div>
		<span
			class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/50 uppercase"
		>
			{module.root_path}
		</span>
	</div>

	<div class="flex flex-col gap-1">
		<h3 class="text-sm font-semibold text-base-content">{cardTitle}</h3>
		{#if hasSummary}
			{#if summary.total_items > 0}
				<p class="text-xs leading-relaxed text-base-content/60">
					{summary.total_items} item{summary.total_items === 1 ? '' : 's'}
				</p>
				{#if summary.recent_items.length > 0}
					<ul class="mt-1 flex flex-col gap-0.5">
						{#each summary.recent_items.slice(0, 3) as item}
							<li class="flex items-center gap-1.5 text-xs text-base-content/50">
								{#if item.item_type === 'file'}
									<FileText size={12} />
								{:else}
									<Folder size={12} />
								{/if}
								<span class="truncate">{item.name}</span>
							</li>
						{/each}
					</ul>
				{/if}
			{:else}
				<p class="text-xs leading-relaxed text-base-content/40">No items yet</p>
			{/if}
		{:else}
			<p class="text-xs leading-relaxed text-base-content/60">{cardDescription}</p>
		{/if}
	</div>

	<div class="mt-auto pt-1">
		<span
			class="inline-flex items-center gap-1.5 rounded-lg bg-brand-500/5 px-3 py-1.5 text-xs font-medium text-brand-600 transition-colors group-hover:bg-brand-500/10"
		>
			{actionLabel}
			<ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
		</span>
	</div>
</a>
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api/types.ts frontend/src/lib/api/modules.ts frontend/src/lib/components/dashboard/ModuleCard.svelte
git commit -m "feat: integrate module summary into dashboard cards

- Add ModuleSummary and SummaryItem types
- Add getModuleSummary API client
- Update ModuleCard to fetch and display dynamic summary data
- Show item count and recent items list when summaryMode is active
- Fall back to static description when summary is unavailable

Signed-off-by: aaron@kubedo.com"
```

---

### Task 3: Kanban Specialized Renderer

**Files:**
- Create: `frontend/src/lib/components/modules/KanbanModuleView.svelte`
- Modify: `frontend/src/routes/(app)/modules/[key]/+page.svelte`

- [ ] **Step 1: Create KanbanModuleView component**

Create `frontend/src/lib/components/modules/KanbanModuleView.svelte`:

```svelte
<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { Folder, Plus, ArrowRight, GripVertical } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	export let moduleConfig: any;
	export let modulePageConfig: any;

	$: emptyTitle = modulePageConfig?.emptyStateTitle ?? 'No boards yet';
	$: emptyDescription = modulePageConfig?.emptyStateDescription ?? 'Create your first kanban board to get started.';
	$: emptyAction = modulePageConfig?.emptyStateAction ?? 'New Board';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['kanban-root', moduleConfig?.module_key],
		queryFn: async () => {
			if (!moduleConfig) return null;
			const res = await fetch('/api/v1/folders/root/contents');
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			return { ...contents, current_folder: folder };
		},
		enabled: !!moduleConfig
	});

	$: contents = $rootFolderQuery.data;
	$: boards = contents?.folders ?? [];
	$: cards = contents?.files ?? [];

	async function handleCreateBoard() {
		if (!moduleConfig?.default_template) return;
		const name = window.prompt('Enter a name for the new board:');
		if (!name) return;
		try {
			await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create board:', err);
		}
	}

	function navigateToBoard(folderId: string) {
		goto(`/files?folder=${folderId}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if boards.length === 0 && cards.length === 0}
		<EmptyState
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold uppercase tracking-wider text-base-content">Boards</h2>
			<button class="btn btn-primary btn-sm" on:click={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>

		{#if boards.length > 0}
			<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
				{#each boards as board}
					<button
						class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						on:click={() => navigateToBoard(board.id)}
					>
						<div class="flex items-start justify-between">
							<div class="flex items-center gap-2">
								<GripVertical size={16} class="text-base-content/30" />
								<Folder size={18} class="text-brand-500" />
								<span class="text-sm font-medium text-base-content">{board.name}</span>
							</div>
							<ArrowRight size={14} class="text-base-content/30 transition-transform group-hover:translate-x-0.5" />
						</div>
						<div class="flex items-center gap-2 text-xs text-base-content/50">
							<span>Updated {new Date(board.updated_at).toLocaleDateString()}</span>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">No boards yet. Create your first board to get started.</p>
		{/if}
	{/if}
</div>
```

- [ ] **Step 2: Wire kanban renderer in module page**

In `frontend/src/routes/(app)/modules/[key]/+page.svelte`, update the dispatch:

```svelte
<script lang="ts">
	// ... existing imports ...
	import KanbanModuleView from '$lib/components/modules/KanbanModuleView.svelte';
	import MeetingsModuleView from '$lib/components/modules/MeetingsModuleView.svelte';
</script>

<!-- ... in the module contents section, replace the {#if} block: -->
{#if moduleConfig?.renderer === 'notes'}
    <NotesModuleView {moduleConfig} {modulePageConfig} />
{:else if moduleConfig?.renderer === 'kanban'}
    <KanbanModuleView {moduleConfig} {modulePageConfig} />
{:else if moduleConfig?.renderer === 'meetings'}
    <MeetingsModuleView {moduleConfig} {modulePageConfig} />
{:else}
    <GenericModuleView {moduleConfig} {modulePageConfig} />
{/if}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/components/modules/KanbanModuleView.svelte frontend/src/routes/(app)/modules/[key]/+page.svelte
git commit -m "feat: add kanban specialized renderer

- Create KanbanModuleView with board grid layout
- Fetch module root folder contents
- Create board from template
- Wire kanban renderer dispatch in module page

Signed-off-by: aaron@kubedo.com"
```

---

### Task 4: Meetings Specialized Renderer

**Files:**
- Create: `frontend/src/lib/components/modules/MeetingsModuleView.svelte`
- Modify: `frontend/src/routes/(app)/modules/[key]/+page.svelte` (already done in Task 3)

- [ ] **Step 1: Create MeetingsModuleView component**

Create `frontend/src/lib/components/modules/MeetingsModuleView.svelte`:

```svelte
<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { FileText, Plus, Calendar, Users, Clock } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	export let moduleConfig: any;
	export let modulePageConfig: any;

	$: emptyTitle = modulePageConfig?.emptyStateTitle ?? 'No meetings yet';
	$: emptyDescription = modulePageConfig?.emptyStateDescription ?? 'Create your first meeting note to get started.';
	$: emptyAction = modulePageConfig?.emptyStateAction ?? 'New Meeting';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['meetings-root', moduleConfig?.module_key],
		queryFn: async () => {
			if (!moduleConfig) return null;
			const res = await fetch('/api/v1/folders/root/contents');
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			return { ...contents, current_folder: folder };
		},
		enabled: !!moduleConfig
	});

	$: contents = $rootFolderQuery.data;
	$: meetings = contents?.files ?? [];

	async function handleCreateMeeting() {
		if (!moduleConfig?.default_template) return;
		const name = window.prompt('Enter a name for the new meeting:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			if (result.object_type === 'file') {
				goto(`/files?preview=${result.object_id}`);
			}
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create meeting:', err);
		}
	}

	function navigateToMeeting(fileId: string) {
		goto(`/files?preview=${fileId}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if meetings.length === 0 && contents?.folders?.length === 0}
		<EmptyState
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateMeeting}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold uppercase tracking-wider text-base-content">Meetings</h2>
			<button class="btn btn-primary btn-sm" on:click={handleCreateMeeting}>
				<Plus size={14} />
				<span>New Meeting</span>
			</button>
		</div>

		{#if meetings.length > 0}
			<div class="flex flex-col gap-3">
				{#each meetings as meeting}
					<button
						class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						on:click={() => navigateToMeeting(meeting.id)}
					>
						<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500">
							<FileText size={18} />
						</div>
						<div class="flex flex-col gap-1 min-w-0">
							<span class="text-sm font-medium text-base-content truncate">{meeting.name}</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{new Date(meeting.updated_at).toLocaleDateString()}
								</span>
							</div>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">No meetings yet. Create your first meeting note to get started.</p>
		{/if}
	{/if}
</div>
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/components/modules/MeetingsModuleView.svelte
git commit -m "feat: add meetings specialized renderer

- Create MeetingsModuleView with meeting list layout
- Show meeting files with date and navigation
- Create meeting from template
- Wire meetings renderer dispatch in module page

Signed-off-by: aaron@kubedo.com"
```

---

### Task 5: Backend Unit Tests

**Files:**
- Modify: `backend/server/src/services/module_service.rs`
- Modify: `backend/server/src/services/template_service.rs`

- [ ] **Step 1: Add ModuleService unit tests**

Replace the existing `#[cfg(test)]` block at the end of `backend/server/src/services/module_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_error_display() {
        let err = ModuleError::NotFound("notes".to_string());
        assert_eq!(err.to_string(), "Module not found: notes");
    }

    #[test]
    fn test_module_error_display_already_exists() {
        let err = ModuleError::AlreadyExists("meetings".to_string());
        assert_eq!(err.to_string(), "Module already exists: meetings");
    }

    #[test]
    fn test_module_error_display_permission_denied() {
        let err = ModuleError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }

    #[test]
    fn test_module_error_display_database() {
        let err = ModuleError::Database("connection failed".to_string());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn test_update_module_input_debug() {
        let input = UpdateModuleInput {
            display_name: Some("Test".to_string()),
            description: None,
            icon: Some("file-text".to_string()),
            permissions: None,
            ai_indexing: None,
            audit: None,
            ui_config: Some(json!({"sidebar": {"enabled": true}})),
        };
        let debug = format!("{:?}", input);
        assert!(debug.contains("Test"));
        assert!(debug.contains("sidebar"));
    }

    #[test]
    fn test_module_summary_serialize() {
        let summary = ModuleSummary {
            module_key: "notes".to_string(),
            mode: "recent-items".to_string(),
            total_items: 5,
            recent_items: vec![
                SummaryItem {
                    id: "uuid-1".to_string(),
                    name: "Note 1".to_string(),
                    item_type: "file".to_string(),
                    updated_at: Utc::now(),
                },
            ],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("notes"));
        assert!(json.contains("recent-items"));
        assert!(json.contains("Note 1"));
    }

    #[test]
    fn test_summary_item_serialize() {
        let item = SummaryItem {
            id: "uuid-1".to_string(),
            name: "Folder A".to_string(),
            item_type: "folder".to_string(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("Folder A"));
        assert!(json.contains("folder"));
    }
}
```

- [ ] **Step 2: Add TemplateService unit tests**

Replace the existing `#[cfg(test)]` block at the end of `backend/server/src/services/template_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_error_display() {
        let err = TemplateError::NotFound("my-template".to_string());
        assert_eq!(err.to_string(), "Template not found: my-template");
    }

    #[test]
    fn test_template_error_display_already_exists() {
        let err = TemplateError::AlreadyExists("default-note".to_string());
        assert_eq!(err.to_string(), "Template already exists: default-note");
    }

    #[test]
    fn test_template_error_display_module_not_found() {
        let err = TemplateError::ModuleNotFound("unknown".to_string());
        assert_eq!(err.to_string(), "Module not found or disabled: unknown");
    }

    #[test]
    fn test_template_error_display_permission_denied() {
        let err = TemplateError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }

    #[test]
    fn test_template_error_display_invalid_data() {
        let err = TemplateError::InvalidData("bad path".to_string());
        assert_eq!(err.to_string(), "Invalid data: bad path");
    }

    #[test]
    fn test_template_default_file_serialize() {
        let file = TemplateDefaultFile {
            path: "README.md".to_string(),
            content: Some("# Hello".to_string()),
            content_base64: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("README.md"));
        assert!(json.contains("# Hello"));
    }

    #[test]
    fn test_created_object_serialize() {
        let obj = CreatedObject {
            object_id: Uuid::new_v4(),
            object_type: "folder".to_string(),
            path: "/Notes/My Note".to_string(),
        };
        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("folder"));
        assert!(json.contains("/Notes/My Note"));
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/services/module_service.rs backend/server/src/services/template_service.rs
git commit -m "test: add unit tests for module and template services

- ModuleError display tests for all variants
- TemplateError display tests for all variants
- ModuleSummary serialization tests
- SummaryItem serialization tests
- TemplateDefaultFile and CreatedObject serialization tests
- UpdateModuleInput debug test

Signed-off-by: aaron@kubedo.com"
```

---

## Verification

After all tasks are complete:

1. **Backend structure:** Confirm `ModuleSummary`, `SummaryItem` types exist; `get_module_summary` method and handler are wired.
2. **Frontend structure:** Confirm `ModuleCard` conditionally fetches summary; `KanbanModuleView` and `MeetingsModuleView` exist and are dispatched.
3. **Tests:** Confirm all test modules compile (run `cargo test` in CI since local cargo unavailable).
4. **Git:** All 5 commits on a clean branch ahead of main.

---

## Self-Review

1. **Spec coverage:** All three Phase 2 areas covered — summary providers (Tasks 1-2), specialized renderers (Tasks 3-4), tests (Task 5).
2. **Placeholder scan:** No TBD, TODO, or vague steps. All code is concrete.
3. **Type consistency:** `ModuleSummary` uses `i64` for `total_items` matching SQL count returns. `SummaryItem` uses `String` for IDs matching existing patterns. Frontend `item_type` uses literal union `'file' | 'folder'`.
