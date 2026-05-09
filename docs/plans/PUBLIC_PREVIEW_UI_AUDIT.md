# Public Preview UI — Baseline Audit & Implementation Plan

> Generated: 2026-05-03
> Scope: Frontend (SvelteKit + Tailwind v4 + DaisyUI) + select backend touchpoints
> Constraint: No code changes in this document. Research only.

---

## A. Code Map

### A.1 App Shell & Layout

| File | Component | Purpose |
|------|-----------|---------|
| `frontend/src/routes/+layout.svelte` | Root layout | QueryClientProvider, favicon |
| `frontend/src/routes/+layout.ts` | SSR disable | `export const ssr = false` — entire app CSR-only |
| `frontend/src/routes/(app)/+layout.svelte` | App group layout | Mounts AppShell, auth state, module refresh |
| `frontend/src/lib/layout/AppShell.svelte` | `AppShell` | LeftRail, Topbar, SidebarNav, `<main>` |
| `frontend/src/lib/layout/LeftRail.svelte` | `LeftRail` | Far-left icon rail: Home, Folders, Settings + dynamic modules |
| `frontend/src/lib/layout/Topbar.svelte` | `Topbar` | Header: mobile hamburger, nav label, global search, storage ring, notifications, theme toggle, user menu (**327 lines**) |
| `frontend/src/lib/layout/SidebarNav.svelte` | `SidebarNav` | Secondary sidebar for files page: Quick links, Library, folder tree (**603 lines**) |
| `frontend/src/lib/components/layout/Sidebar.svelte` | `Sidebar` | **Legacy / unused** — dead code risk |
| `frontend/src/lib/components/layout/Breadcrumbs.svelte` | `Breadcrumbs` | Reusable breadcrumb with hardcoded max-width breakpoints |
| `frontend/src/routes/admin/+layout.svelte` | Admin layout | Dark sidebar, admin guard, ~170 lines of inline SVG icons |

### A.2 Routes

| Route | File | Purpose |
|-------|------|---------|
| `/dashboard` | `(app)/dashboard/+page.svelte` | Workspace dashboard with widget grid |
| `/files` | `(app)/files/+page.svelte` | Unified file explorer (**1811 lines**) |
| `/files/edit/[id]` | `(app)/files/edit/[id]/+page.svelte` | Image editor |
| `/modules/[key]` | `(app)/modules/[key]/+page.svelte` | Module landing (notes, meetings, standups, kanban, decisions, brainstorming, shares) |
| `/modules/[key]/[id]` | `(app)/modules/[key]/[id]/+page.svelte` | Module item detail (notes, meetings, decisions) |
| `/modules/brainstorming/[boardId]` | `(app)/modules/brainstorming/[boardId]/+page.svelte` | Brainstorming board editor (Excalidraw) |
| `/notes/[id]` | `(app)/notes/[id]/+page.svelte` | Legacy note editor page |
| `/notifications` | `(app)/notifications/+page.svelte` | Notifications list |
| `/settings` | `(app)/settings/+page.svelte` | User settings |
| `/shared-with-me` | `(app)/shared-with-me/+page.svelte` | Redirects to `/files?root=shared` |
| `/shared-with-me/[type]/[id]` | `(app)/shared-with-me/[type]/[id]/+page.svelte` | Shared resource detail (**463 lines**) |
| `/shares` | `(app)/shares/+page.svelte` | Share Control Center (**corrupted markup**) |
| `/login` | `login/+page.svelte` | Login |
| `/invite/[token]` | `invite/[token]/+page.svelte` | Invite acceptance |
| `/share/[token]` | `share/[token]/+page.svelte` | Public share landing (**788 lines**) |
| `/p/note/[shareId]` | `p/note/[shareId]/+page.svelte` | Public note share |
| `/device` | `device/+page.svelte` | Device pairing |

### A.3 Module System

| File | Symbol | Purpose |
|------|--------|---------|
| `frontend/src/lib/modules/registry.ts` | `PREDEFINED_MODULES`, `refreshModules()` | 7 hardcoded modules with UI config and permissions (**615 lines**) |
| `frontend/src/lib/modules/modulePaths.ts` | `WORKSPACE_ROOT`, `getModuleRoot()` | Canonical path resolver: `/Workspace/{ModuleName}` |
| `frontend/src/lib/modules/modulePages.ts` | `getModuleRootContents()`, `resolveModuleFolderId()` | Resolves module paths to folder IDs, loads contents |
| `frontend/src/lib/modules/moduleActions.ts` | `runModulePrimaryAction()` | Dashboard card actions |
| `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte` | `ModulePageRenderer` | Maps `ui.page.renderer` to component |

### A.4 Module Views

| Module | File | Lines | Create Pattern |
|--------|------|-------|----------------|
| Notes | `NotesModuleView.svelte` | — | Inline button → `createNote()` → navigate |
| Meetings | `MeetingsModuleView.svelte` | — | `window.prompt()` → `meetingsApi.create()` → navigate |
| Standups | `StandupsModuleView.svelte` | — | `window.prompt()` → `createFromTemplate()` → navigate |
| Kanban | `KanbanModuleView.svelte` | **1887** | Modal (`CreateKanbanBoardModal`) → inline board view |
| Brainstorming | `BrainstormingModuleView.svelte` | **261** | Modal (`ModalBase`) → navigate to board |
| Decisions | `DecisionsModuleView.svelte` | — | `window.prompt()` → `decisionsApi.create()` → navigate |
| Shares | `SharesModuleView.svelte` | — | `window.prompt()` → `createFromTemplate()` → **does NOT open** |
| Generic fallback | `GenericModuleView.svelte` | — | `window.prompt()` → `createFromTemplate()` → navigate |

### A.5 Editors

| File | Component | Engine | Save Behavior |
|------|-----------|--------|---------------|
| `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte` | `MarkdownDocumentPage` | Tiptap (`RichMarkdownEditor`) | Autosave 1500ms + manual Ctrl+S |
| `frontend/src/lib/editor/components/RichMarkdownEditor.svelte` | `RichMarkdownEditor` | Tiptap | Emits `change` event — parent saves |
| `frontend/src/lib/editor/components/EditorToolbar.svelte` | `EditorToolbar` | Tiptap toolbar | Uses `prompt()` for link URLs |
| `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` | Module detail page | `MarkdownDocumentPage` | Autosave via API mutation |
| `frontend/src/routes/(app)/notes/[id]/+page.svelte` | Notes page | `MarkdownDocumentPage` | Autosave + auto-rename from H1 |
| `frontend/src/lib/components/editors/BaseEditor.svelte` | `BaseEditor` | Legacy shell | Manual save only, `confirm()` on close |
| `frontend/src/lib/components/editors/TextEditor.svelte` | `TextEditor` | Monaco | Manual via `BaseEditor` |
| `frontend/src/lib/components/editors/MarkdownEditor.svelte` | `MarkdownEditor` | Tiptap | Manual via `BaseEditor` |
| `frontend/src/lib/components/editors/ExcalidrawEditor.svelte` | `ExcalidrawEditor` | Excalidraw (React) | Manual via `BaseEditor`, memory leak risk |
| `frontend/src/routes/(app)/modules/brainstorming/[boardId]/+page.svelte` | Brainstorming editor | Excalidraw (React) | Autosave 1500ms, no `beforeunload` guard |

### A.6 Modals

| File | Component | Pattern | Notes |
|------|-----------|---------|-------|
| `frontend/src/lib/components/common/ModalBase.svelte` | `ModalBase` | Native `<dialog>` | Foundation. Title ID uses `Math.random()` |
| `frontend/src/lib/components/modals/DeleteConfirmation.svelte` | `DeleteConfirmation` | `ModalBase` | Simple deletion confirm |
| `frontend/src/lib/components/modals/CreateFileModal.svelte` | `CreateFileModal` | `ModalBase` | Multi-step: folder picker, type grid, filename |
| `frontend/src/lib/components/modals/CreateFolderModal.svelte` | `CreateFolderModal` | `ModalBase` | Folder name + location picker |
| `frontend/src/lib/components/modals/EditFileModal.svelte` | `EditFileModal` | `ModalBase` | Editable file selection |
| `frontend/src/lib/components/modals/RenameModal.svelte` | `RenameModal` | `ModalBase` | Single input rename |
| `frontend/src/lib/components/modals/UploadTargetModal.svelte` | `UploadTargetModal` | `ModalBase` | Upload destination picker |
| `frontend/src/lib/components/modals/EmptyTrashModal.svelte` | `EmptyTrashModal` | `ModalBase` | Trash emptying confirm |
| `frontend/src/lib/components/modals/ReplaceFileModal.svelte` | `ReplaceFileModal` | `ModalBase` | File replacement with version info |
| `frontend/src/lib/components/modals/CreateKanbanBoardModal.svelte` | `CreateKanbanBoardModal` | `ModalBase` | Board name input |
| `frontend/src/lib/components/modals/VersionHistoryModal.svelte` | `VersionHistoryModal` | DaisyUI `modal` | **Does NOT use ModalBase**. Nested restore confirm |
| `frontend/src/lib/components/modals/MoveModal.svelte` | `MoveModal` | Custom fixed div | **Does NOT use ModalBase** |
| `frontend/src/lib/components/modals/ShareModal.svelte` | `ShareModal` | DaisyUI `modal` | **884 lines**. Two tabs, 6 mutations inline |
| `frontend/src/lib/components/modals/FilePreviewModal.svelte` | `FilePreviewModal` | DaisyUI `modal` | Large preview modal |

### A.7 Kanban

| File | Component | Purpose |
|------|-----------|---------|
| `frontend/src/lib/components/modules/KanbanModuleView.svelte` | `KanbanModuleView` | **1887-line god component**: board list, board view, columns, cards, card modal, drag/drop |
| `frontend/src/lib/api/kanban.ts` | Kanban API | Board/card/column CRUD, move |
| `frontend/src/lib/components/dashboard/widgets/KanbanSummaryWidget.svelte` | `KanbanSummaryWidget` | Dashboard widget |
| `backend/server/src/services/kanban_service.rs` | `KanbanService` | Board creation, N+1 card loading, event journaling |
| `backend/server/src/handlers/kanban.rs` | Kanban handlers | 20+ routes |

### A.8 Brainstorming

| File | Component | Purpose |
|------|-----------|---------|
| `frontend/src/lib/components/modules/BrainstormingModuleView.svelte` | `BrainstormingModuleView` | Board gallery with thumbnails |
| `frontend/src/routes/(app)/modules/brainstorming/[boardId]/+page.svelte` | Brainstorming editor | Excalidraw integration, autosave, preview generation |
| `frontend/src/lib/components/dashboard/widgets/RecentBrainstormBoardsWidget.svelte` | `RecentBrainstormBoardsWidget` | Dashboard widget |
| `frontend/src/lib/editor/components/ExcalidrawEditor.svelte` | `ExcalidrawEditor` | Sketch insertion into notes |
| `backend/server/src/services/brainstorming_service.rs` | `BrainstormingService` | Board creation, save, preview |
| `backend/server/src/handlers/brainstorming.rs` | Brainstorming handlers | Board CRUD |

### A.9 Shares

| File | Component | Purpose |
|------|-----------|---------|
| `frontend/src/lib/components/modals/ShareModal.svelte` | `ShareModal` | **884 lines**. Public links + user/group sharing |
| `frontend/src/lib/components/files/ShareIndicator.svelte` | `ShareIndicator` | Inline share status badge |
| `frontend/src/lib/components/modules/SharesModuleView.svelte` | `SharesModuleView` | Share packages list |
| `frontend/src/routes/(app)/shares/+page.svelte` | Shares Control Center | Owned shares list (**corrupted markup**) |
| `frontend/src/routes/share/[token]/+page.svelte` | Public share landing | Anonymous access, uploads (**788 lines**) |
| `frontend/src/routes/(app)/shared-with-me/[type]/[id]/+page.svelte` | Shared resource detail | File preview / folder browse (**463 lines**) |
| `frontend/src/routes/p/note/[shareId]/+page.svelte` | Public note share | Read-only markdown |
| `frontend/src/lib/api/shares.ts` | Shares API | 18 functions. Public share functions use raw `fetch()` with `localhost:8080` fallback |
| `backend/crates/core/src/services/share_service.rs` | `ShareService` | **2212 lines**. Public/user/group share logic |
| `backend/crates/core/src/services/user_share_service.rs` | `UserShareService` | **Deprecated but still used** |

### A.10 Tests

| Category | Files | Coverage | Gaps |
|----------|-------|----------|------|
| Frontend unit | `shares.test.ts`, `activity.test.ts`, `fileSort.test.ts`, `selection.test.ts`, `theme.test.ts`, various component tests | API mocking, store logic, component rendering | **Zero tests for**: `ShareModal`, `SharesModuleView`, `KanbanModuleView`, `BrainstormingModuleView`, `FilePreviewModal`, `MoveModal`, `VersionHistoryModal` |
| Frontend E2E | `admin.e2e.ts` | Admin login, user/group mgmt, OIDC, audit log | **Zero share E2E tests** |
| Backend unit | `domain/share.rs` (22 tests), `share_errors.rs` (16 tests), handler DTO tests (11 tests) | Domain logic, error messages, deserialization | **Zero handler integration tests** |
| Backend contract | `share_link_contract.rs` (8 tests), `group_sharing_test.rs` (8 tests) | Share access, expiry, passwords, revocation, group sharing | **All 8 share contracts `#[ignore]`**. Group sharing: 6 of 8 `#[ignore]` |
| Backend integration | `file_operations.rs`, `folder_cascade.rs`, `invites_test.rs`, `kanban_test.rs`, `notes_test.rs`, `compat_layer_integration_test.rs` | File CRUD, folders, invites, kanban, notes, compat layer | No share-specific integration tests running in CI |

---

## B. Shared Implementation Risks

### B.1 Metadata Exposure (HIGH PRIORITY)

| Leak | Exposure Point | Risk |
|------|---------------|------|
| `index.md` | File browser inside artifact folders | **High** — visible as plain file named `index.md` |
| `__primary__.md` | File browser inside note/standup/decision folders | **Medium** — internal template marker visible |
| `.rustshare.json` | Backend search results | **Medium** — search handlers do NOT filter hidden files |
| `events.jsonl` | Backend search results | **Medium** — same search gap |
| Kanban folder hierarchy | "Open in Files" from Kanban | **High** — users see `00-Backlog`, `CARD-0001-...`, `index.md`, `attachments/` |
| Module artifact folders | "Open in Files" from all module views | **High** — raw internal structure visible |
| `index.editor.json` | File browser (if created) | **Medium** — no backend SQL filter |

**Files involved:**
- `frontend/src/routes/(app)/files/+page.svelte` (shows `index.md` as normal file)
- `frontend/src/lib/components/modules/KanbanModuleView.svelte` (opens raw board folder)
- `frontend/src/lib/components/modules/NotesModuleView.svelte` (opens raw note folder)
- `backend/server/src/handlers/search.rs` (no hidden-file filtering)
- `backend/server/src/handlers/folders.rs` (filters `.rustshare-*` but NOT `index.md`, `__primary__.md`)
- `backend/crates/core/src/services/folder_service.rs` (`is_hidden_file()` missing `index.md`, `__primary__.md`)

### B.2 Create-and-Open Gaps

| Flow | Opens After Create? | Pattern | Issue |
|------|---------------------|---------|-------|
| New note (module view) | ✅ Yes | Inline button | Navigates to `/modules/notes/{id}` |
| New note (files view) | ✅ Yes | `CreateFileModal` | Navigates to `/notes/{id}` — **different route** |
| New meeting | ✅ Yes | `window.prompt()` | Inconsistent with Notes/Brainstorming/Kanban |
| New standup | ✅ Yes | `window.prompt()` | Inconsistent |
| New Kanban board | ✅ Yes | Modal | Opens inline (good) |
| New brainstorming | ✅ Yes | Modal | Navigates to board page |
| New decision | ✅ Yes | `window.prompt()` | Inconsistent |
| New share package | ❌ **NO** | `window.prompt()` | **Only module that does NOT open after create** |
| Generic template | ✅ Yes | `window.prompt()` | Inconsistent |

**Shared issue:** No module has duplication guards. No unified create-modal pattern.

### B.3 Browser-Native Prompts (HIGH PRIORITY)

**`window.prompt` — 7 occurrences:**
1. `MeetingsModuleView.svelte:31` — meeting title
2. `StandupsModuleView.svelte:34` — standup name
3. `SharesModuleView.svelte:32` — share package name
4. `DecisionsModuleView.svelte:31` — decision title
5. `GenericModuleView.svelte:27` — generic item name
6. `KanbanModuleView.svelte:100` — rename board
7. `EditorToolbar.svelte:74` — insert link URL

**`window.confirm` — 14 occurrences:**
- `KanbanModuleView.svelte` — archive board, archive card, delete card, delete attachment, delete checklist
- `files/+page.svelte` — bulk delete, permanent file delete, permanent folder delete
- `admin/templates/+page.svelte` — delete template
- `admin/modules/+page.svelte` — disable module
- `shares/+page.svelte` — revoke share
- `ShareModal.svelte` — revoke share link, remove group access, remove recipient
- `BaseEditor.svelte` — unsaved changes guard

**`window.alert` — 8 occurrences:**
- `GenericModuleView.svelte` — no template, create failure
- `KanbanModuleView.svelte` — card move failed
- `ExcalidrawEditor.svelte` — empty canvas, export failure
- `share/[token]/+page.svelte` — download failure (×2)
- `notes/[id]/+page.svelte` — sketch upload failure

**No wrapper/abstraction layer exists.** Every call is direct browser-native API.

### B.4 Workspace Root / Folder Navigation Inconsistencies

| Issue | Detail |
|-------|--------|
| Module paths | All modules use `/Workspace/{ModuleName}` via `getModuleRoot()` |
| Legacy fallback | `modulePaths.ts` falls back to `/{ModuleName}` if `/Workspace/...` missing |
| "Open in Files" | `resolveModuleFolderId()` can return `null` silently — button does nothing |
| LeftRail links | Go to `/modules/{key}`, NOT to `/files?folder={id}` |
| Breadcrumb duplication | 4+ separate breadcrumb systems: `files/+page.svelte`, `explorerStore`, `SidebarNav.svelte`, `shared-with-me/[type]/[id]/+page.svelte` |
| Explorer store drift | `explorerStore` in `$lib/explorer/store.svelte.ts` is sophisticated but underutilized; files page manages its own URL-derived state |

### B.5 Modal System Fragmentation

Three modal patterns coexist:
1. `ModalBase` + native `<dialog>` (intended pattern)
2. DaisyUI `modal` + `modal-box` (`ShareModal`, `FilePreviewModal`, `VersionHistoryModal`, admin modals)
3. Custom fixed-position div (`MoveModal`)

Consequences:
- No unified focus trap
- No generic form-modal abstraction
- `ShareModal` (884 lines) and `KanbanModuleView` (1887 lines) are monolithic
- `VersionHistoryModal` has `console.log` debug statements

### B.6 Editor System Duality

| Aspect | Legacy File Editors | Modern Module Editors |
|--------|--------------------|----------------------|
| Shell | `BaseEditor` | `MarkdownDocumentPage` |
| Save | Manual only | Autosave 1500ms + manual |
| Engine | Monaco / Tiptap / Excalidraw | Tiptap |
| Dirty guard | `confirm()` | None (relies on parent) |
| Svelte version | Svelte 4 (`export let`, `$:`) | Svelte 5 (`$props`, `$state`, `$effect`) |

**Specific risks:**
- `ExcalidrawEditor` memory leak: `innerHTML = ''` instead of `root.unmount()`
- `ExcalidrawEditor` dirty state never resets after save
- `TextEditor` Monaco init uses `setTimeout(..., 50)` — fragile
- `standups` not in `modules/[key]/[id]/+page.svelte` API switch — **broken**
- No offline/draft persistence anywhere

### B.7 Backend Search Metadata Leak

`backend/server/src/handlers/search.rs` and `backend/crates/core/src/services/search_service.rs` do NOT filter hidden metadata files (`.rustshare.json`, `events.jsonl`, `*.editor.json`). If the search index includes them, they surface in global search.

---

## C. Section-Specific Risks

### C.1 Home (/dashboard)

| Risk | Detail |
|------|--------|
| Module widgets fetch full data | `KanbanSummaryWidget` fetches full board details for a summary view |
| No empty-state actions | `RecentBrainstormBoardsWidget` has no action if `primaryAction` missing |
| Dashboard is thin | Mostly widget wrappers — low risk for Public Preview |

### C.2 Folders (/files)

| Risk | Detail |
|------|--------|
| **1811-line god page** | Queries, mutations, state, breadcrumbs, uploads, drag-drop, modals all in one file |
| Duplicated tree-walking | `buildFolderPathFromApiTree()` recurses entire tree on every reactive update |
| Client-side search | `Topbar.svelte` fetches ALL files and ALL folders into memory for search |
| Mixed Svelte 4/5 | `$:` reactive statements alongside `$derived`/`$effect` |
| `index.md` visible | Not filtered from file listings — leaks artifact package structure |
| `__primary__.md` visible | Same issue for notes/standups/decisions |

### C.3 Notes

| Risk | Detail |
|------|--------|
| Two routes for same artifact | `/modules/notes/{id}` and `/notes/{id}` both exist |
| Broken back navigation | `notes/[id]/+page.svelte` calls `goto('/notes')` — route does not exist |
| Auto-rename from H1 | `notes/[id]/+page.svelte` renames on save if title changed — could surprise user |
| No duplication guard | Multiple "Untitled Note" entries possible |

### C.4 Meeting Notes

| Risk | Detail |
|------|--------|
| `window.prompt()` for title | Inconsistent UX — should use modal like Kanban/Brainstorming |
| No duplication guard | Same as Notes |

### C.5 Standup Records

| Risk | Detail |
|------|--------|
| `window.prompt()` for name | Inconsistent UX |
| **Broken detail page** | `modules/[key]/[id]/+page.svelte` does NOT include `standups` in API resolver switch — `api` is `null` |
| Dashboard action uses default name | `runModulePrimaryAction()` builds name from action label with no prompt |

### C.6 Kanban

| Risk | Detail |
|------|--------|
| **1887-line god component** | Should be split into ~6 components |
| N+1 query explosion (backend) | `get_board` iterates columns → cards → metadata + content per card |
| `events.jsonl` unbounded growth | Append-only file loaded entirely into memory on every card detail view |
| Activity feed is developer-facing | Raw `event_type.replace(/card\./, '')` shown to users |
| Drag/drop optimistic update race | Manual cache surgery; concurrent moves corrupt rollback |
| WIP limit stored but not enforced | Backend stores limit, never checks |
| No keyboard shortcuts | No accessibility for card creation/movement |
| Card modal ~450 lines inline | Title saves on blur; content requires explicit Save — inconsistent |
| `move_card` sequence race | `(before + after) / 2` ordering; theoretical overflow |
| `derive_preview` compiles regex on every call | Heavy for large boards |

### C.7 Brainstorming

| Risk | Detail |
|------|--------|
| **No `beforeunload` handler** | 1.5s debounce autosave — navigating away loses work |
| Heavy runtime dependency | Excalidraw + React + ReactDOM dynamically imported — no chunk splitting evident |
| React root cleanup bug | `innerHTML = ''` instead of `root.unmount()` |
| Preview generation blocks main thread | `exportToBlob` PNG export on every save |
| Template list hardcoded in both frontend and backend | Adding template requires code changes in two places |
| No admin-configurable templates | |
| `excalidrawInstance` typed as `any` | |

### C.8 Decisions

| Risk | Detail |
|------|--------|
| `window.prompt()` for title | Inconsistent UX |
| No duplication guard | |

### C.9 Shares

| Risk | Detail |
|------|--------|
| **`shares/+page.svelte` corrupted markup** | Literal `tttttttttt` tab characters in template — likely breaks compilation/rendering |
| `ShareService::revoke_share` blocks non-owner admins | Contradicts recent non-owner admin group share creation |
| `ShareService::update_share` same issue | |
| `public_shares.rs` upload has no file size validation | |
| Access logging errors are warnings only | Silent audit data loss |
| `sessionToken` reactive assignment runs every cycle | `share/[token]/+page.svelte:78-80` |
| `shares.ts` public share functions use raw `fetch()` | Hardcoded `localhost:8080` fallback; misses interceptors |
| `ShareModal` 884 lines with 6 inline mutations | Monolithic; `activeTab` cast `('share' as any)` |
| `recipientPermissionDrafts` manual state | Error-prone |
| All 8 share contract tests `#[ignore]` | Zero CI coverage for core sharing logic |
| Group sharing tests 75% placeholder | 6 of 8 ignored |
| Frontend zero share component tests | `ShareModal`, `ShareIndicator`, `SharesModuleView` untested |
| `user_share_service.rs` deprecated but active | `#![allow(deprecated)]` in handler |

---

## D. Recommended Implementation Sequence

Use this order unless a blocking dependency is discovered:

| Phase | Task | Why First |
|-------|------|-----------|
| **1** | **Global metadata visibility** | Blocks all module polish — `index.md`, `__primary__.md`, and artifact folders leak internals everywhere |
| **2** | **Create-and-open behavior** | Unifies the most user-visible inconsistency (prompts + Shares not opening) |
| **3** | **Shared modals** | Replaces all native prompts/confirms; needed by phases 4+ |
| **4** | **Workspace root / folder navigation** | Fixes breadcrumb duplication, silent null failures, "Open in Files" |
| **5** | **Folders** | Central file explorer — biggest page, most surface area |
| **6** | **Notes** | Core module; fix broken back nav, unify routes |
| **7** | **Meeting Notes** | Replace prompt with modal |
| **8** | **Standup Records** | Replace prompt + fix broken detail page |
| **9** | **Decisions** | Replace prompt with modal |
| **10** | **Kanban** | Biggest module component; split + polish |
| **11** | **Brainstorming** | Fix autosave loss, cleanup Excalidraw |
| **12** | **Shares** | Fix corrupted markup, permission inconsistency, test gaps |
| **13** | **Home** | Dashboard polish — lowest risk, last |
| **14** | **Wording / empty states** | Copy sweep across all pages |
| **15** | **Final QA** | End-to-end verification |

### Phase Detail

#### Phase 1 — Global Metadata Visibility
- Add `index.md`, `__primary__.md` to backend `is_hidden_file()` filter
- Add `*.editor.json` to backend hidden-file filter
- Add hidden-file filtering to search handlers (`search.rs`, `search_service.rs`)
- Audit and remove all user-facing UUID/internal ID exposure
- Hide artifact package folder structures from "Open in Files"

#### Phase 2 — Create-and-Open Behavior
- Build unified `CreateArtifactModal` (name input, optional template selection)
- Replace all `window.prompt()` create flows with modal
- Make Shares module open created artifact after create
- Add duplication guards (name collision check before create)

#### Phase 3 — Shared Modals
- Create `ConfirmDialog` component (replaces all `window.confirm()`)
- Create `AlertToast` utility (replaces all `window.alert()`)
- Update `ModalBase` focus trap
- Migrate DaisyUI modals to `ModalBase` where feasible

#### Phase 4 — Workspace Root / Folder Navigation
- Consolidate breadcrumb systems into one (explorer store)
- Fix `resolveModuleFolderId()` silent null — show error or create folder
- Remove legacy `Sidebar.svelte` dead code
- Unify "Open in Files" behavior across all modules

#### Phase 5 — Folders
- Split `files/+page.svelte` into sub-components
- Move search to server-side or indexed
- Fix drag-drop edge cases
- Remove `index.md`/`__primary__.md` from visible listings

#### Phase 6 — Notes
- Deprecate `/notes/[id]` route, redirect to `/modules/notes/[id]`
- Fix broken `goto('/notes')` back navigation
- Add empty state + create CTA

#### Phase 7 — Meeting Notes
- Replace `window.prompt()` with `CreateArtifactModal`
- Add empty state

#### Phase 8 — Standup Records
- Replace `window.prompt()` with `CreateArtifactModal`
- Fix `modules/[key]/[id]/+page.svelte` to include `standups` in API switch
- Add empty state

#### Phase 9 — Decisions
- Replace `window.prompt()` with `CreateArtifactModal`
- Add empty state

#### Phase 10 — Kanban
- Split `KanbanModuleView.svelte` into sub-components
- Replace native confirms with `ConfirmDialog`
- Fix activity feed human-readable strings
- Add keyboard shortcuts
- Backend: paginate card loading, cap `events.jsonl`

#### Phase 11 — Brainstorming
- Add `beforeunload` guard for unsaved changes
- Fix React root cleanup (`root.unmount()`)
- Debounce preview generation or move off main thread
- Add empty state

#### Phase 12 — Shares
- Fix corrupted `shares/+page.svelte` markup
- Fix `revoke_share` / `update_share` to allow non-owner admins
- Replace native confirms with `ConfirmDialog`
- Add share component tests
- Un-ignore and fix contract tests
- Add file size validation to public upload

#### Phase 13 — Home
- Polish widget empty states
- Ensure all module widgets handle missing data gracefully

#### Phase 14 — Wording / Empty States
- Copy sweep: replace developer-facing strings
- Empty state illustrations + CTAs for all module views

#### Phase 15 — Final QA
- Full E2E smoke test of all create flows
- Verify no `window.prompt`/`confirm`/`alert` remain
- Verify metadata files are hidden everywhere
- Cross-browser check

---

## E. Suggested Tests / Checks for Each Phase

| Phase | Verification |
|-------|-------------|
| **1. Metadata visibility** | Backend: unit test `is_hidden_file()` covers `index.md`, `__primary__.md`, `*.editor.json`. Search handler test: hidden files excluded from results. Frontend: visual check — open artifact folder in Files, verify no internal files shown. |
| **2. Create-and-open** | E2E: create each artifact type, verify navigation to correct route. Verify Shares opens after create. Verify no duplicate names allowed. |
| **3. Shared modals** | Unit: `ConfirmDialog` renders, emits confirm/cancel. Component: verify no `window.confirm`/`alert`/`prompt` in built bundle (`grep -r "window\." dist/`). |
| **4. Workspace root** | E2E: click "Open in Files" from each module, verify correct folder. Breadcrumb test: navigate deep folder tree, verify single source of truth. |
| **5. Folders** | E2E: upload, move, rename, delete, drag-drop. Performance: search with 1000+ files. Visual: no internal metadata files in listings. |
| **6. Notes** | E2E: create note, edit, autosave, back navigation. Unit: route redirect from `/notes/[id]` to `/modules/notes/[id]`. |
| **7. Meetings** | E2E: create meeting, verify modal (not prompt), verify navigation. |
| **8. Standups** | E2E: create standup, open detail page, verify it loads (regression for broken API switch). |
| **9. Decisions** | E2E: create decision, verify modal (not prompt). |
| **10. Kanban** | E2E: create board, add card, drag card, archive card. Unit: activity feed produces human-readable strings. Performance: board with 50+ cards loads <2s. |
| **11. Brainstorming** | E2E: create board, draw, navigate away without saving — verify `beforeunload` fires. Unit: React root cleanup on unmount. |
| **12. Shares** | E2E: create public share, access via token, revoke. Contract: un-ignore and pass all 8 share contract tests. Unit: `ShareModal` renders both tabs. |
| **13. Home** | E2E: dashboard loads with all widgets. Visual: empty states render correctly. |
| **14. Wording** | Manual/copy review: no raw event types, no developer strings. Accessibility: run axe-core or similar. |
| **15. Final QA** | Full E2E suite. Bundle analysis: no `window.prompt`/`confirm`/`alert`. Lighthouse score check. Cross-browser (Chrome, Firefox, Safari). |

---

## Appendix: Quick Reference — Files to Touch by Phase

| Phase | Primary Files |
|-------|--------------|
| 1 | `backend/crates/core/src/services/folder_service.rs`, `backend/server/src/handlers/search.rs`, `backend/crates/core/src/services/search_service.rs`, `frontend/src/lib/components/modules/*ModuleView.svelte` |
| 2 | `frontend/src/lib/components/modules/MeetingsModuleView.svelte`, `StandupsModuleView.svelte`, `DecisionsModuleView.svelte`, `SharesModuleView.svelte`, `GenericModuleView.svelte`, `KanbanModuleView.svelte` (rename) |
| 3 | `frontend/src/lib/components/common/ModalBase.svelte`, new `ConfirmDialog.svelte`, `AlertToast.svelte`, all files with `window.confirm`/`alert`/`prompt` |
| 4 | `frontend/src/lib/modules/modulePages.ts`, `frontend/src/lib/explorer/store.svelte.ts`, `frontend/src/routes/(app)/files/+page.svelte`, `frontend/src/lib/layout/SidebarNav.svelte`, `frontend/src/lib/components/layout/Sidebar.svelte` (delete) |
| 5 | `frontend/src/routes/(app)/files/+page.svelte` (split), `frontend/src/lib/layout/Topbar.svelte` |
| 6 | `frontend/src/routes/(app)/notes/[id]/+page.svelte`, `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` |
| 7 | `frontend/src/lib/components/modules/MeetingsModuleView.svelte` |
| 8 | `frontend/src/lib/components/modules/StandupsModuleView.svelte`, `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` |
| 9 | `frontend/src/lib/components/modules/DecisionsModuleView.svelte` |
| 10 | `frontend/src/lib/components/modules/KanbanModuleView.svelte` (split), `backend/server/src/services/kanban_service.rs` |
| 11 | `frontend/src/routes/(app)/modules/brainstorming/[boardId]/+page.svelte`, `frontend/src/lib/editor/components/ExcalidrawEditor.svelte` |
| 12 | `frontend/src/routes/(app)/shares/+page.svelte`, `frontend/src/lib/components/modals/ShareModal.svelte`, `backend/crates/core/src/services/share_service.rs`, `backend/tests/contracts/share_link_contract.rs`, `backend/tests/group_sharing_test.rs` |
| 13 | `frontend/src/routes/(app)/dashboard/+page.svelte`, dashboard widgets |
| 14 | All module views, all pages with empty states |
| 15 | All of the above + E2E test suite |
