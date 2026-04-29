# Upgrading RustShare

This guide helps operators upgrade a running RustShare deployment.

> **Project status:** Pre-1.0. Breaking changes may occur on MINOR version bumps. Always review [`CHANGELOG.md`](../CHANGELOG.md) and the [Breaking Changes](#breaking-changes) section before upgrading.

---

## General Upgrade Steps

Follow this sequence for every upgrade:

1. **Create a backup**
   ```bash
   ./scripts/backup-stack.sh
   ```
   Verify the backup bundle contains `postgres.sql.gz`, `rustfs-data.tar.gz`, `config.tar.gz`, and `manifest.env`.

2. **Review the changelog**
   - Read [`CHANGELOG.md`](../CHANGELOG.md) for the target version.
   - Check [Breaking Changes](#breaking-changes) and [Environment Variable Changes](#environment-variable-changes) below.
   - If upgrading across a MINOR version while `< 1.0.0`, expect potential breaking changes.

3. **Pull the new image**
   ```bash
   docker compose pull
   ```
   Or, if using the pilot profile:
   ```bash
   export RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend:X.Y.Z
   docker compose -f docker-compose.yml -f docker-compose.pilot.yml pull
   ```

4. **Run database migrations**
   - Migrations are applied automatically when the backend container starts, or
   - Run manually:
     ```bash
     docker compose exec backend sqlx migrate run
     ```
   - Check backend logs for migration success:
     ```bash
     docker compose logs backend --tail=50
     ```

5. **Restart services**
   ```bash
   docker compose up -d
   ```
   Or with the pilot profile:
   ```bash
   docker compose -f docker-compose.yml -f docker-compose.pilot.yml up -d
   ```

6. **Verify health**
   ```bash
   ./scripts/final-launch-smoke.sh
   ```
   Confirm all containers are running and the health endpoint responds.

---

## Pre-1.0 Warning

RustShare is currently pre-1.0. While we adhere to SemVer:

- **MINOR bumps (e.g., `0.2.x` → `0.3.0`) may include breaking changes.**
- **PATCH bumps (e.g., `0.2.1` → `0.2.2`) should be safe.**

**Recommendations:**

- Pin to exact versions in production (`X.Y.Z`, not `X.Y` or `X`).
- Read `CHANGELOG.md` and this upgrading guide before any MINOR upgrade.
- Test MINOR upgrades in a staging environment first.

---

## Database Migrations

RustShare uses `sqlx` migrations stored in `backend/migrations/`.

### Automatic migrations

The Docker entrypoint runs `sqlx migrate run` on startup by default. No operator action is required unless the migration fails.

### Manual migrations

If you prefer to run migrations explicitly (e.g., during a maintenance window):

```bash
docker compose exec backend sqlx migrate run
```

### Troubleshooting

If a migration fails with a checksum mismatch:

```
error: migration 20260404000002 was previously applied but has been modified
```

See [`docs/DEPLOYMENT.md`](DEPLOYMENT.md#migration-checksum-fix) for the resolution procedure.

---

## Docker Compose Upgrade

### Standard stack

```bash
# 1. Backup
./scripts/backup-stack.sh

# 2. Pull new images
docker compose pull

# 3. Recreate containers with new images
docker compose up -d

# 4. Verify
./scripts/final-launch-smoke.sh
```

### Pilot stack (pre-built image)

```bash
# 1. Backup
./scripts/backup-stack.sh

# 2. Set the target image
export RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend:X.Y.Z

# 3. Pull and recreate
docker compose -f docker-compose.yml -f docker-compose.pilot.yml pull
docker compose -f docker-compose.yml -f docker-compose.pilot.yml up -d

# 4. Verify
./scripts/final-launch-smoke.sh
```

---

## Rollback Procedure

If an upgrade fails or causes unexpected behavior:

1. **Stop the current stack**
   ```bash
   docker compose down
   ```

2. **Restore the previous image**
   - For standard stack: rebuild from the previous Git commit.
   - For pilot stack: revert the `RUSTSHARE_BACKEND_IMAGE` to the previous version tag.

3. **Restart with the previous image**
   ```bash
   docker compose up -d
   ```
   Or for pilot:
   ```bash
   export RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend:PREVIOUS_VERSION
   docker compose -f docker-compose.yml -f docker-compose.pilot.yml up -d
   ```

4. **If data corruption occurred, restore from backup**
   ```bash
   ./scripts/restore-stack.sh ./backups/YYYYMMDDTHHMMSS
   ```
   Then run the post-restore smoke test:
   ```bash
   ./scripts/post-restore-smoke.sh
   ```

5. **Verify rollback**
   ```bash
   ./scripts/final-launch-smoke.sh
   ```

---

## Breaking Changes

> This section documents known breaking changes by version. Because RustShare is pre-1.0, MINOR version bumps may require operator action.

*No breaking changes have been documented yet. This section will be populated as releases are cut.*

| Version | Breaking Change | Migration Path |
|---------|-----------------|----------------|
| — | — | — |

---

## Environment Variable Changes

> This section tracks new, removed, or renamed environment variables by version.

*No environment variable changes have been documented yet.*

| Version | Change | Details |
|---------|--------|---------|
| — | — | — |
