# PostgreSQL Leftovers Checklist

This document tracks remaining PostgreSQL dependencies that need to be addressed for a complete zero-PostgreSQL implementation.

## Critical Leftovers (Block Production Deployment)

### 1. Server Main (backend/server/src/main.rs)
- [ ] Line 76: `use sqlx::PgPool;` - Import still present
- [ ] Line 98: `pub db_pool: PgPool` - AppState still contains PgPool
- [ ] Lines 132-133: Database connection code
- [ ] Line 138: `sqlx::migrate!()` - Migration runner
- [ ] Lines 143-144: MetadataStore and EventStore initialized with db_pool
- [ ] Lines 192-197: Repositories initialized with db_pool

**Action**: Replace with new AppState from `server/src/state/mod.rs`

### 2. Docker Compose (docker-compose.yml)
- [ ] Lines 2-16: PostgreSQL service definition
- [ ] Line 52: `DATABASE_URL` environment variable
- [ ] Line 88: `RUSTSHARE_METADATA_BACKEND=postgres` (default)
- [ ] Line 94-95: Depends on PostgreSQL
- [ ] Line 114: `postgres_data` volume

**Action**: Create docker-compose.standalone.yml and docker-compose.distributed.yml

### 3. Infrastructure Crate (backend/crates/infrastructure/)
- [ ] Cargo.toml: SQLx dependency
- [ ] src/repositories/user_repository.rs - Full SQLx implementation
- [ ] src/repositories/file_repository.rs - Full SQLx implementation
- [ ] src/repositories/folder_repository.rs - Full SQLx implementation
- [ ] src/repositories/share_repository.rs - Full SQLx implementation
- [ ] src/repositories/notification_repository.rs - Full SQLx implementation

**Action**: Either:
- Option A: Replace implementations with RustFS versions
- Option B: Mark crate as deprecated/legacy
- Option C: Add feature flag to exclude SQLx code

### 4. Storage Crate Legacy Code (backend/crates/storage/src/)
- [ ] metadata.rs - Old PostgreSQL-based MetadataStore
- [ ] event_store.rs - Old PostgreSQL-based EventStore
- [ ] service_integration.rs - Integration with SQLx types
- [ ] Cargo.toml - SQLx dependency still present

**Action**: 
- Remove or feature-flag legacy implementations
- Ensure only metadata_v2 code is used

### 5. Core Domain Types (backend/crates/core/src/domain/)
- [ ] notification.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] file.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] folder.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] share.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] user.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] device_token.rs - Has `#[derive(sqlx::FromRow)]`
- [ ] thumbnail.rs - Has `#[derive(sqlx::FromRow)]`

**Action**: Remove SQLx derives or make them conditional on a feature flag

### 6. Environment Configuration (.env.example)
- [ ] Line 12: `DATABASE_URL` documented
- [ ] Line 106-111: Backend selection with postgres as default
- [ ] Line 132: "Update database password" in checklist
- [ ] Line 140: "Set up database backups" in checklist

**Action**: Update to reflect zero-PostgreSQL as default

## Medium Priority (Cleanup)

### 7. Migration Files (backend/migrations/)
- [ ] 32 SQL migration files for PostgreSQL schema

**Action**: 
- Keep for migration tooling
- Document as "legacy migration files - not needed for new deployments"

### 8. Test Files (backend/tests/)
- [ ] Tests use PostgreSQL test database
- [ ] admin_require_admin_test.rs
- [ ] admin_users_test.rs
- [ ] admin_groups_test.rs
- [ ] And 10+ more test files

**Action**: Update tests to use RustFS backends or in-memory stores

### 9. Documentation References
- [ ] backend/README.md - References PostgreSQL
- [ ] TESTING.md - References PostgreSQL test setup
- [ ] PRODUCTION_DEPLOYMENT.md - References PostgreSQL
- [ ] PRODUCTION_READINESS.md - References PostgreSQL

**Action**: Update all documentation to reflect zero-PostgreSQL architecture

## Low Priority (Optional Cleanup)

### 10. Handler Files (backend/server/src/handlers/)
- [ ] Many handlers still use old repository types
- [ ] Need gradual migration to new RustFS repositories

**Action**: Migrate incrementally as handlers are updated

### 11. Metadata Integration (backend/server/src/metadata_integration.rs)
- [ ] Contains dual-write logic
- [ ] References PostgreSQL-backed services

**Action**: Simplify to use only RustFS once migration is complete

## Migration Strategy

### Phase 1: New Components (DONE ✅)
- CoordinationStore abstraction
- SessionManager
- RustFS repositories
- New schemas

### Phase 2: Server Integration (IN PROGRESS)
- [ ] Create new main.rs without PostgreSQL
- [ ] Update AppState
- [ ] Add profile-based initialization

### Phase 3: Legacy Cleanup (PENDING)
- [ ] Feature-flag or remove SQLx dependencies
- [ ] Update docker-compose files
- [ ] Migrate tests

### Phase 4: Documentation (PENDING)
- [ ] Update all docs to reflect new architecture
- [ ] Migration guide for existing users

## Quick Wins

These can be done quickly:

1. **Update .env.example** - Remove DATABASE_URL as required
2. **Create docker-compose files** - Standalone and distributed versions
3. **Feature-flag SQLx** - Add `postgres-legacy` feature flag
4. **Update STATUS.md** - Mark components as migrated

## Verification Command

Check for remaining PostgreSQL references:

```bash
# Find SQLx imports
grep -r "use sqlx" backend/ --include="*.rs"

# Find PgPool usage
grep -r "PgPool" backend/ --include="*.rs"

# Find postgres references
grep -r "postgres" backend/ --include="*.rs" -i

# Find SQL migrations in code
grep -r "sqlx::migrate" backend/ --include="*.rs"
```

## Decision Points

1. **Keep or Remove Infrastructure Crate?**
   - Option A: Keep for backwards compatibility (feature-flagged)
   - Option B: Remove entirely (clean break)
   - **Recommendation**: Option A with deprecation warnings

2. **Test Strategy?**
   - Option A: Migrate all tests to RustFS
   - Option B: Support both backends in tests
   - **Recommendation**: Option A with in-memory stores for unit tests

3. **Migration Tooling?**
   - Keep migration files for existing users
   - Provide standalone migration binary
   - Document migration path clearly
