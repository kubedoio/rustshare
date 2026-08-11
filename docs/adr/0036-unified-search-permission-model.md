# ADR-0036: Permission-Aware Unified Search v1

Status: Accepted (implemented v1alpha1)
Date: 2026-08-11

## Context

Elembra Memory can now search across Files/Notes and Buzz Chat through one
interface (`POST /api/v1/search`). The first version is deliberately
non-LLM: keyword/full-text search, existing vector similarity, metadata and
source filters, deterministic ranking, and citations that reauthorize.

The central constraint (inherited from ADR-0032/0033/0034/0035) is that
**search indexes and Memory records are never authorization**. Candidates are
only hints; final inclusion of every result must be re-decided by the CURRENT
owning source:

- Files/Notes → `SourceAuthorizer` → `FilesResourceOwner` →
  `PermissionResolver` (owner + shares + folder ancestry, exactly as the Files
  handlers enforce);
- Chat → `SourceAuthorizer` → `ChatResourceOwner` → the configured
  `BuzzAuthority` (current membership/visibility/message availability at the
  community's authoritative relay).

Legacy note indexing already encodes user/group ACL projections directly in
`note_index_chunks` (the permission-aware indexing audit). That projection is a
**coarse candidate pre-filter only**; this ADR makes `SourceAuthorizer` the
final gate for every Files/Notes candidate too, so a stale or malicious index
projection can never grant inclusion or leak content.

## Decision

**One search endpoint returns ranked, permission-aware Files/Notes and Chat
results; snippets are built only from authorized source content; candidate
producers are never final authority.**

1. **Candidate producers** (hints, tenant-scoped, never authorization):
   - Files name/path over the `files` metadata table (always available);
   - note-index keyword (`keyword_search_with_acl`) and vector
     (`semantic_search`) over `note_index_chunks` (when AI is configured);
   - Chat over the Memory-owned `memory_catalog` (tombstoned records
     excluded). `memory_catalog` is a searchable index of references — it
     never authorizes.
2. **Final gate**: every candidate `ResourceRef` is batch-reauthorized against
   its owning source with the owning action (`files.read` / `chat.read`) in
   chunks of `MAX_BATCH_SIZE`; only `Decision::Allow` survives. A denied,
   missing, or invalid ref is dropped (existence-hiding); a source
   authorization failure fails closed for the candidates it covers and never
   fails the whole search.
3. **Snippets**: built exclusively from `Representation::Text` fetches after
   authorization. A stale or malicious index/Memory hint can never appear in a
   response. Chat messages without an indexing copy (reference-first policy)
   return with `snippet: null`.
4. **Deterministic ranking**: exact-name `1.0` / name-prefix `0.9` /
   substring `0.6` for files name/path; occurrence-based keyword scores and
   existing cosine similarity for the note index; content `0.8` / metadata
   `0.4` for chat. Dedupe by canonical `ResourceRef` keeps the max score; the
   final sort is score desc, occurred_at desc, source application, ref URI —
   stable across calls.
5. **Citations**: every result carries the canonical `elembra://` ref; opening
   a citation reauthorizes through the owning source. Chat provenance
   preserves community/channel/message context as citation context, never as
   authorization.
6. **Keyword search works without embeddings/AI providers**: name/path and
   Chat sources are always available; the note-index sources simply contribute
   nothing when AI is disabled.
7. **No new infrastructure**: reuse `SourceAuthorizer`, `ResourceRef`,
   `PrincipalContext`, the existing permission-aware note index, the Memory
   catalog, and the Buzz authority gateway. No new vector store, no new
   authorization cache, no external search infrastructure, no LLM.

## Consequences

- A stale index row that once granted access can produce a candidate but never
  a result: the owning source re-decides every time.
- Revoked chat membership removes results on the next search (the
  `ChatResourceOwner` gate and `BuzzAuthority` are unchanged and already
  immediate).
- Cross-tenant candidates cannot appear: every candidate producer is
  tenant-scoped and every ref is reauthorized under a validated
  tenant/workspace scope.
- A Buzz outage denies Chat candidates while Files results still return.
- Cost: every result triggers batch reauthorization plus per-result
  resolve/fetch — correct-first, not optimized; ranking tuning and
  fetch-minimization are follow-ups.

## Non-goals (this iteration)

RAG/LLM answers, prompt/context assembly, Agents, Mail, frontend work,
external search infrastructure, a new vector database, another authorization
cache, another Memory Catalog.

## Related

- ADR-0031 (durable integration events), ADR-0032 (ResourceRef /
  source-authorization contract), ADR-0033 (Memory architecture),
  ADR-0034 (Elembra/Buzz boundary), ADR-0035 (Buzz source-authorization
  gateway).
- `docs/specs/unified-search-v1alpha1.md`,
  `docs/plans/2026-08-11-permission-aware-unified-search-v1.md`,
  `backend/tests/unified_search_test.rs`.
