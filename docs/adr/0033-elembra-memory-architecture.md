# ADR-0033: Elembra Memory Architecture

Status: Proposed  
Date: 2026-08-07  
Related: issue #119

## Context

The current RustShare RAG direction correctly requires structured memory objects, hybrid search, citations and permission filtering. However, treating `memory`, `index`, `vector store`, `RAG`, and `AI` as one subsystem would create a new platform god-object and blur authoritative source ownership.

Elembra must preserve business memory across Files, Notes, Mail, Chat, Connectors and later Applications without turning the Memory database into a second authoritative copy of every source.

## Decision

**Elembra Memory is a first-party Application/platform service separate from Platform Core.** It is divided into three conceptual layers with different correctness requirements:

1. **Memory Catalog** — durable provenance/reference records.
2. **Index/Search** — rebuildable retrieval projections.
3. **Retrieval/RAG** — permission-aware source materialization and answer generation.

## 1. Memory Catalog

The catalog stores the existence, provenance and discoverability of useful business memory.

A catalog record includes, as applicable:

```text
memory_record_id
tenant_id
workspace_id
source_application
source_type
source_ref (ResourceRef)
source_version
actor/author PrincipalRef
occurred_at
observed_at
checksum/fingerprint
classification
retention_policy_ref
legal_hold_ref
provenance
indexing status
source authorization owner
```

The catalog should prefer references over copied source content.

It may retain enough immutable metadata to explain that a historical record existed even after source deletion where retention/audit policy permits, but must not leak deleted/revoked content.

## 2. Index/Search

Indexes are projections and must be rebuildable.

Initial implementation:

- PostgreSQL metadata/index state;
- PostgreSQL full-text search;
- pgvector embeddings;
- parser/chunker per source type;
- optional reranker abstraction only when quality measurements justify it.

An index row/chunk may include:

- Memory Catalog record ID;
- ResourceRef and source version;
- chunk locator (heading/page/message/event position);
- normalized text or owner-authorized derived text;
- embedding;
- structured metadata;
- coarse ACL projection/hash/version;
- indexing model/parser versions.

The original source remains authoritative.

## 3. Retrieval/RAG

Canonical query flow:

```text
PrincipalContext + query + scope
        │
        ▼
Hybrid candidate retrieval
(tenant/workspace + coarse ACL + metadata + keyword + vector)
        │
        ▼
Source Application batch authorization
        │
        ▼
Drop denied/stale/deleted candidates
        │
        ▼
Fetch authorized source representations
        │
        ▼
Rerank / context assembly
        │
        ▼
LLM generation
        │
        ▼
Answer + ResourceRef citations + provenance + audit
```

No source content that fails the source Application's current authorization may be supplied to the model.

## Publication patterns

Applications publish memory in one of two ways.

### Reference-first publication

Preferred for independently authoritative systems such as Buzz Chat, Mail and external Connectors.

Application publishes:

- ResourceRef/version;
- safe metadata/provenance;
- indexing hint/source type;
- optional content hash;
- authorization owner.

Memory later fetches authorized content through the source contract.

### Artifact-backed publication

For durable content intentionally stored as an Elembra Files artifact, the Memory record can reference that immutable artifact/version directly. Files remains the authorization/source authority.

## Chat policy

Do not automatically make the Memory database a full duplicate Buzz event store.

Default Chat projection stores:

- Buzz event reference/id;
- tenant/workspace/community/channel context;
- mapped author Principal;
- timestamp/type/classification;
- checksum/signature/provenance metadata;
- authorization owner/reference.

Materialize/index message content according to workspace policy and search/retention needs. Preserve the ability to trace every result to the signed Buzz source.

Edits, tombstones, deletions, channel removal, retention and legal hold require explicit projection rules; an immutable signed event history must not be falsely modeled as an ordinary mutable SQL row.

## Mail policy

Mail keeps IMAP/SMTP/account/thread state in Mail. Durable EML/attachments may be stored as Files artifacts where archival policy chooses. Memory records point to the Mail message or archived artifact with provenance.

## Shell/Connector policy

Connectors perform source-sensitive filtering/redaction before publication where required. Shell secret redaction therefore happens locally before sync in addition to server-side validation. Memory does not become a reason to upload data that the Connector policy says must remain local.

## Permission projection

Coarse ACL/index projections are permitted to improve candidate filtering.

Required properties:

- tenant/workspace always present;
- projection has an authorization version/hash where practical;
- malformed/missing/stale ACL projection fails closed or forces source authorization before candidacy;
- permission/group/share changes trigger reindex/projection invalidation via durable events;
- source reauthorization still occurs before LLM materialization.

## Model/provider isolation

Embedding and LLM provider concerns are behind explicit provider interfaces. They do not define the Memory domain model.

Memory records and citations must remain useful if:

- embedding model changes;
- vector store changes;
- LLM provider changes;
- AI is disabled entirely.

Sovereign Business Memory is therefore not synonymous with RAG.

## AI-disabled behavior

Without LLM/embeddings, Elembra Memory should still provide:

- catalog/provenance;
- metadata browsing;
- exact/keyword search;
- source links;
- retention/audit integration.

This is an important sovereignty and operational property.

## Failure/rebuild model

- Source Applications continue working when Memory is down.
- Publication events remain in outbox/retry queues.
- Indexes can be rebuilt from catalog + source APIs.
- A failed embedding/index task records state/error and is retryable.
- Search result opening always goes to/re-authorizes with the source.

## Consequences

### Positive

- Memory is durable without stealing source ownership.
- RAG security is enforceable at the source boundary.
- Index/storage/model choices remain replaceable.
- Chat/Mail/external sources can participate without database merging.
- Memory remains valuable without AI.

### Negative

- Source authorization/fetch APIs are required.
- Eventual consistency means index freshness is observable rather than magically transactional.
- Rebuild/reconciliation tooling must be implemented.

## Rejected alternatives

### Copy all source data into one `company_memory` database and authorize there

Rejected. It creates a second source of truth, stale permissions and complex deletion/retention divergence.

### Vector database as the memory model

Rejected. Embeddings/chunks are derived indexes, not business records or provenance.

### Permission filtering after generation

Rejected as a security failure.

### Put Memory in Platform Core

Rejected. Parsing, indexing, embeddings, retrieval and AI provider concerns are Application/service logic, not tenant/identity foundation.

## Acceptance criteria

- [ ] Catalog, Index/Search and Retrieval/RAG are separate interfaces/components.
- [ ] Catalog records use ResourceRefs/provenance.
- [ ] Index is explicitly rebuildable.
- [ ] Source Applications remain authoritative for access.
- [ ] Source batch reauthorization occurs before LLM context assembly.
- [ ] Answers cite ResourceRefs/source locations.
- [ ] AI access produces auditable source-access records.
- [ ] Memory can operate in non-LLM keyword/catalog mode.
- [ ] Chat integration does not require duplicating Buzz as a second authoritative event database.
