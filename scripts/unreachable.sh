#!/usr/bin/env bash
# Unreachable Code Detection
# Finds functions that are never called from any entry point
#
# Usage: ./scripts/unreachable.sh [--format list|summary]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Configuration (can be overridden via env vars)
PROJECT_NAME="${PROJECT_NAME:-magellan}"
DB_DIR="${DB_DIR:-$PROJECT_ROOT/.codemcp}"
DB_FILE="$DB_DIR/${PROJECT_NAME}.db"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_result() { echo -e "${CYAN}[DEAD]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/unreachable.sh [--format list|summary]

Finds functions that are never called from any entry point.

OPTIONS:
  --format list|summary  Output format (default: summary)

ENVIRONMENT VARIABLES:
  PROJECT_NAME    Database name (default: magellan)
  DB_DIR          Database directory (default: .codemcp)

Examples:
  ./scripts/unreachable.sh
  ./scripts/unreachable.sh --format list
EOF
}

FORMAT="summary"

while [ $# -gt 0 ]; do
    case "$1" in
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --help|-h)
            show_usage
            exit 0
            ;;
        *)
            shift
            ;;
    esac
done

if [ ! -f "$DB_FILE" ]; then
    log_error "Database not found: $DB_FILE"
    log_info "Start the watcher first: ./scripts/magellan-workflow.sh start"
    exit 2
fi

# Check if llmgrep is available
if ! command -v llmgrep &> /dev/null; then
    log_error "llmgrep not found in PATH"
    exit 3
fi

log_info "Scanning for unreachable code in $PROJECT_NAME..."
echo ""

# Use direct SQL query for efficiency - find functions with zero incoming references
log_info "Querying database for unreferenced functions..."

UNREACHABLE_OUTPUT=$(sqlite3 "$DB_FILE" "
    SELECT
        ge2.name,
        ge2.file_path,
        json_extract(ge2.data, '$.start_line')
    FROM graph_entities ge2
    WHERE ge2.kind = 'Symbol'
      AND json_extract(ge2.data, '$.kind') IN ('Function', 'Method')
      AND ge2.file_path LIKE '%/src/%'
      AND NOT EXISTS (
          -- Has no incoming REFERENCE or CALL edges (only DEFINES means unreferenced)
          SELECT 1 FROM graph_edges e
          WHERE e.to_id = ge2.id
            AND e.edge_type IN ('REFERENCES', 'CALLS', 'CALLER')
      )
      AND ge2.name NOT LIKE 'test_%'
    ORDER BY ge2.file_path, ge2.name;
" 2>/dev/null)

if [ -z "$UNREACHABLE_OUTPUT" ]; then
    log_info "No unreachable code found - clean!"
    exit 0
fi

UNREACHABLE_COUNT=$(echo "$UNREACHABLE_OUTPUT" | wc -l)

echo "$UNREACHABLE_OUTPUT" | while IFS='|' read -r name file line; do
    log_result "UNREACHABLE: $name"
    echo "       at $file:$line"
done

echo ""
log_info "Total unreachable public functions: $UNREACHABLE_COUNT"

if [ "$FORMAT" = "list" ]; then
    echo ""
    log_info "Full list:"
    echo "$UNREACHABLE_OUTPUT" | while IFS='|' read -r name file line; do
        echo "  - $name ($file:$line)"
    done
fi

if [ $UNREACHABLE_COUNT -eq 0 ]; then
    log_info "No unreachable code found - clean!"
    exit 0
else
    exit 1
fi
