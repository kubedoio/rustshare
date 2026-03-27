# Architecture Decision Record: Metadata Storage Refactor

**Date:** 2026-03-27  
**Status:** Accepted  
**Deciders:** RustShare Team

## Context

The current metadata storage implementation uses PostgreSQL for all metadata (files, folders, shares, versions). While this has served us well, we need to:

1. Support horizontal scaling for metadata operations
2. Reduce database load for high-frequency operations
3. Maintain consistency guarantees during the migration
4. Enable future multi-region deployments
5. Leverage object storage (RustFS/S3) which already hosts file content

## Decision

We will refactor the metadata layer to support multiple backends with a phased migration approach:

### 1. New Metadata System (metadata_v2)

A new `metadata_v2` module in the `rustshare-storage` crate provides:

- **Document-based storage**: JSON documents in RustFS/S3 instead of relational tables
- **Hierarchical object layout**: Organized by app, type, and identifier
- **Index objects**: Separate index files for efficient folder children queries
- **Event append**: Immutable event log for audit trail and eventual consistency
- **Runtime caching**: In-memory cache with automatic invalidation

### 2. Repository Pattern

Clean trait-based repository interface:

```rust
pub trait FolderRepository: Send + Sync { ... }
pub trait FileRepository: Send + Sync { ... }
pub trait FileVersionRepository: Send + Sync { ... }
pub trait ShareRepository: Send + Sync { ... }
```

Implementations:
- `Postgres*Repository` - Legacy PostgreSQL (existing)
- `RustFS*Repository` - New RustFS-based implementation
- `DualWrite*Repository` - Migration wrapper (writes both, reads one)

### 3. Backend Selection

Environment variable `RUSTSHARE_METADATA_BACKEND` controls the active backend:

| Backend | Behavior |
|---------|----------|
| `postgres` | PostgreSQL only (legacy, default) |
| `rustfs` | RustFS only (target state) |
| `dual_write` | Write to both, read from PostgreSQL |
| `rustfs_reads` | Write to both, read from RustFS |
| `localfs` | Local filesystem (development only) |

### 4. Consistency Model

**Selected:** Option A - Synchronous metadata + synchronous index updates + event append

- Metadata writes are synchronous and blocking
- Index updates are synchronous (required for folder queries)
- Event append is asynchronous fire-and-forget
- Read-after-write consistency within a single request

**Rationale:**
- Simple mental model for developers
- Strong consistency for critical operations
- Acceptable performance trade-off (2-3 object store ops vs 1)

### 5. Object Key Layout

```
apps/{app}/
  meta/
    folders/{id}.json
    files/{id}.json
    file_versions/{file_id}/{version}.json
    shares/{id}.json
    events/{YYYY}/{MM}/{DD}/{id}.json
  indexes/
    folders/{id}/children.json
```

**Benefits:**
- Hierarchical organization enables prefix-based operations
- Natural grouping for backup and lifecycle policies
- UUID-based identifiers avoid hot spots

### 6. Migration Strategy

Four-stage migration with verification at each step:

```
┌──────────┐    ┌────────────┐    ┌───────────────┐    ┌─────────┐
│ postgres │ → │ dual_write │ → │ rustfs_reads │ → │ rustfs  │
└──────────┘    └────────────┘    └───────────────┘    └─────────┘
     │                │                  │                │
     │          [verify parity]    [verify reads]   [verify ops]
     │                │                  │                │
  stable          safe rollback     read validation   target state
```

**Rollback:** At `dual_write` and `rustfs_reads` stages, can revert to `postgres` by changing environment variable.

## Consequences

### Positive

1. **Horizontal scalability**: Object storage scales independently of compute
2. **Reduced database load**: Metadata operations move off PostgreSQL
3. **Unified storage**: File content and metadata in same system
4. **Multi-region ready**: Object storage replication enables future geo-distribution
5. **Clean abstraction**: Repository pattern enables future backend changes

### Negative

1. **Migration complexity**: Four-stage migration requires careful execution
2. **Temporary storage overhead**: `dual_write` stage doubles storage
3. **Operational complexity**: More components to monitor and debug
4. **Learning curve**: New patterns for developers to understand

### Mitigations

- Comprehensive verification tools (`verify-metadata.sh`)
- Admin endpoints for health checking and stats
- Automated parity checking between backends
- Clear rollback procedures at each stage

## Related Documents

- [Metadata Refactor Design](2026-03-27-metadata-refactor-design.md)
- [Repository Pattern Guide](../backend/crates/storage/src/repos/README.md)
- [Migration Runbook](../scripts/README.md)

## References

- [ADR-001: Object Storage for File Content](adr-001-object-storage.md) - Previous storage decision
- [S3 Consistency Model](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html#ConsistencyModel)
- [Event Sourcing Pattern](https://martinfowler.com/eaaDev/EventSourcing.html)
