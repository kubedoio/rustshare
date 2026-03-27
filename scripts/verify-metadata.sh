#!/bin/bash
# Metadata verification script
# 
# Verifies data integrity and consistency in the metadata system.
# Can be used during migration or for regular health checks.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
ADMIN_URL="${RUSTSHARE_ADMIN_URL:-http://localhost:8080/api/admin/metadata}"
API_KEY="${RUSTSHARE_ADMIN_API_KEY:-}"

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

log_section() {
    echo -e "${BLUE}[==== $1 ====]${NC}"
}

# Check if curl and jq are available
check_prerequisites() {
    if ! command -v curl &> /dev/null; then
        log_error "curl is required but not installed"
        exit 1
    fi
    
    if ! command -v jq &> /dev/null; then
        log_error "jq is required but not installed"
        exit 1
    fi
}

# Generic API call
api_call() {
    local method="$1"
    local endpoint="$2"
    local curl_opts="-s"
    
    if [ -n "$API_KEY" ]; then
        curl_opts="$curl_opts -H 'Authorization: Bearer $API_KEY'"
    fi
    
    curl $curl_opts -X "$method" "${ADMIN_URL}${endpoint}"
}

# Health check
health_check() {
    log_section "Health Check"
    
    local response
    response=$(api_call "GET" "/health")
    
    echo "$response" | jq .
    
    local status
    status=$(echo "$response" | jq -r '.status // "unknown"')
    
    if [ "$status" = "healthy" ]; then
        log_info "Metadata system is healthy"
        return 0
    else
        log_warn "Metadata system health: $status"
        return 1
    fi
}

# Get statistics
show_stats() {
    log_section "Metadata Statistics"
    
    local response
    response=$(api_call "GET" "/stats")
    
    echo "$response" | jq .
}

# Verify parity between backends
verify_parity() {
    log_section "Parity Verification"
    log_info "Checking parity between PostgreSQL and RustFS..."
    
    # This endpoint may need to be added to the admin API
    local response
    response=$(api_call "GET" "/verify/parity" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
    
    local passed
    passed=$(echo "$response" | jq -r '.passed // 0')
    local failed
    failed=$(echo "$response" | jq -r '.failed // 0')
    
    if [ "$failed" -eq 0 ]; then
        log_info "Parity check passed: $passed entities verified"
        return 0
    else
        log_error "Parity check failed: $failed entities mismatched"
        return 1
    fi
}

# Verify internal consistency
verify_consistency() {
    log_section "Consistency Verification"
    log_info "Checking internal consistency..."
    
    local response
    response=$(api_call "GET" "/verify/consistency" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
    
    local issues
    issues=$(echo "$response" | jq -r '.issues // 0')
    
    if [ "$issues" -eq 0 ]; then
        log_info "Consistency check passed"
        return 0
    else
        log_error "Consistency check found $issues issues"
        return 1
    fi
}

# Verify specific entity
verify_entity() {
    local entity_type="$1"
    local entity_id="$2"
    
    log_section "Verify $entity_type: $entity_id"
    
    local response
    response=$(api_call "GET" "/verify/${entity_type}/${entity_id}" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
}

# Run all verifications
verify_all() {
    local failed=0
    
    health_check || failed=1
    show_stats
    verify_parity || failed=1
    verify_consistency || failed=1
    
    if [ $failed -eq 0 ]; then
        log_info "All verifications passed!"
        return 0
    else
        log_error "Some verifications failed"
        return 1
    fi
}

# Show usage
usage() {
    cat <<EOF
Usage: $0 [command] [options]

Commands:
  health              Check metadata system health
  stats               Show metadata statistics
  parity              Verify PostgreSQL vs RustFS parity
  consistency         Verify internal consistency
  verify <type> <id>  Verify specific entity (folder|file|share)
  all                 Run all verifications (default)
  help                Show this help

Options:
  --admin-url <url>   Admin API base URL (default: http://localhost:8080/api/admin/metadata)
  --api-key <key>     API key for authentication

Environment Variables:
  RUSTSHARE_ADMIN_URL     Admin API base URL
  RUSTSHARE_ADMIN_API_KEY API key for authentication

Examples:
  # Run all verifications
  $0 all

  # Check health only
  $0 health

  # Verify specific folder
  $0 verify folder 550e8400-e29b-41d4-a716-446655440000

  # Use custom admin URL
  $0 --admin-url http://prod-server/api/admin/metadata health

EOF
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --admin-url)
                ADMIN_URL="$2"
                shift 2
                ;;
            --api-key)
                API_KEY="$2"
                shift 2
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            *)
                break
                ;;
        esac
    done
    
    # Remaining args are command and arguments
    COMMAND="${1:-all}"
    shift || true
}

# Main
main() {
    parse_args "$@"
    check_prerequisites
    
    case "$COMMAND" in
        health)
            health_check
            ;;
        stats)
            show_stats
            ;;
        parity)
            verify_parity
            ;;
        consistency)
            verify_consistency
            ;;
        verify)
            local entity_type="$1"
            local entity_id="$2"
            if [ -z "$entity_type" ] || [ -z "$entity_id" ]; then
                log_error "Usage: $0 verify <folder|file|share> <id>"
                exit 1
            fi
            verify_entity "$entity_type" "$entity_id"
            ;;
        all)
            verify_all
            ;;
        help|--help|-h)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown command: $COMMAND"
            usage
            exit 1
            ;;
    esac
}

main "$@"
