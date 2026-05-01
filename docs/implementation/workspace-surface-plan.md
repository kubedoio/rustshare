# Workspace Surface Implementation Plan

## 1. Repository Map

### Frontend (SvelteKit)
- **Dashboard**: `frontend/src/routes/(app)/dashboard/+page.svelte` and `frontend/src/lib/components/dashboard/*`
- **Sidebar Component**: `frontend/src/lib/components/layout/Sidebar.svelte`
- **Top Header Component**: `frontend/src/lib/components/layout/Header.svelte`
- **Routing System**: SvelteKit file-based routing (`frontend/src/routes/`). Dynamic module routing uses `frontend/src/routes/(app)/modules/[key]/+page.svelte`.
- **Notes Implementation**: `frontend/src/lib/components/modules/NotesModuleView.svelte` and the editor at `frontend/src/routes/(app)/notes/[id]/+page.svelte`.
- **File Browser**: Rooted at `frontend/src/routes/(app)/files/+page.svelte` along with components in `frontend/src/lib/components/files/`.
- **API Conventions**: Handled via `ApiClient` wrapper in `frontend/src/lib/api/client.ts` communicating with the backend over REST.

### Backend (Rust Workspace)
- **Storage/File Abstraction**: Implemented in `backend/crates/storage/` (e.g. `rustfs_repos.rs`, `sync/`, `metadata_v2/`).
- **Auth/Admin/Permissions**: Handled in `backend/crates/auth/` and `backend/crates/core/domain/user.rs`. Handlers leverage `AuthenticatedUser` extractors.
- **API Handlers/Services**: `backend/server/src/handlers/` and `backend/server/src/services/`.

## 2. Existing Conventions

- **File-Backed Template Modules**: Business capabilities (Notes, Kanban, etc.) are treated as specialized files and folders. Human readable content goes in `index.md`, machine state goes in `.rustshare.json`. (ADR-0016).
- **UI Contract**: Modularity is driven by a rigid UI contract for modules, managing visibility, positioning, icon registration, and dashboard summary views (ADR-0018, `module-ui-contract.md`).
- **Frontend State**: The SvelteKit frontend adopts modern Svelte 5 patterns (runes like `$state`, `$derived`).
- **Backend Architecture**: Axum router wrapping PostgreSQL logic via `sqlx`. Services orchestrate domain logic from the storage and auth crates.

## 3. Files Likely to Change

- `frontend/src/lib/components/layout/Sidebar.svelte` (Dynamic integration of enabled modules, replacing hardcoded links)
- `frontend/src/routes/(app)/dashboard/+page.svelte` (Complete restructuring to include Compact Workspace Summary, Modules Grid, and Module Summary Sections)
- `frontend/src/lib/components/dashboard/*` (Module Grid components and Summary Widgets based on `summaryMode`)
- `frontend/src/routes/(app)/modules/[key]/+page.svelte` (Central dispatcher for module renderers)
- `frontend/src/lib/api/modules.ts` (API bindings for fetching the UI configuration registry and summary providers)
- `backend/server/src/services/module_service.rs` (Logic to provide the module UI configurations and dashboard summary statistics to the frontend)

## 4. Implementation Risks

- **Data Fetching Performance**: Aggregating summary data for all enabled dashboard modules (Recent Notes, Standups, Kanban active cards) could result in N+1 database queries. A batched API or aggressive caching will be needed.
- **Fallback Resilience**: As per the specs, if a module or summary provider fails, the frontend must display a generic fallback and not crash the whole dashboard shell.
- **Icons & Asset Security**: The UI contract strictly limits icons to a predefined registry. Ensuring strict validation at the backend/frontend border so no raw SVGs/XSS vectors leak in.
- **Migration & File Layout**: Existing notes/folders must align with the new structure or the module renderers might throw errors when loading legacy file objects.

## 5. Exact Commands to Run After Each Milestone

**Frontend:**
- Type Checking: `npm run check` (from `frontend/`)
- Linting: `npm run lint` and `npm run format` (from `frontend/`)
- Unit Tests: `npm run test` (from `frontend/`)

**Backend:**
- Tests: `cargo test` (from `backend/`)
- Linting: `cargo clippy -- -D warnings` (from `backend/`)
- Formatting: `cargo fmt` (from `backend/`)
