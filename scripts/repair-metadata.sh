#!/bin/bash
# Metadata repair script
#
# Repairs inconsistencies and issues in the metadata system.
# Use with caution - always run verify first!

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
DRY_RUN=true

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

# Repair specific entity
repair_entity() {
    local entity_type="$1"
    local entity_id="$2"
    local repair_type="${3:-auto}"
    
    log_section "Repair $entity_type: $entity_id"
    
    if [ "$DRY_RUN" = true ]; then
        log_warn "DRY RUN - Would repair $entity_type $entity_id"
        return 0
    fi
    
    local response
    response=$(api_call "POST" "/repair/${entity_type}/${entity_id}?type=${repair_type}" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
    
    local success
    success=$(echo "$response" | jq -r '.success // false')
    
    if [ "$success" = "true" ]; then
        log_info "Repair successful"
        return 0
    else
        log_error "Repair failed"
        return 1
    fi
}

# Repair all detected issues
repair_all() {
    log_section "Repair All Issues"
    
    if [ "$DRY_RUN" = true ]; then
        log_warn "DRY RUN mode - no changes will be made"
        log_warn "To actually repair, run with --execute flag"
        
        # Show what would be repaired
        local response
        response=$(api_call "GET" "/verify/consistency" 2>/dev/null || echo '{"issues": []}')
        
        local issues
        issues=$(echo "$response" | jq -r '.issues // 0')
        
        if [ "$issues" -eq 0 ]; then
            log_info "No issues detected that need repair"
        else
            log_warn "Found $issues issues that would be repaired"
            echo "$response" | jq '.details // []'
        fi
        return 0
    fi
    
    log_warn "Executing repairs..."
    
    local response
    response=$(api_call "POST" "/repair/all" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
    
    local repaired
    repaired=$(echo "$response" | jq -r '.repaired // 0')
    local failed
    failed=$(echo "$response" | jq -r '.failed // 0')
    
    log_info "Repaired: $repaired, Failed: $failed"
}

# Fix parent references
fix_parent_refs() {
    local folder_id="$1"
    
    log_section "Fix Parent References"
    
    if [ "$DRY_RUN" = true ]; then
        log_warn "DRY RUN - Would fix parent refs for $folder_id"
        return 0
    fi
    
    repair_entity "folder" "$folder_id" "parent"
}

# Sync from PostgreSQL to RustFS
sync_to_rustfs() {
    local entity_type="$1"
    local entity_id="$2"
    
    log_section "Sync to RustFS: $entity_type $entity_id"
    
    if [ "$DRY_RUN" = true ]; then
        log_warn "DRY RUN - Would sync $entity_type $entity_id to RustFS"
        return 0
    fi
    
    local response
    response=$(api_call "POST" "/sync/${entity_type}/${entity_id}?target=rustfs" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
}

# Show usage
usage() {
    cat <<EOF
Usage: $0 [command] [options]

Commands:
  all                       Repair all detected issues
  repair <type> <id>        Repair specific entity
  fix-parent <folder_id>    Fix parent references for folder
  sync <type> <id>          Sync entity from PostgreSQL to RustFS
  help                      Show this help

Options:
  --admin-url <url>         Admin API base URL (default: http://localhost:8080/api/admin/metadata)
  --api-key <key>           API key for authentication
  --execute                 Actually perform repairs (default: dry run)
  --yes                     Skip confirmation prompts

Environment Variables:
  RUSTSHARE_ADMIN_URL       Admin API base URL
  RUSTSHARE_ADMIN_API_KEY   API key for authentication

Examples:
  # Dry run - see what would be repaired
  $0 all

  # Actually repair all issues
  $0 --execute all

  # Repair specific folder
  $0 --execute repair folder 550e8400-e29b-41d4-a716-446655440000

  # Fix parent references
  $0 --execute fix-parent 550e8400-e29b-41d4-a716-446655440000

WARNING:
  This script modifies data. Always run in dry-run mode first
  and verify the output before using --execute.

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
            --execute)
                DRY_RUN=false
                shift
                ;;
            --yes|-y)
                AUTO_CONFIRM=true
                shift
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
    
    COMMAND="${1:-}"
    shift || true
}

# Confirm execution
confirm() {
    if [ "${AUTO_CONFIRM:-false}" = true ]; then
        return 0
    fi
    
    echo
    log_warn "This will modify metadata. Are you sure?"
    read -p "Type 'yes' to continue: " answer
    
    if [ "$answer" != "yes" ]; then
        log_info "Cancelled"
        exit 0
    fi
}

# Main
main() {
    parse_args "$@"
    
    if [ -z "$COMMAND" ]; then
        usage
        exit 1
    fi
    
    check_prerequisites
    
    if [ "$DRY_RUN" = false ]; then
        confirm
    fi
    
    case "$COMMAND" in
        all)
            repair_all
            ;;
        repair)
            local entity_type="$1"
            local entity_id="$2"
            if [ -z "$entity_type" ] || [ -z "$entity_id" ]; then
                log_error "Usage: $0 repair <folder|file|share> <id>"
                exit 1
            fi
            repair_entity "$entity_type" "$entity_id"
            ;;
        fix-parent)
            local folder_id="$1"
            if [ -z "$folder_id" ]; then
                log_error "Usage: $0 fix-parent <folder_id>"
                exit 1
            fi
            fix_parent_refs "$folder_id"
            ;;
        sync)
            local entity_type="$1"
            local entity_id="$2"
            if [ -z "$entity_type" ] || [ -z "$entity_id" ]; then
                log_error "Usage: $0 sync <folder|file|share> <id>"
                exit 1
            fi
            sync_to_rustfs "$entity_type" "$entity_id"
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
