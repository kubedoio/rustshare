# Permission-Aware Unified Search v1 — Implementation Plan

Branch: `codex/buzz-source-authorization`
Date: 2026-08-11
Status: executing

## Goal

One query returns ranked Files/Notes and Buzz Chat results, where **every
returned/materialized result is authorized by its CURRENT owning source**.
Search indexes and Memory records are never authorization.

Non-goals: RAG/LLM answers, Agents, Mail, frontend work, external search
infrastructure, new vector DB, new authorization cache, new Memory Catalog.

## Reused foundations (do not duplicate)

- `rustshare-resource-auth` — `SourceAuthorizer` (batch authorize / resolve /
  fetch), `ResourceRef`, `PrincipalContext`, `Decision`, `Candidate`,
  `Purpose::SearchPreview`, `FILES_READ`, `CHAT_READ`, `MAX_BATCH_SIZE`.
- `FilesResourceOwner` / `ChatResourceOwner` (already registered by
  `authz::build_source_authorizer`; Chat owner defers the final decision to
  the configured `BuzzAuthority` — never consult `memory_catalog` for
  authorization).
- `AiService::semantic_search` → `ContentIndexer::search_with_acl` →
  `VectorStore` (pgvector cosine over `note_index_chunks`, ACL SQL pre-filter +
  Rust `can_access` + `PermissionResolver` post-filter) — the existing
  permission-aware vector path, reused as a candidate producer.
- `MemoryCatalogStore` + `memory_catalog` (Memory-owned searchable index).
- `MetadataStore` (`files` table) for name/path keyword candidates.
- New SQL must use dynamic `sqlx::query` (no `query!` macros — repo
  convention that protects the `cargo sqlx prepare --workspace --check` gate).

## Flow (hard requirements)

```
PrincipalContext + query
→ candidates (Files/Notes: name/path + note keyword + note vector;
              Chat: memory_catalog keyword)
→ dedupe by canonical ResourceRef (keep best score)
→ batch reauthorization per source action:
     Files/Notes → FILES_READ; Chat → CHAT_READ
     (SourceAuthorizer::authorize_batch, MAX_BATCH_SIZE chunks)
→ drop Deny/NotFound/Invalid; per-candidate failures never fail the search
→ final deterministic rank → take top `limit`
→ per final result: resolve() (SearchPreview) then fetch() Text → authorized
  snippet (truncate); fetch/resolve failure drops only that item
→ return normalized result contract with ResourceRef + provenance
```

Fail closed: source authorization unavailable/ambiguous for a candidate ⇒ that
candidate is dropped. Buzz outage ⇒ Chat candidates denied (Chat owner fails
closed), Files results still return. A stale/malicious index hint is never
returned as a snippet; snippets come only from the authorized fetch.

## Workstream 1 — Core: keyword search on the note index (coder subagent A)

Files:

1. `backend/crates/core/src/services/ai/vector_store.rs`
   - Add `async fn keyword_search_with_acl(&self, principal:
     &RetrievalPrincipal, query: &str, limit: usize) ->
     anyhow::Result<Vec<(IndexedDocument, f32)>>` to the `VectorStore` trait.
   - Add a pure `pub fn keyword_score(doc: &IndexedDocument, query: &str) ->
     f32` used by both impls (occurrence-based: count query-term occurrences
     in `file_name` (weighted 2.0), `file_path` (1.0), `content` (1.0),
     normalized to (0,1]; 0.0 when no term matches). Must be deterministic.
   - Implement for `InMemoryVectorStore`: reuse `can_access` filtering
     (same as `search_with_acl`), score via `keyword_score`, filter score > 0,
     sort score desc then `file_id`, truncate to `limit`.
2. `backend/crates/infrastructure/src/vector/pg_vector_store.rs`
   - Implement `keyword_search_with_acl` with dynamic sqlx, mirroring
     `search_with_acl`'s SQL pre-filter but adding
     `AND (content ILIKE '%'||$3||'%' OR file_name ILIKE '%'||$3||'%' OR
     file_path ILIKE '%'||$3||'%')`; ORDER BY name-match DESC, id; LIMIT.
     After fetching rows, run `validate_chunk_acl` + `can_access` (exact
     enforcement, as today), compute score with the shared `keyword_score`.
3. `backend/crates/core/src/services/ai/indexing.rs`
   - Add `ContentIndexer::keyword_search_with_acl(&self, principal, query,
     limit) -> Vec<(IndexedDocument, f32)>` calling the store; on store error
     log and return `Vec::new()` (mirror `search_with_acl`).
4. `backend/crates/core/src/services/ai_service.rs`
   - Add `AiService::keyword_search(&self, query, user_id, tenant_id, limit)
     -> Result<Vec<SemanticSearchResult>, AiError>` mirroring
     `semantic_search` (same validation, group-id resolution, principal
     construction, hidden-metadata filter, `PermissionResolver` post-filter,
     snippet truncation). Refactor the common tail of `semantic_search` /
     `keyword_search` into a private helper to avoid duplication.
5. Tests: unit tests for `keyword_score`; InMemory keyword ACL tests
   (allowed owner, denied stranger, stale acl_version, cross-tenant); extend
   `backend/tests/ai_vector_store_permission_contract.rs` with a keyword
   section in `run_permission_contract` (both backends) if straightforward.

## Workstream 2 — Storage: candidate queries (coder subagent B)

1. `backend/crates/storage/src/metadata.rs`
   - Add `pub async fn search_files_by_name_path(&self, tenant_id: Uuid,
     query: &str, limit: usize) -> anyhow::Result<Vec<File>>` on
     `MetadataStore`, dynamic sqlx:
     `SELECT ... FROM files WHERE tenant_id = $1 AND deleted_at IS NULL AND
     (name ILIKE '%'||$2||'%' OR path ILIKE '%'||$2||'%') ORDER BY (name
     ILIKE $2) DESC, (name ILIKE '%'||$2||'%') DESC, path LIMIT $3`.
     Escape `%`, `_`, `\` in the pattern (ILIKE). Must NOT apply permission
     filtering (candidates only; SourceAuthorizer is the final gate).
2. `backend/crates/storage/src/memory_catalog.rs`
   - Add `pub async fn search(&self, tenant_id: TenantId, query: &str,
     limit: usize) -> anyhow::Result<Vec<MemoryCatalogRecord>>`:
     `SELECT {RECORD_COLUMNS} FROM memory_catalog WHERE tenant_id = $1 AND
     indexing_status <> 'tombstoned' AND (content ILIKE ... OR message_id =
     $3 OR author_pubkey = $3 OR channel_id ILIKE ...) ORDER BY occurred_at
     DESC, message_id LIMIT $4`. Escape ILIKE wildcards. Reuse
     `RECORD_COLUMNS` + `row_to_record`.

## Workstream 3 — Server: unified search service + handler + route (coder subagent C)

1. `backend/server/src/services/unified_search.rs` (new)
   - `UnifiedSearchService { authorizer: Arc<SourceAuthorizer>,
     metadata: Arc<MetadataStore>, ai: Option<Arc<AppAiService>>,
     memory_catalog: Arc<MemoryCatalogStore> }`.
   - Internal candidate struct carrying: source application, source type,
     `ResourceRef`, retrieval score, title, location, occurred_at, cached
     hint (NEVER emitted), per-source provenance payload, note/file ids.
   - Pure, unit-tested helpers:
     - `is_hidden_file_name(name) -> bool` (`.rustshare*`, `events.jsonl`,
       `index.md`, `__primary__.md`, `*.editor.json` — same list as
       `AiService`).
     - `dedupe_and_rank(candidates, limit) -> Vec<...>` — dedupe by
       canonical `ResourceRef` keeping max score; sort (score desc,
       occurred_at desc, source_application, resource_ref) — deterministic.
     - `authorized_snippet(fetched: &[u8], query, max_chars) -> String` —
       find first query-term occurrence, window around it, truncate with
       ellipsis; UTF-8 safe.
   - `pub async fn search(&self, ctx: &PrincipalContext, query: &str,
     sources: &[SearchSource], limit: usize) -> Result<UnifiedSearchResponse,
     UnifiedSearchError>`:
     - validate query (non-empty, ≤ 1000 chars);
     - collect candidates per source (Files: name/path always; note keyword +
       vector when `ai` is Some; Chat: `memory_catalog.search`);
     - dedupe/rank preliminarily (cap `limit * 4`);
     - split refs by action (`FILES_READ` group, `CHAT_READ` group),
       `authorize_batch` in MAX_BATCH_SIZE chunks; keep only Allow;
     - final rank → top `limit`;
     - per item: `resolve(ctx, ref, Purpose::SearchPreview)` (Err → drop),
       then `fetch(ctx, ref, Representation::Text)` → snippet; for chat
       `VersionUnavailable` → snippet None (reference-only is legal); any
       other fetch error → drop that item only;
     - build response items (never raw internal DB ids as identity; the
       cross-Application contract is the `ResourceRef` URI).
   - Error type: `InvalidQuery(String)`, `Internal(String)` — never expose
     per-candidate denials.
2. `backend/server/src/handlers/search.rs` (new)
   - `SearchRequest { query: String, limit: usize (default 10, max 50,
     clamped), #[serde(default)] sources: Option<Vec<String>> }` — accepted
     source names: `"files"`, `"chat"` (unknown → 400). Default both.
   - `SearchResultItem` (utoipa `ToSchema`): `sourceApplication`,
     `sourceType`, `resourceRef` (URI string), `title`, `snippet` (Option),
     `location` (Option), `occurredAt` (Option), `updatedAt` (Option),
     `score` (f32), `provenance` (per-source JSON object).
   - Chat provenance carries `communityId`, `channelId`, `channelKind`,
     `authorPubkey`, `messageId` (context for citation, NOT authorization).
     Files provenance carries `fileId`, `noteId`?, `mimeType`.
   - Handler `pub async fn unified_search(State(state): State<AppState>,
     auth: AuthenticatedUser, Json(req))` — build
     `PrincipalContext::user(user_id, tenant_id, tenant_id)`, call
     `state.unified_search_service.search(...)`.
   - Register in `handlers/mod.rs` (`pub mod search; pub use
     search::unified_search;`).
3. `backend/server/src/routes.rs` — `pub fn search_routes()` with
   `POST /api/v1/search`; merge in `main.rs` router.
4. `backend/server/src/state.rs` — add
   `pub unified_search_service: Arc<services::unified_search::UnifiedSearchService>`
   to `AppState`.
5. Wire the field in BOTH construction sites:
   - `backend/server/src/bootstrap.rs` (~line 900 `AppState {`): build
     `UnifiedSearchService` from the already-available authorizer, metadata
     store, ai_service option, memory_catalog_store.
   - `backend/server/tests/common/mod.rs` (the `AppState` literal): build the
     service with `SourceAuthorizer::empty()` (suites construct their own).
6. `backend/server/src/openapi.rs` — register the path + schemas
   (`unified_search`, request/response types).
7. Unit tests in `unified_search.rs` for all pure helpers.

## Workstream 4 — E2E suite (coder subagent D)

New `backend/tests/unified_search_test.rs` (DB-backed, `#[ignore]`, serialized
via a global `LazyLock<Mutex<()>>`, cleanup per test — follow
`buzz_authority_gateway_test.rs` conventions exactly, including
`--test-threads=1`).

Harness:
- Real Postgres pool; `MemoryCatalogStore`, `ChatObservationStore`,
  `ChatIdentityStore`; `MetadataStore`; `FileService` for creating files;
  `PermissionResolver` + `FilesResourceOwner`; `ChatResourceOwner` with a
  programmable in-process `BuzzAuthority` test double (Allow/Deny/NotFound/Err
  per channel), PLUS one full-stack test with the REAL `BuzzGatewayAuthority`
  against an in-test fake HTTP relay (copy the compact fake-relay pattern from
  `buzz_authority_gateway_test.rs`) proving the complete
  search → SourceAuthorizer → Chat owner → Buzz gateway → relay decision →
  snippet path.
- Note index: `ContentIndexer<SimpleEmbeddingGenerator>` over
  `InMemoryVectorStore` (AI-enabled path); `ai: None` for the no-AI path.
- Insert memory_catalog rows directly via `MemoryCatalogStore` (or via the
  observation+projection helpers copied from the gateway suite when the test
  needs the full ingest path).

Required proofs (map to the 14 from the task):
1. one query returns Files + Chat results;
2. cross-tenant candidates never appear (tenant A cannot see tenant B file or
   chat candidate);
3. revoked Files share disappears (PermissionResolver denies ⇒ dropped);
4. stale Files ACL cannot leak content (index row with a stale granting ACL;
   SourceAuthorizer denies ⇒ dropped; assert no index text in snippet);
5. revoked Chat membership removes result (authority Allow → Deny ⇒ dropped);
6. stale Chat Memory cannot override Buzz (alive catalog record + authority
   Deny ⇒ dropped);
7. deleted/tombstoned Chat content does not appear (tombstoned record not a
   candidate; Deleted observation ⇒ gate NotFound ⇒ dropped);
8. unauthorized snippets never enter response (denied candidate text never
   appears in the response body);
9. one source authorization failure does not leak or corrupt other valid
   results (mix allowed Files + denied/Err Chat ⇒ only allowed results, request
   succeeds);
10. duplicate candidates collapse deterministically (file matched by name AND
    note content ⇒ one result);
11. keyword search works without embeddings (`ai = None` ⇒ name/path + chat
    results still returned);
12. pagination/ranking deterministic (same query twice ⇒ identical order);
13. citation open path reauthorizes (returned `resourceRef` fed back into
    `SourceAuthorizer::fetch` after revocation ⇒ denied);
14. no private cross-Application DB dependency (assert the search reads only
    Files metadata/index + Memory catalog; it never reads chat observation
    tables for authorization — reviewed by construction, exercised by the
    suite).

## Workstream 5 — Docs (me or a subagent)

- `docs/specs/unified-search-v1alpha1.md` — request/response contract,
  authorization flow, ranking, fail-closed semantics, citation semantics.
- `docs/adr/0036-unified-search-permission-model.md` — architecture decision
  (candidates from Memory/index; final gate = current source authorization;
  snippets only from authorized fetch).
- `CHANGELOG.md` entry.
- Update `docs/agent-guides/*` only if a convention changed (unlikely).

## Validation before review

- `cargo fmt --all --check`
- `CARGO_INCREMENTAL=0 SQLX_OFFLINE=true cargo clippy --workspace
  --all-targets --all-features -- -D warnings`
- `set -a; . ./backend/.env; set +a; CARGO_INCREMENTAL=0 SQLX_OFFLINE=true
  cargo test --workspace --all-features --lib`
- `set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true cargo sqlx prepare
  --workspace --check`
- `set -a; . ./backend/.env; set +a; CARGO_INCREMENTAL=0 SQLX_OFFLINE=true
  cargo test -p rustshare-server --test unified_search_test --
  --ignored --test-threads=1`
- Re-run the existing 8 DB suites to prove no regression.

## Review gates

1. Spec/design review of the plan above (subagent, read-only).
2. Quality/code review of the merged implementation (subagent, read-only).
3. Fix material findings; re-run validation.
4. Adversarial architecture review (the 7 questions from the task) — fix
   material issues.

## Deliverable

Architecture decisions, contract, changed files, test results, branch/head
SHA. Do NOT push/merge — wait for explicit confirmation.
