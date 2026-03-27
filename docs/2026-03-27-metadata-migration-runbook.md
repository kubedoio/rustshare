# Metadata Migration Runbook

**Date:** 2026-03-27  
**Applies to:** RustShare deployments using PostgreSQL metadata (legacy)  
**Goal:** Migrate to RustFS/S3-based metadata storage

## Overview

This runbook guides you through migrating your RustShare deployment from PostgreSQL-only metadata storage to the new RustFS/S3-based metadata system. The migration uses a **phased approach** with verification at each stage and safe rollback options.

## Migration Stages

```
┌──────────┐      ┌────────────┐      ┌───────────────┐      ┌─────────┐
│ postgres │ ───> │ dual_write │ ───> │ rustfs_reads │ ───> │ rustfs  │
│  (start) │      │  (Stage 1) │      │   (Stage 2)   │      │  (end)  │
└──────────┘      └────────────┘      └───────────────┘      └─────────┘
      │                  │                    │                   │
   stable         [verify parity]      [validate reads]      [monitor]
                       │                    │                   │
                  rollback OK           rollback OK         committed
```

**Time Estimate:**
- Stage 1 (dual_write): 5-10 minutes
- Verification: 10-30 minutes (depending on data size)
- Stage 2 (rustfs_reads): 5 minutes
- Validation: 1-24 hours (recommended monitoring period)
- Stage 3 (rustfs): 5 minutes

## Prerequisites

### 1. System Requirements

- RustShare version with metadata_v2 support (2026-03-27 or later)
- Access to RustFS/S3 storage (should already be configured for file storage)
- Database backup (strongly recommended)

### 2. Verify Current State

```bash
# Check current metadata backend
echo $RUSTSHARE_METADATA_BACKEND
# Should be: postgres (or empty/default)

# Verify RustFS connectivity
curl -s http://localhost:9000/minio/health/live
# Should return: 200 OK
```

### 3. Backup (CRITICAL)

```bash
# Create database backup before migration
pg_dump $DATABASE_URL > rustshare-metadata-backup-$(date +%Y%m%d).sql

# Or use the built-in backup script
./scripts/backup-stack.sh
```

## Stage 1: Enable Dual-Write Mode

In this stage, all metadata writes go to **both** PostgreSQL and RustFS, but reads still come from PostgreSQL. This is the safest starting point.

### Step 1.1: Set Environment Variable

```bash
# For Docker Compose deployments
export RUSTSHARE_METADATA_BACKEND=dual_write

# For systemd services, edit /etc/rustshare/rustshare.env
RUSTSHARE_METADATA_BACKEND=dual_write

# For Kubernetes, update the ConfigMap/Deployment
kubectl set env deployment/rustshare RUSTSHARE_METADATA_BACKEND=dual_write
```

### Step 1.2: Update docker-compose.yml (if using Docker)

```yaml
services:
  backend:
    environment:
      # ... existing env vars ...
      RUSTSHARE_METADATA_BACKEND: dual_write
      RUSTSHARE_METADATA_CACHE: "true"
      RUSTSHARE_METADATA_PREFIX: apps/rustshare
```

### Step 1.3: Deploy the Change

```bash
# Docker Compose
docker compose up -d backend

# Watch logs for any errors
docker compose logs -f backend | grep -i metadata
```

### Step 1.4: Verify Dual-Write is Active

```bash
# Check health endpoint
curl -s http://localhost:8080/api/admin/metadata/health | jq .

# Expected response:
# {
#   "status": "healthy",
#   "backend": "dual_write",
#   "postgres_connected": true,
#   "rustfs_connected": true
# }
```

### Step 1.5: Generate Some Activity

The dual-write mode only writes new/updated metadata to RustFS. To migrate existing data, you need activity:

```bash
# Option A: Normal usage - use the app normally for a few hours

# Option B: Backfill existing data (if you have the backfill tool)
# curl -X POST http://localhost:8080/api/admin/metadata/backfill
```

**Note:** Without backfill, older metadata won't exist in RustFS until it's accessed/modified. This is fine for gradual migration.

## Stage 2: Verify Data Parity

Before proceeding, verify that PostgreSQL and RustFS have matching data.

### Step 2.1: Check Statistics

```bash
./scripts/verify-metadata.sh stats

# Or via curl
curl -s http://localhost:8080/api/admin/metadata/stats | jq .
```

### Step 2.2: Run Parity Verification

```bash
# Run parity check (compares PostgreSQL vs RustFS)
./scripts/verify-metadata.sh parity

# Expected output:
# [==== Parity Verification ====]
# [INFO] Checking parity between PostgreSQL and RustFS...
# {
#   "passed": 150,
#   "failed": 0,
#   "mismatches": []
# }
# [INFO] Parity check passed: 150 entities verified
```

### Step 2.3: Check Consistency

```bash
./scripts/verify-metadata.sh consistency
```

### Step 2.4: Monitor for Errors

```bash
# Watch for dual-write errors
docker compose logs -f backend | grep -E "(metadata|dual_write|error)"
```

### Stage 2 Decision Point

| Condition | Action |
|-----------|--------|
| All checks pass | Proceed to Stage 3 |
| Minor mismatches | Run repair: `./scripts/repair-metadata.sh --execute all` |
| Major issues | Rollback to `postgres` and investigate |

## Stage 3: Switch Reads to RustFS

In this stage, reads come from RustFS while writes still go to both. This validates the RustFS backend is working correctly.

### Step 3.1: Set Environment Variable

```bash
export RUSTSHARE_METADATA_BACKEND=rustfs_reads
```

### Step 3.2: Deploy the Change

```bash
docker compose up -d backend
```

### Step 3.3: Verify RustFS Reads

```bash
# Check backend status
curl -s http://localhost:8080/api/admin/metadata/health | jq .

# Expected:
# {
#   "status": "healthy",
#   "backend": "rustfs_reads",
#   "postgres_connected": true,
#   "rustfs_connected": true,
#   "reads_from": "rustfs",
#   "writes_to": ["postgres", "rustfs"]
# }
```

### Step 3.4: Test Application Functionality

Perform these tests to verify reads work:

```bash
# Test 1: List folders
curl -s http://localhost:8080/api/v1/folders \
  -H "Cookie: rustshare.sid=YOUR_SESSION"

# Test 2: List folder contents
curl -s http://localhost:8080/api/v1/folders/{folder_id}/contents \
  -H "Cookie: rustshare.sid=YOUR_SESSION"

# Test 3: List files
curl -s http://localhost:8080/api/v1/files \
  -H "Cookie: rustshare.sid=YOUR_SESSION"

# Test 4: Access a share
curl -s http://localhost:8080/api/v1/shares/{share_id}
```

### Step 3.5: Monitor for 24 Hours (Recommended)

Keep the system in `rustfs_reads` mode for at least 24 hours of normal usage:

```bash
# Set up monitoring
curl -s http://localhost:8080/api/admin/metadata/stats | jq .

# Run every hour and log
while true; do
  date >> /var/log/rustshare-metadata-stats.log
  curl -s http://localhost:8080/api/admin/metadata/stats >> /var/log/rustshare-metadata-stats.log
  sleep 3600
done
```

### Stage 3 Decision Point

| Condition | Action |
|-----------|--------|
| No errors for 24h | Proceed to Stage 4 |
| Read errors detected | Rollback to `dual_write` or `postgres` |
| Performance issues | Check RustFS connectivity, consider cache tuning |

## Stage 4: Full RustFS Migration

In this stage, PostgreSQL is no longer used for metadata. This is the **point of no return** for easy rollback.

### Step 4.1: Final Verification

```bash
# Run all verifications one last time
./scripts/verify-metadata.sh all
```

### Step 4.2: Set Environment Variable

```bash
export RUSTSHARE_METADATA_BACKEND=rustfs
```

### Step 4.3: Deploy the Change

```bash
docker compose up -d backend
```

### Step 4.4: Verify Full Migration

```bash
# Check health
curl -s http://localhost:8080/api/admin/metadata/health | jq .

# Expected:
# {
#   "status": "healthy",
#   "backend": "rustfs",
#   "postgres_connected": false,  # or null
#   "rustfs_connected": true
# }
```

### Step 4.5: Monitor Continuously

```bash
# Watch logs closely for first hour
docker compose logs -f backend | grep -i metadata

# Check stats every 15 minutes
for i in {1..4}; do
  sleep 900
  curl -s http://localhost:8080/api/admin/metadata/stats | jq .
done
```

## Post-Migration Tasks

### 1. Update Documentation

Document that your deployment now uses RustFS metadata:

```bash
# Add to your deployment notes
echo "Metadata backend: rustfs (migrated $(date +%Y-%m-%d))" >> DEPLOYMENT_NOTES.md
```

### 2. Adjust PostgreSQL (Optional)

After 30 days of stable operation, you can:

- Reduce PostgreSQL backup frequency for metadata tables
- Consider truncating old metadata tables (KEEP BACKUPS FIRST!)
- Document that PostgreSQL is only used for auth/non-metadata data

### 3. Performance Tuning

Tune the metadata cache based on your workload:

```bash
# Check cache hit rate
curl -s http://localhost:8080/api/admin/metadata/stats | jq '.cache'

# If hit rate < 80%, increase cache size or tune TTL
RUSTSHARE_METADATA_CACHE_SIZE=10000  # entries (default varies)
```

## Rollback Procedures

### Rollback from dual_write to postgres

```bash
# Immediate rollback - no data loss risk
export RUSTSHARE_METADATA_BACKEND=postgres
docker compose up -d backend

# RustFS metadata can be deleted later if desired
```

### Rollback from rustfs_reads to dual_write

```bash
# Safe rollback - both backends still have data
export RUSTSHARE_METADATA_BACKEND=dual_write
docker compose up -d backend

# Or go directly back to postgres
export RUSTSHARE_METADATA_BACKEND=postgres
docker compose up -d backend
```

### Rollback from rustfs (Emergency)

**⚠️ WARNING:** This is more complex as writes only went to RustFS.

```bash
# Option 1: If you have dual_write data from <24h ago
# Restore from PostgreSQL backup and re-migrate

# Option 2: Export from RustFS, import to PostgreSQL
# (Requires custom tooling - contact support)
```

## Troubleshooting

### Issue: High Latency in dual_write Mode

**Cause:** Writing to both backends adds latency.

**Solution:**
- This is expected - it's the trade-off for safety
- Monitor latency separately for reads vs writes
- Consider `rustfs_reads` sooner if latency is critical

### Issue: Cache Misses

**Symptoms:** Slow reads, high RustFS request count.

**Solution:**
```bash
# Verify cache is enabled
echo $RUSTSHARE_METADATA_CACHE  # should be "true"

# Check cache stats
curl -s http://localhost:8080/api/admin/metadata/stats | jq '.cache'

# Clear and rebuild if needed
curl -X POST http://localhost:8080/api/admin/metadata/cache/clear
```

### Issue: Parity Check Failures

**Symptoms:** Mismatches between PostgreSQL and RustFS.

**Solution:**
```bash
# See detailed mismatches
./scripts/verify-metadata.sh parity

# Repair specific entities
./scripts/repair-metadata.sh --execute repair folder {folder_id}
./scripts/repair-metadata.sh --execute repair file {file_id}

# Or repair all
./scripts/repair-metadata.sh --execute all
```

### Issue: RustFS Connection Errors

**Symptoms:** Health check shows `rustfs_connected: false`.

**Solution:**
```bash
# Check RustFS is running
curl -v http://localhost:9000/minio/health/live

# Verify credentials
echo $STORAGE_ACCESS_KEY
echo $STORAGE_SECRET_KEY

# Check bucket exists
aws --endpoint-url=http://localhost:9000 s3 ls s3://$STORAGE_BUCKET/

# Check network connectivity
telnet localhost 9000
```

## Command Reference

### Quick Health Check
```bash
curl -s http://localhost:8080/api/admin/metadata/health | jq .
```

### Statistics
```bash
curl -s http://localhost:8080/api/admin/metadata/stats | jq .
```

### Verification Commands
```bash
# Full verification
./scripts/verify-metadata.sh all

# Specific checks
./scripts/verify-metadata.sh health
./scripts/verify-metadata.sh stats
./scripts/verify-metadata.sh parity
./scripts/verify-metadata.sh consistency
```

### Repair Commands
```bash
# Dry run (default)
./scripts/repair-metadata.sh all

# Execute repairs
./scripts/repair-metadata.sh --execute all

# Repair specific entity
./scripts/repair-metadata.sh --execute repair folder {id}
```

### Migration Script
```bash
# Interactive migration helper
./scripts/migrate-to-rustfs.sh

# Specific stages
./scripts/migrate-to-rustfs.sh dual-write
./scripts/migrate-to-rustfs.sh verify
./scripts/migrate-to-rustfs.sh rustfs-reads
./scripts/migrate-to-rustfs.sh rustfs-full
```

## FAQ

**Q: Do I need to migrate immediately?**  
A: No. PostgreSQL metadata continues to work. Migration is recommended for horizontal scalability but not required.

**Q: Can I migrate partially (some users only)?**  
A: Not currently. Migration is per-deployment. Per-user migration is on the roadmap.

**Q: What happens if RustFS goes down in rustfs_reads mode?**  
A: Reads will fail. The system falls back to error responses. Consider keeping PostgreSQL as backup if uptime is critical.

**Q: How much storage does RustFS metadata use?**  
A: Approximately 2-5KB per file/folder, plus ~1KB per event. For 10,000 files: ~30-50MB.

**Q: Can I go back to PostgreSQL after rustfs mode?**  
A: Not easily. You'd need to export from RustFS and import to PostgreSQL. Plan your migration carefully.

## Support

If you encounter issues during migration:

1. Check this runbook's troubleshooting section
2. Review logs: `docker compose logs backend | grep -i metadata`
3. Run verification: `./scripts/verify-metadata.sh all`
4. Contact support with: health check output, stats output, and relevant logs

## References

- [Metadata Refactor Design](2026-03-27-metadata-refactor-design.md)
- [Metadata Refactor ADR](2026-03-27-metadata-refactor-adr.md)
- [Backup/Restore Runbook](2026-03-20-backup-restore-runbook.md)
