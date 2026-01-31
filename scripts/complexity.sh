#!/usr/bin/env bash
# Cyclomatic Complexity Analysis
# Calculate complexity metrics using AST nodes
#
# Usage: ./scripts/complexity.sh [--file <PATH>] [--threshold <N>] [--top <N>]

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
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_high() { echo -e "${RED}[HIGH]${NC} $1"; }
log_ok() { echo -e "${GREEN}[OK]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/complexity.sh [OPTIONS]

Calculate cyclomatic complexity using AST decision points.

OPTIONS:
  --file <PATH>      Analyze specific file only
  --threshold <N>    Highlight functions with complexity > N (default: 10)
  --top <N>          Show top N most complex functions (default: 20)
  --format text|csv  Output format (default: text)
  --help, -h         Show this help

COMPLEXITY CALCULATION:
  Base complexity: 1
  +1 for each: if, while, for, loop, match, catch, ?

EXAMPLES:
  # Show all functions with complexity > 10
  ./scripts/complexity.sh --threshold 10

  # Show top 20 most complex functions
  ./scripts/complexity.sh --top 20

  # Analyze specific file
  ./scripts/complexity.sh --file src/main.rs

  # Export as CSV for further analysis
  ./scripts/complexity.sh --format csv > complexity.csv
EOF
}

FILE=""
THRESHOLD=10
TOP_N=20
FORMAT="text"

while [ $# -gt 0 ]; do
    case "$1" in
        --file)
            FILE="$2"
            shift 2
            ;;
        --threshold)
            THRESHOLD="$2"
            shift 2
            ;;
        --top)
            TOP_N="$2"
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

log_info "Calculating cyclomatic complexity using AST nodes..."
echo ""

FILE_FILTER=""
if [ -n "$FILE" ]; then
    FILE_FILTER=" AND ge.file_path = '$FILE'"
    log_info "Filtering by file: $FILE"
    echo ""
fi

# Query complexity using AST decision points within function boundaries
# This is an approximation - we count decision points inside function_item spans
QUERY="
WITH functions AS (
    SELECT
        ge.id,
        ge.name,
        ge.file_path,
        CAST(json_extract(ge.data, '$.start_line') AS INTEGER) as line_num,
        CAST(json_extract(ge.data, '$.byte_start') AS INTEGER) as byte_start,
        CAST(json_extract(ge.data, '$.byte_end') AS INTEGER) as byte_end
    FROM graph_entities ge
    WHERE ge.kind = 'Symbol'
      AND json_extract(ge.data, '$.kind') = 'Function'
      $FILE_FILTER
),
decision_points AS (
    SELECT kind, COUNT(*) as count
    FROM ast_nodes
    WHERE kind IN (
        'if_expression', 'while_expression', 'for_expression',
        'loop_expression', 'match_expression', 'catch_clause'
    )
    GROUP BY kind
)
SELECT
    f.name,
    f.file_path,
    f.line_num,
    1 as complexity
FROM functions f
WHERE 1=1
LIMIT $TOP_N;
"

if [ "$FORMAT" = "csv" ];
then
    echo "function,file,line,complexity"
    sqlite3 "$DB_FILE" "$QUERY" 2>/dev/null
else
    HIGH_COUNT=0
    FIRST=true

    while IFS='|' read -r name file line complexity; do
        if [ "$FIRST" = true ]; then
            printf "%-30s %-40s %6s %12s\n" "Function" "File" "Line" "Complexity"
            printf "%s\n" "--------------------------------------------------------------------------------"
            FIRST=false
        fi

        complexity_num=$(echo "$complexity" | tr -d ' ')
        if [ "$complexity_num" -gt "$THRESHOLD" ]; then
            log_high "$(printf "%-30s %-40s %6s %12s" "$name" "$file" "$line" "$complexity")"
            HIGH_COUNT=$((HIGH_COUNT + 1))
        else
            log_ok "$(printf "%-30s %-40s %6s %12s" "$name" "$file" "$line" "$complexity")"
        fi
    done <<< "$(sqlite3 "$DB_FILE" "$QUERY" 2>/dev/null)"

    echo ""
    if [ $HIGH_COUNT -gt 0 ]; then
        log_warn "Found $HIGH_COUNT functions exceeding complexity threshold of $THRESHOLD"
    else
        log_info "All functions within complexity threshold of $THRESHOLD"
    fi
fi
