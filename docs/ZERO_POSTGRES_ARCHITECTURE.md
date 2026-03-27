# RustShare Zero-Postgres Architecture

## Executive Summary

This document defines the target zero-Postgres architecture for RustShare. The system will run with:
- **RustShare** (application)
- **RustFS** (durable system of record)
- **Redis** (optional, for distributed coordination only)

PostgreSQL is completely removed from the runtime architecture.

---

## Current State Analysis

### PostgreSQL Dependencies (to be removed)

| Component | Current Storage | Usage Pattern | Migration Target |
|-----------|----------------|---------------|------------------|
| `users` table | PostgreSQL | CRUD, lookup by email/username | RustFS canonical |
| `user_sessions` table | PostgreSQL | Session token storage, lookup by hash | Redis ephemeral |
| `folders` table | PostgreSQL | Hierarchical data | RustFS canonical (exists in metadata_v2) |
| `files` table | PostgreSQL | File metadata | RustFS canonical (exists in metadata_v2) |
| `file_versions` table | PostgreSQL | Version history | RustFS canonical (exists in metadata_v2) |
| `shares` table | PostgreSQL | Share metadata | RustFS canonical (exists in metadata_v2) |
| `notifications` table | PostgreSQL | User notifications | RustFS derived projection |
| `replication_jobs` table | PostgreSQL | Job queue | RustFS canonical + ephemeral coordination |
| `replication_targets` table | PostgreSQL | Target config | RustFS canonical |
| `oidc_login_states` table | PostgreSQL | OIDC flow state | Redis ephemeral |
| `device_pair_requests` table | PostgreSQL | Device pairing | Redis ephemeral |
| `device_tokens` table | PostgreSQL | Long-lived tokens | RustFS canonical |
| `user_security_events` table | PostgreSQL | Audit log | RustFS derived (event log) |
| `admin_actions` table | PostgreSQL | Admin audit | RustFS derived (event log) |
| `share_access_log` table | PostgreSQL | Access audit | RustFS derived (event log) |
| `webhook_configs` table | PostgreSQL | Webhook settings | RustFS canonical |
| `oidc_config` table | PostgreSQL | OIDC settings | RustFS canonical |
| `smtp_config` table | PostgreSQL | SMTP settings | RustFS canonical |
| `user_groups` table | PostgreSQL | Group management | RustFS canonical |
| `user_group_members` table | PostgreSQL | Membership | RustFS canonical |
| `file_thumbnails` table | PostgreSQL | Thumbnail metadata | RustFS derived |

### SQLx Usage Count

- Total `sqlx::` references: ~612 across backend codebase
- Main files affected:
  - `server/src/main.rs` - Pool initialization, migrations
  - `storage/src/metadata.rs` - ~1800 lines of SQL queries
  - `storage/src/event_store.rs` - Event sourcing storage
  - `infrastructure/src/repositories/*.rs` - All repositories
  - Domain models with `#[derive(sqlx::FromRow)]`

---

## Target Architecture

### State Placement Rules

#### 1. RustFS Canonical Metadata (Durable Truth)

These are the source of truth and must survive restarts:

- **User accounts** - Username, email, password_hash, profile, quotas
- **Files** - File head documents
- **Folders** - Folder hierarchy documents  
- **File versions** - Immutable version snapshots
- **Shares** - Share documents (public and user-to-user)
- **Tombstones** - Soft-delete records
- **Durable events** - Append-only event log
- **Device tokens** - Long-lived device authentication
- **System configuration** - OIDC, SMTP, webhook configs
- **User groups** - Group definitions and memberships
- **Replication targets** - Target configuration
- **Durable job documents** - Job definitions and final status

#### 2. RustFS Derived Projections (Rebuildable)

These can be rebuilt from canonical state:

- **Folder children indexes** - List of children per folder
- **User roots index** - Root folders per user
- **Shared with me index** - Shares per recipient
- **Notification views** - User notification summaries
- **Job status summaries** - Job queue views
- **Thumbnail metadata** - Thumbnail location info

#### 3. Ephemeral (Memory or Redis)

These are safe to lose and reconstruct:

- **WebSocket presence** - Connection state
- **Runtime hot cache** - Acceleration only
- **Rate limiting counters** - Throttling state
- **Active worker claims** - Current job leases
- **Short-lived locks** - Coordination leases
- **Session tokens (lookup)** - Active session index
- **Session revocation cache** - Logout tracking
- **OIDC login states** - In-progress flows
- **Device pair requests** - Pending pairings
- **Idempotency keys** - Duplicate prevention (bounded TTL)
- **Upload coordination** - Multi-part upload state

#### 4. Removed

- **PostgreSQL connection pool** - Entirely removed
- **SQLx runtime dependency** - Removed from production path
- **Migration runner** - Replaced with schema versioning
- **All SQL queries** - Replaced with object store operations

---

## Runtime Profiles

### Profile A: Standalone

```
Components: rustshare + rustfs
Redis: Disabled
PostgreSQL: None
```

**Use case:** Single-node deployments, development, simple installations

**Characteristics:**
- All ephemeral state in process memory
- In-process coordination (InMemoryCoordination)
- Session revocation cached in memory (cleared on restart)
- No distributed guarantees
- Simple deployment

**Limitations:**
- Session revocation may not be immediate across restarts
- No horizontal scaling
- Rate limits reset on restart
- In-memory cache lost on restart

### Profile B: Distributed

```
Components: rustshare (multiple) + rustfs + redis
Redis: Required for coordination
PostgreSQL: None
```

**Use case:** Production, high availability, horizontal scaling

**Characteristics:**
- Redis-backed coordination (RedisCoordinationStore)
- Distributed session revocation
- Shared rate limiting state
- Worker claim coordination across instances
- WebSocket presence/fanout

**Requirements:**
- Redis available at startup
- All coordination via Redis or RustFS
- Consistent session handling across instances

---

## Configuration Model

### Environment Variables

```bash
# Runtime profile (required)
RUSTSHARE_RUNTIME_PROFILE=standalone|distributed

# Redis configuration (required for distributed)
RUSTSHARE_REDIS_ENABLED=true|false
RUSTSHARE_REDIS_URL=redis://localhost:6379

# Session configuration
RUSTSHARE_SESSION_MODE=stateless|durable
RUSTSHARE_SESSION_TTL_SECONDS=86400

# Job coordination backend
RUSTSHARE_JOB_COORDINATION=memory|redis

# Metadata backend (always rustfs in zero-postgres)
RUSTSHARE_METADATA_BACKEND=rustfs
```

### Profile Validation Rules

| Profile | Redis Required | Redis Optional | Invalid Config |
|---------|----------------|----------------|----------------|
| standalone | - | Allowed | `redis_required=true` without Redis |
| distributed | Yes | - | Missing Redis URL, `redis_enabled=false` |

---

## Module Structure

### New/Modified Modules

```
backend/crates/storage/src/
├── coordination/
│   ├── mod.rs              # CoordinationStore trait
│   ├── memory.rs           # InMemoryCoordinationStore
│   └── redis.rs            # RedisCoordinationStore
├── session/
│   ├── mod.rs              # SessionManager trait
│   ├── stateless.rs        # JWT-based sessions
│   └── redis.rs            # Redis-backed revocation
├── notification/
│   ├── mod.rs              # NotificationStore trait
│   └── projection.rs       # RustFS-backed projection
├── job/
│   ├── mod.rs              # JobCoordinator trait
│   ├── queue.rs            # Job queue management
│   └── coordination.rs     # Worker claim logic
├── user/
│   ├── mod.rs              # UserRepository trait
│   └── rustfs.rs           # RustFS implementation
└── config/
    ├── mod.rs              # ConfigStore trait
    └── rustfs.rs           # System config storage

backend/server/src/
├── runtime_profile.rs      # Profile detection and validation
├── state.rs                # AppState without PgPool
└── main.rs                 # Updated initialization
```

---

## Data Flow Diagram

### Write Path (Canonical)

```
Handler → Service → Repository → MetadataDocumentStore (RustFS)
                      ↓
                EventLogStore (RustFS - append-only)
                      ↓
                Projection Update (RustFS - derived)
                      ↓
                Cache Invalidation (Redis or Memory)
```

### Read Path (Accelerated)

```
Handler → Service → RuntimeCache (check)
                      ↓ (miss)
                IndexStore (RustFS projection)
                      ↓ (miss or stale)
                MetadataDocumentStore (RustFS canonical)
                      ↓
                Cache Update (Redis or Memory)
```

### Coordination Path (Distributed Mode)

```
Operation → CoordinationStore.acquire_lease()
                      ↓
                Redis SET resource:lease (NX, EX)
                      ↓
                Execute operation
                      ↓
                CoordinationStore.release_lease()
                      ↓
                Redis DEL resource:lease
```

---

## Session Model

### Stateless Sessions (Default for Standalone)

**Mechanism:** Signed/encrypted JWT in HTTP-only cookie

**Login flow:**
1. Authenticate user against RustFS user store
2. Generate signed JWT (user_id, email, exp)
3. Set HTTP-only cookie
4. Optional: Store session hash in Redis/memory for revocation

**Logout flow:**
1. Clear cookie
2. Add JWT signature to revocation cache (Redis or memory)
3. Revocation cache has TTL matching JWT expiry

**Forced revocation:**
- Standalone: Memory cache (cleared on restart, acceptable limitation)
- Distributed: Redis cache (shared across instances)

### Guarantees

| Mode | Revocation Latency | Survives Restart | Cross-Instance |
|------|-------------------|------------------|----------------|
| standalone | Immediate | No | N/A |
| distributed | Immediate | Yes | Yes |

---

## Notification Model

### Architecture

**Canonical source:** Event log in RustFS

**Derived view:** Per-user notification index in RustFS

**Runtime fanout:** Optional Redis pub/sub for real-time delivery

### Data Flow

1. Action occurs → Event appended to EventLogStore
2. NotificationProjector updates user notification index
3. Real-time notification via WebSocket (if connected)
4. User fetches notifications from derived index

### Schema

```rust
// Notification document (in RustFS)
struct NotificationDocument {
    id: Uuid,
    user_id: Uuid,           // Recipient
    event_id: Uuid,          // Reference to event
    resource_type: String,   // "file", "folder", "share"
    resource_id: Uuid,
    notification_type: String, // "shared", "modified", etc.
    title: String,
    message: String,
    read: bool,
    created_at: DateTime<Utc>,
}

// User notification index (in RustFS)
struct UserNotificationIndex {
    user_id: Uuid,
    notifications: Vec<NotificationRef>,
    unread_count: u32,
    version: u64,
}
```

---

## Job/Replication Coordination Model

### Architecture

**Durable state (RustFS):**
- Job document with status, retry count, result
- Replication target configuration

**Ephemeral coordination (Redis or Memory):**
- Active job claims (leases)
- Worker heartbeats
- Rate limiting between attempts

### Job Document

```rust
struct JobDocument {
    id: Uuid,
    job_type: String,           // "replication", "thumbnail", etc.
    resource_type: String,      // "file_version", etc.
    resource_id: Uuid,
    status: JobStatus,          // Pending, Running, Completed, Failed
    priority: i32,
    created_at: DateTime<Utc>,
    scheduled_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    result: Option<JobResult>,
    retry_count: u32,
    max_retries: u32,
    error_message: Option<String>,
}
```

### Coordination Flow

1. Job created → Write JobDocument to RustFS (status: Pending)
2. Worker polls → Query for Pending jobs
3. Worker claims → CoordinationStore.acquire_lease(job_id)
4. Update job → Write JobDocument (status: Running)
5. Execute job → Perform work
6. Complete job → Write JobDocument (status: Completed/Failed)
7. Release lease → CoordinationStore.release_lease()

### Safety

- Lease prevents concurrent execution
- Lease TTL prevents orphaned jobs
- Heartbeat extends lease during long operations
- Job document survives worker crashes

---

## Implementation Phases

### Phase 1: Coordination Abstraction
- Create CoordinationStore trait
- Implement InMemoryCoordinationStore
- Implement RedisCoordinationStore
- Add coordination to metadata_v2

### Phase 2: User Storage
- Create UserRepository trait
- Implement RustFS user storage
- Migrate user queries from PostgreSQL
- Add user-related indexes

### Phase 3: Session Management
- Create SessionManager trait
- Implement JWT-based stateless sessions
- Implement Redis revocation cache
- Replace session table queries

### Phase 4: Notification Storage
- Create NotificationStore trait
- Implement RustFS notification projection
- Create NotificationProjector
- Replace notification queries

### Phase 5: Job Coordination
- Create JobCoordinator trait
- Implement RustFS job document storage
- Implement worker claim logic
- Replace replication job queries

### Phase 6: Remove PostgreSQL
- Remove PgPool from AppState
- Remove SQLx from dependencies
- Update main.rs initialization
- Remove migration runner

### Phase 7: Configuration & Profiles
- Implement runtime profile selection
- Add configuration validation
- Update docker-compose files
- Update documentation

### Phase 8: Testing & Validation
- Unit tests for new components
- Standalone integration tests
- Distributed integration tests
- Performance validation

---

## Correctness Invariants

1. **Canonical truth survives restart** - All RustFS data persists
2. **Redis loss is non-critical** - System continues, coordination falls back
3. **No silent double-processing** - Leases prevent concurrent job execution
4. **Session security** - Revocation works within documented bounds
5. **API compatibility** - No breaking changes to external API
6. **Schema versioning** - Forward/backward compatibility for documents

---

## Migration Path

For existing PostgreSQL installations:

1. Deploy new version with dual-write mode (writes to both)
2. Run migration tool to copy historical data to RustFS
3. Verify parity between backends
4. Switch to rustfs-only mode
5. Remove PostgreSQL

Note: This project implements the clean-slate target architecture. Migration tooling exists in `storage/src/admin/` for existing deployments.
