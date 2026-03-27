#!/bin/bash
# Migration script from PostgreSQL to RustFS metadata backend
#
# This script helps migrate from PostgreSQL to RustFS-backed metadata.
# It should be run in stages as described in the migration guide.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    if [ -z "$RUSTFS_ENDPOINT" ]; then
        log_error "RUSTFS_ENDPOINT not set"
        exit 1
    fi
    
    if [ -z "$DATABASE_URL" ]; then
        log_error "DATABASE_URL not set"
        exit 1
    fi
    
    log_info "Prerequisites OK"
}

# Stage 1: Enable dual-write mode
enable_dual_write() {
    log_info "Stage 1: Enabling dual-write mode..."
    
    export RUSTSHARE_METADATA_BACKEND=dual_write
    log_info "Set RUSTSHARE_METADATA_BACKEND=dual_write"
    log_info "Restart the server to apply this change"
    
    log_warn "Monitor logs for dual-write errors"
}

# Stage 2: Verify data parity
verify_parity() {
    log_info "Stage 2: Verifying data parity..."
    
    # Check health
    curl -s http://localhost:8080/api/admin/metadata/health | jq .
    
    # Get stats
    curl -s http://localhost:8080/api/admin/metadata/stats | jq .
    
    log_info "Use the admin API to verify specific entities:"
    log_info "  curl http://localhost:8080/api/admin/metadata/verify/folder/{id}"
    log_info "  curl http://localhost:8080/api/admin/metadata/verify/file/{id}"
}

# Stage 3: Switch to RustFS reads
enable_rustfs_reads() {
    log_info "Stage 3: Switching to RustFS reads..."
    
    export RUSTSHARE_METADATA_BACKEND=rustfs_reads
    log_info "Set RUSTSHARE_METADATA_BACKEND=rustfs_reads"
    log_info "Restart the server to apply this change"
    
    log_warn "PostgreSQL is still the write source"
    log_warn "Monitor for read errors"
}

# Stage 4: Full RustFS migration
enable_rustfs_full() {
    log_info "Stage 4: Enabling full RustFS backend..."
    
    read -p "Are you sure you want to switch to full RustFS? This cannot be undone. (yes/no): " confirm
    
    if [ "$confirm" != "yes" ]; then
        log_info "Migration cancelled"
        exit 0
    fi
    
    export RUSTSHARE_METADATA_BACKEND=rustfs
    log_info "Set RUSTSHARE_METADATA_BACKEND=rustfs"
    log_info "Restart the server to apply this change"
    
    log_warn "PostgreSQL is no longer used for metadata!"
    log_warn "Ensure you have backups before proceeding"
}

# Rebuild indexes
rebuild_indexes() {
    log_info "Rebuilding indexes..."
    
    # This would call the admin API to rebuild all indexes
    log_info "Use the admin API to rebuild indexes:"
    log_info "  curl -X POST http://localhost:8080/api/admin/metadata/rebuild/folder/{id}/children"
}

# Repair inconsistencies
repair_inconsistencies() {
    log_info "Repairing inconsistencies..."
    
    log_info "Use the admin API to repair:"
    log_info "  curl -X POST http://localhost:8080/api/admin/metadata/repair/folder/{id}/parent"
}

# Show usage
usage() {
    cat <<EOF
Usage: $0 [command]

Commands:
  check           Check prerequisites
  dual-write      Enable dual-write mode (Stage 1)
  verify          Verify data parity (Stage 2)
  rustfs-reads    Switch to RustFS reads (Stage 3)
  rustfs-full     Full RustFS migration (Stage 4)
  rebuild-indexes Rebuild all indexes
  repair          Repair inconsistencies
  help            Show this help

Environment Variables:
  RUSTFS_ENDPOINT       RustFS/S3 endpoint URL
  DATABASE_URL          PostgreSQL connection string
  RUSTSHARE_METADATA_BACKEND  Current backend setting

Examples:
  # Stage 1: Enable dual-write
  $0 dual-write

  # Stage 2: Verify parity
  $0 verify

  # Stage 3: Switch reads to RustFS
  $0 rustfs-reads

  # Stage 4: Full migration
  $0 rustfs-full

EOF
}

# Main
case "${1:-help}" in
    check)
        check_prerequisites
        ;;
    dual-write)
        check_prerequisites
        enable_dual_write
        ;;
    verify)
        verify_parity
        ;;
    rustfs-reads)
        check_prerequisites
        enable_rustfs_reads
        ;;
    rustfs-full)
        check_prerequisites
        enable_rustfs_full
        ;;
    rebuild-indexes)
        rebuild_indexes
        ;;
    repair)
        repair_inconsistencies
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        log_error "Unknown command: $1"
        usage
        exit 1
        ;;
esac
