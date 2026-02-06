#!/usr/bin/env bash
# Blast Zone Analysis - Impact Analysis
# Shows what would be affected if a symbol changes
#
# Usage: ./scripts/blast-zone.sh <symbol_name> [--max-depth N] [--format tree|list]

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
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_section() { echo -e "${CYAN}==>${NC} $1"; echo ""; }
log_result() { echo -e "${CYAN}[IMPACT]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/blast-zone.sh <symbol_name> [--max-depth N] [--format tree|list]

Shows forward impact analysis: what functions would be affected
if this symbol changed?

OPTIONS:
  --max-depth N    Maximum depth to traverse (default: 3)
  --format tree|list  Output format (default: tree)

ENVIRONMENT VARIABLES:
  PROJECT_NAME    Database name (default: magellan)
  DB_DIR          Database directory (default: .codemcp)

Examples:
  ./scripts/blast-zone.sh "my_function"
  ./scripts/blast-zone.sh "MyStruct::method" --max-depth 2
  ./scripts/blast-zone.sh "process_file" --format list
EOF
}

if [ $# -lt 1 ]; then
    show_usage
    exit 1
fi

SYMBOL="$1"
MAX_DEPTH=3
FORMAT="tree"

while [ $# -gt 0 ]; do
    case "$1" in
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

# First verify symbol exists
if ! llmgrep --db "$DB_FILE" search --query "$SYMBOL" --output json >/dev/null 2>&1; then
    log_error "Symbol not found: $SYMBOL"
    log_info "Search for it first: ./scripts/magellan-workflow.sh search \"$SYMBOL\""
    exit 1
fi

log_section "BLAST ZONE ANALYSIS for '$SYMBOL'"
log_info "Max depth: $MAX_DEPTH | Format: $FORMAT"
echo ""

# Use temp files to track visited symbols and impact levels
VISIT_FILE=$(mktemp)
IMPACT_FILE=$(mktemp)
cleanup() { rm -f "$VISIT_FILE" "$IMPACT_FILE"; }
trap cleanup EXIT

# Recursive traversal
traverse_impact() {
    local symbol="$1"
    local depth="$2"
    local path="$3"

    # Skip if already visited
    if grep -qx "$symbol" "$VISIT_FILE" 2>/dev/null; then
        return
    fi
    echo "$symbol" >> "$VISIT_FILE"

    # Calculate impact level
    local level=$((MAX_DEPTH - depth + 1))
    echo "$symbol:$level" >> "$IMPACT_FILE"

    if [ "$depth" -gt "$MAX_DEPTH" ]; then
        return
    fi

    local indent=""
    for ((i=1; i<depth; i++)); do
        indent="  $indent"
    done

    # Get all symbols this symbol references
    local refs
    refs=$(llmgrep --db "$DB_FILE" search --query "$symbol" --mode references --limit 500 --output json 2>/dev/null | jq -r '.data.results[] | select(.name != null) | .name' 2>/dev/null | sort -u)

    if [ -n "$refs" ]; then
        local ref_count=$(echo "$refs" | wc -l)
        if [ "$FORMAT" = "tree" ]; then
            echo "${indent}├─ $symbol (depth $depth, affects $ref_count symbols)"
        fi
        echo "$refs" | while read -r ref; do
            if [ -n "$ref" ]; then
                if [ "$FORMAT" = "tree" ]; then
                    echo "${indent}|  ├─→ $ref"
                fi
                traverse_impact "$ref" $((depth + 1)) "${path}→${symbol}"
            fi
        done
    else
        if [ "$FORMAT" = "tree" ]; then
            echo "${indent}└─ $symbol (leaf)"
        fi
    fi
}

# Run traversal
traverse_impact "$SYMBOL" 1 ""

# Summary
echo ""
log_section "IMPACT SUMMARY"

# Count affected symbols
affected_count=$(wc -l < "$VISIT_FILE" | tr -d ' ')
log_info "Total symbols affected: $affected_count"

# Group by level
echo ""
echo "Impact by level:"
for level in $(seq 1 $MAX_DEPTH); do
    count=$(grep -c ":$level$" "$IMPACT_FILE" 2>/dev/null || echo "0")
    if [ "$count" -gt 0 ]; then
        echo "  Level $level: $count symbol(s)"
    fi
done

# List all affected symbols if format=list
if [ "$FORMAT" = "list" ]; then
    echo ""
    log_info "Affected symbols:"
    while IFS=: read -r sym level; do
        echo "  [$level] $sym"
    done < "$IMPACT_FILE" | sort -t':' -k2 -n
fi

echo ""
log_info "Done."
