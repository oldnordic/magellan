#!/usr/bin/env bash
# Nesting Depth Analysis
# Find deeply nested code using AST parent relationships
#
# Usage: ./scripts/nesting.sh [--threshold <N>] [--file <PATH>]

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
MAGENTA='\033[0;35m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_deep() { echo -e "${MAGENTA}[DEEP]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/nesting.sh [OPTIONS]

Find deeply nested code using AST parent relationships.

OPTIONS:
  --threshold <N>    Highlight nesting deeper than N levels (default: 4)
  --file <PATH>      Analyze specific file only
  --details          Show detailed nesting breakdown
  --help, -h         Show this help

NESTING LEVELS:
  1-2:  Shallow (good)
  3-4:  Moderate (acceptable)
  5+:   Deep (consider refactoring)

EXAMPLES:
  # Find deeply nested code (depth > 4)
  ./scripts/nesting.sh

  # Use different threshold
  ./scripts/nesting.sh --threshold 3

  # Analyze specific file
  ./scripts/nesting.sh --file src/main.rs

  # Show detailed breakdown
  ./scripts/nesting.sh --details
EOF
}

THRESHOLD=4
FILE=""
MODE="summary"

while [ $# -gt 0 ]; do
    case "$1" in
        --threshold)
            THRESHOLD="$2"
            shift 2
            ;;
        --file)
            FILE="$2"
            shift 2
            ;;
        --details)
            MODE="details"
            shift
            ;;
        --help|-h)
            show_usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

if [ ! -f "$DB_FILE" ]; then
    log_error "Database not found: $DB_FILE"
    log_info "Start the watcher first: ./scripts/magellan-workflow.sh start"
    exit 2
fi

# Check if ast_nodes table exists
AST_TABLE_EXISTS=$(sqlite3 "$DB_FILE" "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ast_nodes' LIMIT 1" 2>/dev/null)
if [ -z "$AST_TABLE_EXISTS" ]; then
    log_error "AST nodes table not found. Database needs to be at schema version 5."
    log_info "Run: magellan migrate --db $DB_FILE"
    exit 3
fi

log_info "Analyzing nesting depth (threshold: $THRESHOLD)..."
echo ""

FILE_FILTER=""
if [ -n "$FILE" ]; then
    FILE_FILTER="WHERE kind = '$FILE'  -- Will need adjustment for actual file filtering"
    log_info "Filtering by file: $FILE"
    echo ""
fi

# Calculate nesting depth using recursive CTE
# This finds the maximum depth of nested control structures
NESTING_QUERY="
WITH RECURSIVE
node_ancestry AS (
    -- Base case: root nodes (no parent)
    SELECT
        id,
        parent_id,
        kind,
        byte_start,
        0 as depth
    FROM ast_nodes
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case: count depth from parent
    SELECT
        a.id,
        a.parent_id,
        a.kind,
        a.byte_start,
        na.depth + 1
    FROM ast_nodes a
    JOIN node_ancestry na ON a.parent_id = na.id
),
nesting_kinds AS (
    -- Only count structural nodes for nesting
    SELECT kind, MAX(depth) as max_depth
    FROM node_ancestry
    WHERE kind IN (
        'block', 'if_expression', 'while_expression',
        'for_expression', 'loop_expression', 'match_expression',
        'closure_expression', 'unsafe_block'
    )
    GROUP BY kind
)
SELECT
    kind,
    max_depth
FROM nesting_kinds
ORDER BY max_depth DESC;
"

if [ "$MODE" = "details" ]; then
    log_info "Detailed nesting breakdown:"
    echo ""
    sqlite3 "$DB_FILE" "$NESTING_QUERY" 2>/dev/null | column -t -s '|'
    echo ""
else
    DEEP_OUTPUT=$(sqlite3 "$DB_FILE" "$NESTING_QUERY" 2>/dev/null)

    if [ -z "$DEEP_OUTPUT" ]; then
        log_info "No deeply nested code found."
    else
        DEEP_COUNT=0
        echo "$DEEP_OUTPUT" | while IFS='|' read -r kind depth; do
            depth_num=$(echo "$depth" | tr -d ' ')
            if [ "$depth_num" -gt "$THRESHOLD" ]; then
                log_deep "Maximum depth for $kind: $depth_num"
                DEEP_COUNT=$((DEEP_COUNT + 1))
            fi
        done
    fi
fi

# Show overall statistics
log_info "Overall statistics:"
echo ""

STATS_QUERY="
SELECT
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'if_expression') as if_count,
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'for_expression') as for_count,
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'while_expression') as while_count,
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'loop_expression') as loop_count,
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'match_expression') as match_count,
    (SELECT COUNT(*) FROM ast_nodes WHERE kind = 'block') as block_count;
"

sqlite3 "$DB_FILE" "$STATS_QUERY" 2>/dev/null | while IFS='|' read -r if_cnt for_cnt while_cnt loop_cnt match_cnt block_cnt; do
    echo "  if_expression:    $if_cnt"
    echo "  for_expression:   $for_cnt"
    echo "  while_expression: $while_cnt"
    echo "  loop_expression:  $loop_cnt"
    echo "  match_expression: $match_cnt"
    echo "  blocks:           $block_cnt"
done

echo ""
log_info "Tip: Use --details to see full breakdown by node kind"
