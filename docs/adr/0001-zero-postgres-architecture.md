# ADR 0001: Zero-PostgreSQL Architecture

## Status

**Accepted**

## Context

RustShare was originally built with PostgreSQL as the primary metadata store. As the system evolved, we developed a new metadata layer (`metadata_v2`) using RustFS (S3-compatible object storage) as the durable store. The system was in a transitional state with dual-write capabilities but still required PostgreSQL to function.

The goals of this architecture change are:

1. **Eliminate PostgreSQL dependency** - Remove all runtime dependencies on PostgreSQL
2. **Simplify deployment** - Run with just rustshare + rustfs (optionally + redis)
3. **Maintain horizontal scalability** - Support distributed deployments with Redis coordination
4. **Preserve data durability** - Keep RustFS as the canonical source of truth
5. **Support both standalone and distributed modes** - Single-node and multi-node deployments

## Decision

We will implement a zero-PostgreSQL architecture with the following characteristics:

### 1. State Classification

All state is explicitly classified into three categories:

**Canonical (RustFS)** - Durable source of truth that survives restarts:
- User accounts
- File/folder metadata
- Shares
- Device tokens
- System configuration
- Job definitions
- Events (append-only)

**Derived (RustFS)** - Rebuildable projections:
- Folder children indexes
- User notification indexes
- Job queue indexes
- Thumbnail metadata

**Ephemeral (Memory or Redis)** - Reconstructible coordination state:
- Session revocation cache
- Rate limiting counters
- Job claims (leases)
- WebSocket presence
- Idempotency keys

### 2. Runtime Profiles

Two explicit runtime profiles:

**Standalone** (`RUSTSHARE_RUNTIME_PROFILE=standalone`):
- Components: rustshare + rustfs
- Coordination: In-memory
- Use case: Single-node deployments, development

**Distributed** (`RUSTSHARE_RUNTIME_PROFILE=distributed`):
- Components: rustshare (×N) + rustfs + redis
- Coordination: Redis-backed
- Use case: Production, horizontal scaling

### 3. New Components

**CoordinationStore** (`rustshare_storage::coordination`):
- Trait-based abstraction for coordination primitives
- `InMemoryCoordinationStore` for standalone
- `RedisCoordinationStore` for distributed

**SessionManager** (`rustshare_storage::session`):
- JWT-based stateless sessions
- Optional revocation cache
- No database required

**RustFS Repositories**:
- `UserRepository` - User CRUD with email/username indexes
- `NotificationRepository` - Notification projection
- `JobRepository` - Job queue with priority ordering

### 4. Configuration Model

```bash
# Profile selection
RUSTSHARE_RUNTIME_PROFILE=standalone|distributed

# Redis (required for distributed)
RUSTSHARE_REDIS_ENABLED=true
RUSTSHARE_REDIS_URL=redis://localhost:6379

# RustFS (required)
RUSTFS_ENDPOINT=http://localhost:9000
RUSTFS_REGION=us-east-1
RUSTFS_BUCKET=rustshare
```

## Consequences

### Positive

1. **Simplified deployment** - No PostgreSQL container or management
2. **Reduced operational complexity** - Fewer services to monitor
3. **Horizontal scalability** - Easy to add more rustshare instances
4. **Consistent storage model** - All durable state in RustFS
5. **Flexible deployments** - Standalone for simple use cases, distributed for scale

### Negative

1. **Session revocation in standalone mode** - Revocation doesn't persist across restarts
2. **No SQL queries** - Must use object store patterns (indexes, scans)
3. **Migration effort** - Existing PostgreSQL data must be migrated
4. **New failure modes** - Redis unavailability affects distributed coordination

### Mitigations

1. **Session revocation** - Document limitation; use distributed mode for strict requirements
2. **Query patterns** - Build and maintain indexes in RustFS
3. **Migration** - Provide dual-write and verification tooling
4. **Redis failures** - Graceful degradation; jobs can be re-claimed after lease expiry

## Implementation

### Phase 1: Core Abstractions
- [x] CoordinationStore trait and implementations
- [x] SessionManager for JWT-based auth
- [x] New schemas for users, jobs, notifications

### Phase 2: Repositories
- [x] UserRepository with RustFS backend
- [x] NotificationRepository with projector
- [x] JobRepository with coordinator

### Phase 3: Server Integration
- [ ] Update AppState to remove PgPool
- [ ] Implement runtime profile detection
- [ ] Update handlers to use new repositories

### Phase 4: Migration Support
- [ ] Dual-write verification tools
- [ ] Data migration scripts
- [ ] Rollback procedures

## References

- `docs/ZERO_POSTGRES_ARCHITECTURE.md` - Full architecture specification
- `docs/ZERO_POSTGRES_CONCERN_MAP.md` - Per-concern state classification
- `backend/crates/storage/src/coordination/` - Coordination store implementation
- `backend/crates/storage/src/repos/` - Repository implementations

## Decision Date

2026-03-27

## Decision Makers

Principal Systems Architect
