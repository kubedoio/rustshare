# Notes keyword search without AI (M3.2)

## Audit

Unified Search's Files name/path and Chat catalog candidates are independent
of `AppAiService`. Note-body candidates are not: `UnifiedSearchService` calls
`AiService::keyword_search`, while `AppAiService` is constructed only when
`ai_enabled` is true. Bootstrap also creates the `NoteIndexSink` only from
that AI service's `ContentIndexer`, so NoteService writes and M3.1 repairs have
no writer when AI is disabled. The LLM provider is optional, but bootstrap
currently constructs it independently of the AI switch.

The existing `note_index_chunks` table is the only durable Files/Notes search
projection. It stores body text, ACL projection, and a required pgvector
embedding. PostgreSQL keyword lookup uses bounded `ILIKE` predicates over
content/name/path; there is no Notes `tsvector` or GIN full-text index. The
keyword method itself does not generate embeddings, but its row decoder
expects a non-null vector and the SQL filters on `embedding_policy =
'allowed'`.

The indexed ACL projection is only a candidate filter. Unified Search
reauthorizes each Files ResourceRef with the existing `SourceAuthorizer`, then
resolves and fetches authorized source bytes for result details. RAG
materialization performs another authorization/fetch pass. This contract
remains unchanged.

## Design decision

Keep one `note_index_chunks` projection and one current Files authorization
path. Make the embedding column nullable so an AI-disabled Notes index can
store the existing content and ACL fields without constructing an embedding
generator. Keep keyword lookup on PostgreSQL and use the existing deterministic
keyword scorer. Vector search ignores rows with no embedding.

Bootstrap always provides the shared PostgreSQL index store and keyword
indexer. With AI enabled, NoteService continues to use `ContentIndexer` to
write a body, ACL projection, and embedding into that same table. With AI
disabled, NoteService uses the keyword-only writer, which writes the same body
and ACL projection with a null embedding. The keyword service is also used by
reconciliation and lifecycle cleanup so it remains available without AI.

The existing `embedding_policy` field is more restrictive in practice than
its name suggests: it currently controls whether *any* note body is retained
in `note_index_chunks`, not only whether a vector is generated. M3.2 preserves
that conservative behavior: `allowed` permits keyword indexing regardless of
the AI switch; `denied` or malformed policy blocks content indexing and
removes existing chunks. Existing NoteService behavior treats an absent policy
as `allowed`; M3.2 preserves that default. No independent keyword permission
is introduced or inferred.

Keyword rows remain candidates only. The Files owner remains the sole source
of truth for existence, ownership, visibility, and current permission. The
index's tenant/ACL fields are coarse pre-filters; every returned result and
every RAG source is reauthorized against Files. Reconciliation reads current
Files rows and content and can repair/remove keyword-only rows through the
same writer.

## Out of scope

No second Notes index, new authorization mechanism, content parser, embedding
provider, LLM provider, or change to Files/Chat search contracts. PostgreSQL
keyword matching remains the existing `ILIKE`/occurrence-score behavior; a
dedicated Notes full-text search vector is not added in this slice.
