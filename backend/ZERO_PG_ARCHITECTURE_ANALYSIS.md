# Zero-PostgreSQL Architecture Analysis

## Current State: Stub and Placeholder Map

### Handlers with Stubbed/Placeholder Implementations

| Handler File | Status | Missing Implementation |
|--------------|--------|----------------------|
| `extractors.rs` | PARTIAL | Admin verification uses placeholder, device token lookup not implemented |
| `device_auth.rs` | STUB | Device pairing flow uses CoordinationStore only partially |
| `devices.rs` | STUB | Returns empty list, no real device storage |
| `admin/groups.rs` | STUB | All CRUD operations return empty/not_found |
| `admin/audit.rs` | STUB | Returns empty audit log |
| `admin/config.rs` | STUB | Returns placeholder config responses |
| `admin/users.rs` | STUB | Returns empty lists, no user CRUD |
| `admin/webhooks.rs` | STUB | Returns empty webhook list |
| `sync.rs` | PARTIAL | Uses EventStore but get_events_since needs verification |
| `replication_handlers.rs` | STUB | Returns placeholder replication stats |
| `notifications.rs` | STUB | Returns empty notifications |
| `user_shares.rs` | STUB | Returns NOT_IMPLEMENTED for user sharing |

### Storage Layer Status

| Component | Status | Notes |
|-----------|--------|-------|
| `MetadataDocumentStore` | IMPLEMENTED | Core trait for object-store operations |
| `EventLogStore` | IMPLEMENTED | Append-only event storage |
| `BlobStore` | IMPLEMENTED | Via ObjectStore |
| `FolderRepository` | TRAITS | Needs RustFS implementation |
| `FileRepository` | TRAITS | Needs RustFS implementation |
| `ShareRepository` | TRAITS | Needs RustFS implementation |
| `UserRepository` | MISSING | No trait or implementation |
| `DeviceRepository` | MISSING | No trait or implementation |
| `GroupRepository` | MISSING | No trait or implementation |
| `NotificationRepository` | PARTIAL | Has trait but no implementation |
| `AuditStore` | MISSING | No trait or implementation |
| `ConfigStore` | MISSING | No trait or implementation |
| `JobRepository` | TRAITS | Has traits but needs implementation |
| `LookupStore` | MISSING | For token/email lookups |
| `CoordinationStore` | IMPLEMENTED | Memory and Redis implementations |

### Required JSON Document Schemas (Not Yet Defined)

| Schema | Purpose | Required Fields |
|--------|---------|-----------------|
| `DeviceDocument` | Device tokens | id, user_id, name, token_hash, created_at, last_used_at |
| `PairingRequestDocument` | Device pairing | id, user_code, device_code, token_hash, status, expires_at |
| `GroupDocument` | User groups | id, name, description, created_by, created_at |
| `GroupMembershipDocument` | Group membership | group_id, user_id, added_at, added_by |
| `AuditLogEntryDocument` | Audit trail | id, actor_id, action, target_type, target_id, detail, occurred_at |
| `ConfigDocument` | App/OIDC/SMTP config | id, config_type, data, updated_by, updated_at |
| `WebhookDocument` | Webhook configs | id, name, url, secret_hash, events, enabled |
| `JobDocument` | Background jobs | id, job_type, status, payload, attempts, scheduled_at |
| `UserLookupDocument` | Email -> User ID | email_hash, user_id |
| `TokenLookupDocument` | Token -> Share/Device | token_hash, resource_type, resource_id, expires_at |

## Bucket/Prefix Structure (Canonical)

```
rustshare-data/ (bucket)
├── shared/
│   └── blobs/
│       └── sha256/
│           └── ab/
│               └── cd/
│                   └── <hash>              # Content-addressed blobs
│
└── apps/
    └── rustshare/
        └── meta/
            ├── users/
            │   └── <user-id>.json          # UserDocument
            ├── groups/
            │   └── <group-id>.json         # GroupDocument
            ├── group_memberships/
            │   └── <group-id>/
            │       └── <user-id>.json      # GroupMembershipDocument
            ├── devices/
            │   └── <device-id>.json        # DeviceDocument
            ├── pairings/
            │   └── <pairing-id>.json       # PairingRequestDocument
            ├── folders/
            │   └── <folder-id>.json        # FolderDocument (exists)
            ├── files/
            │   └── <file-id>.json          # FileDocument (exists)
            ├── file_versions/
            │   └── <file-id>/
            │       └── <version-id>.json   # FileVersionDocument (exists)
            ├── shares/
            │   └── <share-id>.json         # ShareDocument (exists)
            ├── webhooks/
            │   └── <webhook-id>.json       # WebhookDocument
            ├── jobs/
            │   └── <job-id>.json           # JobDocument
            ├── tombstones/
            │   ├── files/
            │   │   └── <file-id>.json      # TombstoneDocument (exists)
            │   └── folders/
            │       └── <folder-id>.json    # TombstoneDocument (exists)
            └── config/
                ├── app.json                # ConfigDocument (app settings)
                ├── oidc.json               # ConfigDocument (OIDC)
                └── smtp.json               # ConfigDocument (SMTP)
        
        ├── indexes/
        │   ├── folders/
        │   │   └── <folder-id>/
        │   │       └── children.json       # FolderChildrenIndex (exists)
        │   ├── users/
        │   │   └── <user-id>/
        │   │       ├── roots.json          # UserRootsIndex
        │   │       ├── recent.json         # UserRecentIndex
        │   │       ├── shared_with_me.json # SharedWithMeIndex (exists)
        │   │       ├── devices.json        # UserDevicesIndex
        │   │       ├── notifications.json  # UserNotificationIndex (exists)
        │   │       └── groups.json         # UserGroupsIndex
        │   ├── groups/
        │   │   └── <group-id>/
        │   │       └── members.json        # GroupMembersIndex
        │   ├── jobs/
        │   │   ├── queue.json              # JobQueueIndex
        │   │   └── by_status/
        │   │       ├── pending.json        # JobsByStatusIndex
        │   │       ├── running.json        # JobsByStatusIndex
        │   │       └── failed.json         # JobsByStatusIndex
        │   └── shares/
        │       └── by_resource/
        │           └── <resource-id>.json  # ResourceSharesIndex
        
        ├── lookups/
        │   ├── public_share_tokens/
        │   │   └── <token-hash>.json       # TokenLookupDocument
        │   ├── pairing_codes/
        │   │   └── <code>.json             # PairingCodeLookupDocument
        │   ├── pairing_tokens/
        │   │   └── <token-hash>.json       # TokenLookupDocument
        │   ├── user_by_email/
        │   │   └── <email-hash>.json       # EmailLookupDocument
        │   └── device_by_session/
        │       └── <session-id>.json       # SessionDeviceLookup
        
        ├── events/
        │   └── 2026/
        │       └── 03/
        │           └── 27/
        │               └── <event-id>.json # EventDocument (exists)
        
        └── audit/
            └── 2026/
                └── 03/
                    └── 27/
                        └── <audit-id>.json # AuditLogEntryDocument

```

## Implementation Priority

### Phase 1: Core Identity & Auth (Foundation)
1. UserRepository trait and RustFS implementation
2. DeviceRepository trait and RustFS implementation  
3. LookupStore for email/token resolution
4. Update extractors.rs to use real repositories

### Phase 2: Admin Foundation
1. GroupRepository trait and RustFS implementation
2. AuditStore trait and RustFS implementation
3. ConfigStore trait and RustFS implementation
4. Update all admin handlers

### Phase 3: User Features
1. NotificationRepository RustFS implementation
2. Update notification handlers
3. User share implementation (requires UserRepository)

### Phase 4: Jobs & Replication
1. JobRepository RustFS implementation
2. JobCoordinator with Redis support
3. Update replication handlers

### Phase 5: Cleanup
1. Remove StubMetadataRepository
2. Verify no PostgreSQL/SQLx dependencies
3. Add comprehensive tests

## Key Design Decisions

### 1. Lookup Documents Pattern
For O(1) lookups by secondary keys (email, token), we store separate lookup documents:
- Key: `lookups/user_by_email/<email-hash>.json`
- Value: `{ "user_id": "...", "email": "..." }`
- Updates must maintain consistency with canonical documents

### 2. Index Maintenance Pattern
Indexes are rebuildable projections:
- Written asynchronously after canonical document updates
- Can be regenerated from canonical data if lost
- Optimistic concurrency for updates

### 3. Audit Log Pattern
Audit entries are append-only:
- Stored in date-based prefixes for time-range queries
- Also written to per-user index for user audit trails
- Immutable once written

### 4. Job Queue Pattern
Jobs use a hybrid approach:
- Canonical job documents stored in object store
- CoordinationStore (Redis) used for worker leases
- Index documents for queue state

## Testing Strategy

1. **Unit Tests**: Each repository implementation
2. **Integration Tests**: Handler-level with in-memory stores
3. **Schema Tests**: Round-trip serialization for all documents
4. **Concurrency Tests**: Optimistic locking behavior
5. **Standalone Mode Tests**: Without Redis
6. **Distributed Mode Tests**: With Redis coordination
