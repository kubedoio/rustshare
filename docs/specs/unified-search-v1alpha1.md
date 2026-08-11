# Unified Search v1alpha1

Status: implemented (permission-aware, non-LLM)
Owner: Elembra Memory

## Purpose

A Principal issues one search query and receives ranked Files/Notes and Buzz
Chat results. **Every returned result is authorized by its CURRENT owning
source**; search indexes and Memory records are never authorization.

This document specifies the API contract, the authorization flow, ranking, and
the fail-closed semantics. It is a non-LLM capability: no RAG, no prompt
assembly, no external search infrastructure.

## Endpoint

```
POST /api/v1/search
```

Authentication: `AuthenticatedUser` (same as all user-facing endpoints).
Rate limited by the global middleware.

### Request

```json
{
  "query": "string",
  "limit": 10,
  "sources": ["files", "chat"]
}
```

- `query` — required, trimmed; non-empty; ≤ 1000 chars.
- `limit` — optional, default 10, clamped to `1..=50`.
- `sources` — optional; accepted values `"files"` and `"chat"`. Omitted or
  empty searches both. Unknown values → `400`.

### Response

```json
{
  "results": [
    {
      "sourceApplication": "io.elembra.files",
      "sourceType": "file",
      "resourceRef": "elembra://io.elembra.files/file/<uuid>",
      "title": "quarterly-plan.md",
      "snippet": "…the quarterly plan was approved…",
      "location": "/docs/quarterly-plan.md",
      "occurredAt": "2026-08-01T10:00:00Z",
      "updatedAt": "2026-08-01T10:00:00Z",
      "score": 0.9,
      "provenance": { "fileId": "<uuid>", "mimeType": "text/markdown" }
    },
    {
      "sourceApplication": "io.elembra.chat",
      "sourceType": "message",
      "resourceRef": "elembra://io.elembra.chat/message/<64-hex>",
      "title": "buzz message",
      "snippet": "the quarterly plan was approved",
      "location": "community:<id> / channel:<id>",
      "occurredAt": "2026-08-01T11:00:00Z",
      "updatedAt": "2026-08-01T11:00:00Z",
      "score": 0.8,
      "provenance": {
        "messageId": "<64-hex>",
        "communityId": "<id>",
        "channelId": "<id>",
        "channelKind": "workspace",
        "authorPubkey": "<64-hex>"
      }
    }
  ],
  "total": 2
}
```

Field semantics:

- `resourceRef` — the canonical `elembra://` URI; the cross-Application
  identity contract. Consumers treat it as opaque. **Opening a citation
  reauthorizes**: feeding the ref back into source authorization
  (`SourceAuthorizer::resolve`/`fetch`, or the owning Application's resource
  endpoint) re-checks CURRENT authority. No raw internal DB ids are exposed as
  the cross-Application contract.
- `snippet` — authorized content only; `null` for reference-only chat
  messages (no indexing copy) and for items with no fetchable text. Never
  derived from a stale index/Memory hint.
- `location` — Files: file path; Chat: `community:<id> / channel:<id>`
  citation context. Context only — never authorization.
- `provenance` — per-source citation metadata (never authorization).

## Authorization flow (hard rule)

```
PrincipalContext + query
→ candidate retrieval
    Files: files metadata name/path (always)
           + note index keyword + vector (when AI configured)
    Chat:  Memory catalog keyword
→ dedupe by canonical ResourceRef (keep best score)
→ batch reauthorization per owning source
    Files/Notes → io.elembra.files / files.read
                  (FilesResourceOwner → PermissionResolver)
    Chat        → io.elembra.chat / chat.read
                  (ChatResourceOwner → BuzzAuthority; never memory_catalog)
→ only Decision::Allow survives; Deny/NotFound/Invalid dropped
→ deterministic rank → top `limit`
→ per final result: resolve() then fetch(Text)
    snippet built ONLY from the authorized fetch bytes
→ normalized response contract
```

- Candidate filtering may use projected ACL metadata (the note index ACL
  projection, tenant scope) for performance. **Final result inclusion must use
  current source authorization** — this is non-negotiable.
- If source authorization is unavailable or ambiguous for a candidate, that
  candidate fails closed (dropped). A Buzz outage denies Chat candidates while
  Files results still return; one broken candidate source never fails the
  whole search and never corrupts other results.
- Snippets are generated from `Representation::Text` fetches after
  authorization. A stale or malicious index/Memory hint can never appear in a
  response.
- Cross-tenant candidates never appear: candidate queries are tenant-scoped
  and every ref is reauthorized under a `PrincipalContext` whose
  tenant/workspace scope is validated before any owner is consulted.

## Ranking

Simple and deterministic (no reranker, no external search infra):

- Files name/path: exact-name `1.0`, name-prefix `0.9`, substring in name or
  path `0.6`, else not a candidate.
- Note index: existing vector cosine similarity (`semantic_search`) and
  occurrence-based `keyword_score` (`keyword_search`).
- Chat: content term match `0.8`; message-id/author-pubkey exact or
  channel-id contains `0.4`.
- Dedupe keeps the max score per canonical `ResourceRef`.
- Final sort: score desc, occurred_at desc, source application, ref URI —
  fully deterministic for API use.
- Keyword search works without embeddings/AI providers: name/path and Chat
  candidate sources are always available; the note-index sources simply
  contribute nothing when AI is disabled.

## Fail-closed and existence-hiding

- Unauthorized and nonexistent resources are dropped, never reported as
  "denied" (existence-hiding).
- `SourceError::VersionUnavailable` on a Chat fetch (reference-only record)
  keeps the result with `snippet: null`; any other resolve/fetch failure drops
  only that item.
- `UnifiedSearchError` surfaces only `InvalidQuery`/`Internal` — per-candidate
  denials are never exposed.

## Non-goals (this iteration)

RAG/LLM answers, Agents, Mail, frontend work, external search infrastructure,
new vector database, another authorization cache, another Memory Catalog.

## Related

- ADR-0036 (permission model), ADR-0032/0033/0034/0035,
  `docs/plans/2026-08-11-permission-aware-unified-search-v1.md`.
