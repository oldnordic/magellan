#!/usr/bin/env bash
# Call Chain Analysis - Forward & Backward
# Shows the call chain for a given symbol
#
# Usage: ./scripts/call-chain.sh <symbol_name> [--direction forward|backward] [--max-depth N]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Configuration
PROJECT_NAME="${PROJECT_NAME:-magellan}"
DB_DIR="${DB_DIR:-$PROJECT_ROOT/.codemcp}"
DB_FILE="$DB_DIR/${PROJECT_NAME}.db"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_section() { echo -e "${CYAN}==>${NC} $1"; echo ""; }
log_chain() { echo -e "${BLUE}[CHAIN]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/call-chain.sh <symbol_name> [--direction forward|backward] [--max-depth N]

Shows the call chain for a given symbol.

OPTIONS:
  --direction forward|backward  Trace direction (default: forward)
  --max-depth N               Maximum depth (default: 5)
  --format tree|list         Output format (default: tree)

ENVIRONMENT VARIABLES:
  PROJECT_NAME    Database name (default: magellan)
  DB_DIR          Database directory (default: .codemcp)

DESCRIPTION:
  forward   Shows what this symbol calls (downward)
  backward  Shows what calls this symbol (upward/callers)

Examples:
  ./scripts/call-chain.sh "my_function"
  ./scripts/call-chain.sh "MyStruct::process" --direction backward --max-depth 3
  ./scripts/call-chain.sh "handle_request" --direction forward --format list
EOF
}

DIRECTION="forward"
MAX_DEPTH=5
FORMAT="tree"
SYMBOL=""

while [ $# -gt 0 ]; do
    case "$1" in
        --direction)
            DIRECTION="$2"
            shift 2
            ;;
        --max-depth)
            MAX_DEPTH="$2"
            shift 2
            ;;
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --help|-h)
            show_usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            show_usage
            exit 1
            ;;
        *)
            if [ -z "$SYMBOL" ]; then
                SYMBOL="$1"
            fi
            shift
            ;;
    esac
done

if [ -z "$SYMBOL" ]; then
    show_usage
    exit 1
fi

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

# Verify symbol exists
if ! llmgrep --db "$DB_FILE" search --query "$SYMBOL" --output json >/dev/null 2>&1; then
    log_error "Symbol not found: $SYMBOL"
    log_info "Search for it first: ./scripts/magellan-workflow.sh search \"$SYMBOL\""
    exit 1
fi

log_section "CALL CHAIN ANALYSIS for '$SYMBOL'"
log_info "Direction: $DIRECTION | Max depth: $MAX_DEPTH | Format: $FORMAT"
echo ""

# Use temp file to track visited symbols
VISIT_FILE=$(mktemp)
cleanup() { rm -f "$VISIT_FILE"; }
trap cleanup EXIT

# Traversal function
traverse_chain() {
    local symbol="$1"
    local depth="$2"
    local direction="$3"

    # Check if already visited
    if grep -qx "$symbol" "$VISIT_FILE" 2>/dev/null; then
        return
    fi
    echo "$symbol" >> "$VISIT_FILE"

    if [ "$depth" -gt "$MAX_DEPTH" ]; then
        return
    fi

    local indent=""
    for ((i=1; i<depth; i++)); do
        indent="  $indent"
    done

    # Get related symbols
    local related_symbols=()
    if [ "$direction" = "forward" ]; then
        # Get what this symbol calls/references
        mapfile -t related_symbols < <(llmgrep --db "$DB_FILE" search --query "$symbol" --mode references --limit 100 --output json 2>/dev/null | jq -r '.data.results[] | select(.name != null) | .name' 2>/dev/null | sort -u)
        chain_type="calls"
    else
        # Get what calls this symbol (incoming references)
        mapfile -t related_symbols < <(sqlite3 "$DB_FILE" "SELECT DISTINCT r.name FROM graph_entities r
            JOIN graph_edges e ON e.from_id = r.id
            JOIN graph_entities t ON e.to_id = t.id
            WHERE t.name = '$symbol' AND r.kind = 'Symbol' AND r.name != t.name
            LIMIT 100;" 2>/dev/null)
        chain_type="called by"
    fi

    if [ "${#related_symbols[@]}" -gt 0 ]; then
        if [ "$FORMAT" = "tree" ]; then
            echo "${indent}├─ $symbol ($chain_type ${#related_symbols[@]} symbols)"
        fi
        for related in "${related_symbols[@]}"; do
            if [ -n "$related" ]; then
                if [ "$FORMAT" = "tree" ]; then
                    echo "${indent}|  ├─→ $related"
                fi
                traverse_chain "$related" $((depth + 1)) "$direction"
            fi
        done
    else
        if [ "$FORMAT" = "tree" ]; then
            echo "${indent}└─ $symbol (leaf)"
        fi
    fi
}

# Run traversal
traverse_chain "$SYMBOL" 1 "$DIRECTION"

# Count visited symbols
visited_count=$(wc -l < "$VISIT_FILE" | tr -d ' ')

echo ""
log_info "Done. Total unique symbols visited: $visited_count"
