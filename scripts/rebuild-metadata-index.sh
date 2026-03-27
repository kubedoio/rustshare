#!/bin/bash
# Metadata index rebuild script
#
# Rebuilds indexes from source objects. Use when indexes are corrupted
# or after data recovery operations.

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

# Rebuild folder children index
rebuild_folder_children() {
    local folder_id="$1"
    
    if [ -n "$folder_id" ]; then
        log_section "Rebuild Children Index for Folder: $folder_id"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild children index for folder $folder_id"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/folder/${folder_id}/children" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    else
        log_section "Rebuild All Folder Children Indexes"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild all folder children indexes"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/indexes/folder-children" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    fi
}

# Rebuild user folders index
rebuild_user_folders() {
    local user_id="$1"
    
    if [ -n "$user_id" ]; then
        log_section "Rebuild Folders Index for User: $user_id"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild folders index for user $user_id"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/user/${user_id}/folders" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    else
        log_section "Rebuild All User Folder Indexes"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild all user folder indexes"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/indexes/user-folders" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    fi
}

# Rebuild file versions index
rebuild_file_versions() {
    local file_id="$1"
    
    if [ -n "$file_id" ]; then
        log_section "Rebuild Versions Index for File: $file_id"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild versions index for file $file_id"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/file/${file_id}/versions" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    else
        log_section "Rebuild All File Version Indexes"
        
        if [ "$DRY_RUN" = true ]; then
            log_warn "DRY RUN - Would rebuild all file version indexes"
            return 0
        fi
        
        local response
        response=$(api_call "POST" "/rebuild/indexes/file-versions" 2>/dev/null || echo '{"error": "endpoint not available"}')
        
        echo "$response" | jq .
    fi
}

# Rebuild all indexes
rebuild_all() {
    log_section "Rebuild All Indexes"
    
    if [ "$DRY_RUN" = true ]; then
        log_warn "DRY RUN mode - no changes will be made"
        log_warn "To actually rebuild, run with --execute flag"
        
        log_info "Would rebuild:"
        log_info "  - All folder children indexes"
        log_info "  - All user folder indexes"
        log_info "  - All file version indexes"
        return 0
    fi
    
    log_warn "Rebuilding all indexes..."
    
    local response
    response=$(api_call "POST" "/rebuild/indexes/all" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
    
    local success
    success=$(echo "$response" | jq -r '.success // false')
    
    if [ "$success" = "true" ]; then
        log_info "All indexes rebuilt successfully"
        return 0
    else
        log_error "Some indexes failed to rebuild"
        return 1
    fi
}

# Check index status
check_index_status() {
    log_section "Index Status"
    
    local response
    response=$(api_call "GET" "/stats/indexes" 2>/dev/null || echo '{"error": "endpoint not available"}')
    
    echo "$response" | jq .
}

# Show usage
usage() {
    cat <<EOF
Usage: $0 [command] [options] [args]

Commands:
  all                       Rebuild all indexes
  folder-children [id]      Rebuild folder children index (for all or specific folder)
  user-folders [user_id]    Rebuild user folders index (for all or specific user)
  file-versions [file_id]   Rebuild file versions index (for all or specific file)
  status                    Check index status
  help                      Show this help

Options:
  --admin-url <url>         Admin API base URL (default: http://localhost:8080/api/admin/metadata)
  --api-key <key>           API key for authentication
  --execute                 Actually perform rebuild (default: dry run)
  --yes                     Skip confirmation prompts

Environment Variables:
  RUSTSHARE_ADMIN_URL       Admin API base URL
  RUSTSHARE_ADMIN_API_KEY   API key for authentication

Examples:
  # Dry run - see what would be rebuilt
  $0 all

  # Actually rebuild all indexes
  $0 --execute all

  # Rebuild specific folder's children index
  $0 --execute folder-children 550e8400-e29b-41d4-a716-446655440000

  # Rebuild all user folder indexes
  $0 --execute user-folders

  # Check index status
  $0 status

WARNING:
  Index rebuilds can be resource-intensive on large datasets.
  Consider running during low-traffic periods.

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
    log_warn "Index rebuilds can be resource-intensive. Continue?"
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
            rebuild_all
            ;;
        folder-children)
            local folder_id="${1:-}"
            rebuild_folder_children "$folder_id"
            ;;
        user-folders)
            local user_id="${1:-}"
            rebuild_user_folders "$user_id"
            ;;
        file-versions)
            local file_id="${1:-}"
            rebuild_file_versions "$file_id"
            ;;
        status)
            check_index_status
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
