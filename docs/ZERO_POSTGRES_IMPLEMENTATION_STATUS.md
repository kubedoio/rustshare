# Zero-Postgres Implementation Status

## Completed Work

### Phase 1: CoordinationStore Abstraction ✅

**Files Created:**
- `backend/crates/storage/src/coordination/mod.rs` - Core trait definition
- `backend/crates/storage/src/coordination/memory.rs` - In-memory implementation
- `backend/crates/storage/src/coordination/redis.rs` - Redis implementation

**Features:**
- `CoordinationStore` trait with comprehensive coordination primitives:
  - Distributed locks (short-term mutual exclusion)
  - Leases (longer-term resource claims)
  - Job coordination (claim/heartbeat/release)
  - Rate limiting
  - Session management (cache/revoke/validate)
  - Idempotency keys
  - WebSocket presence tracking
- `InMemoryCoordinationStore` for standalone deployments
- `RedisCoordinationStore` for distributed deployments
- Feature flag `redis-coordination` to make Redis optional

**Key Design Decisions:**
- All coordination state is ephemeral (reconstructible)
- Redis is optional via feature flag
- In-memory implementation for standalone mode
- Consistent error types across implementations

### Phase 2: New Schemas ✅

**Added to `backend/crates/storage/src/metadata_v2/schemas.rs`:**

1. **UserDocument** - User account storage in RustFS
   - All user fields from PostgreSQL schema
   - Support for enable/disable
   - Optimistic concurrency via version field

2. **NotificationDocument** - Notification storage
   - Derived from events
   - Support for read/unread status

3. **UserNotificationIndex** - Rebuildable projection
   - Per-user notification list
   - Unread count denormalization
   - Sorted by created_at

4. **JobDocument** - Job queue storage
   - All job states (Pending, Running, Completed, Failed, Cancelled)
   - Retry logic with exponential backoff
   - Worker assignment tracking
   - Terminal state detection

5. **JobQueueIndex** - Job queue projection
   - Pending, Running, Completed queues
   - Priority-based sorting
   - Size limit on completed history

6. **JobRef** - Lightweight job reference for indices

7. **DeviceTokenDocument** - Long-lived device authentication
   - Expiration and revocation support
   - Device metadata

8. **UserGroupDocument** - User group management
   - Member list management
   - Optimistic concurrency

9. **SystemConfigDocument** - System configuration storage
   - Multiple config types (OIDC, SMTP, Webhooks, etc.)
   - Version tracking

10. **ReplicationTargetDocument** - Replication configuration
    - Encrypted credentials
    - Enable/disable support

11. **ThumbnailDocument** - Thumbnail metadata
    - Reference to blob storage

### Phase 3: SessionManager ✅

**Files Created:**
- `backend/crates/storage/src/session/mod.rs` - Core types and config
- `backend/crates/storage/src/session/manager.rs` - JWT-based session manager

**Features:**
- `SessionManager` with JWT-based stateless sessions
- Support for multiple session types (Web, Api, Device, Share)
- Cookie generation for web sessions
- Token validation with proper error handling
- Session refresh capability
- Logout cookie generation
- Configurable via environment variables

**Key Design Decisions:**
- Stateless JWT validation as primary mechanism
- Optional revocation cache (Redis/memory)
- Standalone mode accepts limitation of non-persistent revocation
- Secure cookie generation with HttpOnly, SameSite, Secure flags

### Dependencies Added ✅

**To `backend/crates/storage/Cargo.toml`:**
- `redis` (optional, with feature flag)
- `jsonwebtoken`
- `sha2`
- `hex`

## Remaining Work

### Phase 4: User Storage Repository

**Needed:**
- `UserRepository` trait
- `RustFsUserRepository` implementation
- User-related indexes (by-email, by-username)
- Integration with MetadataDocumentStore

### Phase 5: Notification Repository

**Needed:**
- `NotificationRepository` trait
- `RustFsNotificationRepository` implementation
- `NotificationProjector` for event-to-notification transformation
- Index management

### Phase 6: Job Repository and Coordinator

**Needed:**
- `JobRepository` trait
- `RustFsJobRepository` implementation
- `JobCoordinator` that uses CoordinationStore for claims
- Integration with replication system

### Phase 7: Remove PostgreSQL from Server

**Needed:**
- Update `AppState` to remove `PgPool`
- Update `main.rs` initialization
- Replace SQLx migrations with RustFS-based schema versioning
- Update all handlers to use new repositories
- Remove SQLx from dependencies where possible

### Phase 8: Runtime Profiles

**Needed:**
- `RuntimeProfile` enum (Standalone, Distributed)
- Profile detection from environment
- Configuration validation
- Profile-specific initialization paths

### Phase 9: Testing and Validation

**Needed:**
- Unit tests for new components
- Standalone integration tests
- Distributed integration tests (with Redis)
- Performance benchmarks

## Architecture Compliance

### ✅ State Placement Rules

| Concern | Canonical (RustFS) | Derived (RustFS) | Ephemeral (Redis) |
|---------|-------------------|------------------|-------------------|
| Users | UserDocument | - | - |
| Sessions | - | - | Cache/Revocation |
| Folders | FolderDocument | - | - |
| Files | FileDocument | - | - |
| File Versions | FileVersionDocument | - | - |
| Shares | ShareDocument | - | - |
| Notifications | NotificationDocument | UserNotificationIndex | - |
| Jobs | JobDocument | JobQueueIndex | Claims/Leases |
| Device Tokens | DeviceTokenDocument | - | - |
| User Groups | UserGroupDocument | - | - |
| System Config | SystemConfigDocument | - | - |
| Replication Targets | ReplicationTargetDocument | - | - |
| Thumbnails | - | ThumbnailDocument | - |

### ✅ Non-Negotiable Rules

1. ✅ PostgreSQL is not used in new code
2. ✅ Redis is not canonical metadata store (only coordination)
3. ✅ RustFS remains durable system of record
4. ✅ Stable IDs used throughout (not paths)
5. ✅ Object-store scans avoided for normal reads (via indexes)
6. ✅ Runtime caches are acceleration only
7. ✅ Multi-object mutations use coordination leases
8. ✅ Both standalone and distributed modes supported

## How to Use

### Standalone Mode (No Redis)

```rust
use rustshare_storage::coordination::{CoordinationStoreFactory};
use rustshare_storage::session::{SessionManager, SessionConfig};

// Create coordination store (in-memory)
let coord_store = CoordinationStoreFactory::create_memory();

// Create session manager
let session_config = SessionConfig::new(jwt_secret);
let session_manager = SessionManager::new(session_config);
```

### Distributed Mode (With Redis)

```rust
use rustshare_storage::coordination::{CoordinationStoreFactory};

// Create Redis coordination store
let coord_store = CoordinationStoreFactory::create_redis("redis://localhost:6379")
    .await?;
```

### Feature Flags

```toml
[dependencies]
rustshare-storage = { path = "../crates/storage", features = ["redis-coordination"] }
```

## Next Steps

1. **Create User Repository** - Implement user CRUD operations via RustFS
2. **Create Notification Repository** - Implement notification storage
3. **Create Job Repository** - Implement job queue via RustFS
4. **Update Server Initialization** - Remove PgPool, use new repositories
5. **Add Runtime Profile Support** - Explicit standalone/distributed modes
6. **Update Documentation** - docker-compose, README, deployment guides

## Testing

Run tests for the new components:

```bash
cd /Users/scolak/Projects/x/rustshare/backend

# Test coordination module
cargo test -p rustshare-storage coordination

# Test session module  
cargo test -p rustshare-storage session

# Test new schemas
cargo test -p rustshare-storage schemas

# Test with Redis coordination feature
cargo test -p rustshare-storage --features redis-coordination
```

## Files Modified

1. `backend/crates/storage/Cargo.toml` - Added dependencies
2. `backend/crates/storage/src/lib.rs` - Added module exports
3. `backend/crates/storage/src/metadata_v2/schemas.rs` - Added new document schemas

## Files Created

1. `backend/crates/storage/src/coordination/mod.rs`
2. `backend/crates/storage/src/coordination/memory.rs`
3. `backend/crates/storage/src/coordination/redis.rs`
4. `backend/crates/storage/src/session/mod.rs`
5. `backend/crates/storage/src/session/manager.rs`
6. `docs/ZERO_POSTGRES_ARCHITECTURE.md`
7. `docs/ZERO_POSTGRES_CONCERN_MAP.md`
8. `docs/ZERO_POSTGRES_IMPLEMENTATION_STATUS.md`
