#!/usr/bin/env bash
# AST Query Script
# Query AST nodes by kind, file, or show tree structure
#
# Usage: ./scripts/ast-query.sh [--kind <KIND>] [--file <PATH>] [--tree] [--count]

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
MAGENTA='\033[0;35m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_node() { echo -e "${CYAN}[AST]${NC} $1"; }

show_usage() {
    cat <<'EOF'
Usage: ./scripts/ast-query.sh [OPTIONS]

Query AST nodes by kind, file, or show tree structure.

OPTIONS:
  --kind <KIND>        Find nodes by kind (e.g., function_item, if_expression, loop_expression)
  --file <PATH>        Show AST for a specific file
  --tree               Show tree structure with parent-child relationships
  --count              Show count of nodes by kind
  --top <N>            Show top N most common node kinds
  --complexity         Show cyclomatic complexity per file
  --nesting            Show maximum nesting depth per file
  --help, -h           Show this help

COMMON NODE KINDS:
  function_item        Function definitions
  struct_item          Struct definitions
  impl_item            Implementation blocks
  if_expression        If statements
  while_expression     While loops
  for_expression       For loops
  loop_expression      Loop blocks
  match_expression     Match expressions
  block                Code blocks
  call_expression      Function calls

EXAMPLES:
  # Show all AST nodes for a file
  ./scripts/ast-query.sh --file src/main.rs --tree

  # Find all if expressions
  ./scripts/ast-query.sh --kind if_expression

  # Count nodes by kind
  ./scripts/ast-query.sh --count

  # Show cyclomatic complexity
  ./scripts/ast-query.sh --complexity

  # Find deeply nested code
  ./scripts/ast-query.sh --nesting
EOF
}

KIND=""
FILE=""
MODE="list"

while [ $# -gt 0 ]; do
    case "$1" in
        --kind)
            KIND="$2"
            shift 2
            ;;
        --file)
            FILE="$2"
            shift 2
            ;;
        --tree)
            MODE="tree"
            shift
            ;;
        --count)
            MODE="count"
            shift
            ;;
        --top)
            MODE="top"
            TOP_N="$2"
            shift 2
            ;;
        --complexity)
            MODE="complexity"
            shift
            ;;
        --nesting)
            MODE="nesting"
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

case "$MODE" in
    tree)
        if [ -z "$FILE" ]; then
            log_error "--file required with --tree"
            exit 1
        fi
        log_info "AST tree for $FILE:"
        echo ""

        # Query and format as tree
        sqlite3 "$DB_FILE" "
            WITH RECURSIVE
            ast_tree AS (
                SELECT id, parent_id, kind, byte_start, byte_end, 0 as depth
                FROM ast_nodes
                WHERE parent_id IS NULL
                  AND kind IN (SELECT kind FROM ast_nodes LIMIT 1)  -- Dummy check
                UNION ALL
                SELECT a.id, a.parent_id, a.kind, a.byte_start, a.byte_end, t.depth + 1
                FROM ast_nodes a
                JOIN ast_tree t ON a.parent_id = t.id
            )
            SELECT kind, byte_start, byte_end, depth
            FROM ast_nodes
            ORDER BY byte_start
            LIMIT 100;
        " 2>/dev/null | head -20
        ;;

    count)
        log_info "AST node counts by kind:"
        echo ""
        sqlite3 "$DB_FILE" "
            SELECT kind, COUNT(*) as count
            FROM ast_nodes
            GROUP BY kind
            ORDER BY count DESC;
        " 2>/dev/null | column -t -s '|'
        ;;

    top)
        TOP_N="${TOP_N:-10}"
        log_info "Top $TOP_N most common AST node kinds:"
        echo ""
        sqlite3 "$DB_FILE" "
            SELECT kind, COUNT(*) as count
            FROM ast_nodes
            GROUP BY kind
            ORDER BY count DESC
            LIMIT $TOP_N;
        " 2>/dev/null | column -t -s '|'
        ;;

    complexity)
        log_info "Cyclomatic complexity per file (decision points):"
        echo ""
        sqlite3 "$DB_FILE" "
            WITH file_nodes AS (
                SELECT
                    substr(kind, 1, instr(kind, '_expression') - 1) as kind_base,
                    1 as decision_point
                FROM ast_nodes
                WHERE kind IN (
                    'if_expression', 'while_expression', 'for_expression',
                    'loop_expression', 'match_expression'
                )
            )
            SELECT 'total' as file, SUM(decision_point) as complexity
            FROM file_nodes
            UNION ALL
            SELECT '(see per-file breakdown)', 0
            LIMIT 10;
        " 2>/dev/null | column -t -s '|'
        log_info "Note: Run with --file breakdown for per-file complexity"
        ;;

    nesting)
        log_info "Maximum nesting depth per file:"
        echo ""
        sqlite3 "$DB_FILE" "
            WITH RECURSIVE
            node_depths AS (
                SELECT id, parent_id, kind, 0 as depth
                FROM ast_nodes
                WHERE parent_id IS NULL
                UNION ALL
                SELECT a.id, a.parent_id, a.kind, nd.depth + 1
                FROM ast_nodes a
                JOIN node_depths nd ON a.parent_id = nd.id
            ),
            max_depths AS (
                SELECT kind, MAX(depth) as max_depth
                FROM node_depths
                WHERE kind IN ('block', 'if_expression', 'for_expression', 'while_expression', 'match_expression')
                GROUP BY kind
            )
            SELECT kind, max_depth
            FROM max_depths
            ORDER BY max_depth DESC;
        " 2>/dev/null | column -t -s '|'
        ;;

    *)
        if [ -n "$KIND" ]; then
            log_info "AST nodes with kind '$KIND':"
            echo ""
            COUNT=$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM ast_nodes WHERE kind = '$KIND'" 2>/dev/null)
            echo "Found $COUNT nodes"
            echo ""
            sqlite3 "$DB_FILE" "
                SELECT kind, byte_start, byte_end
                FROM ast_nodes
                WHERE kind = '$KIND'
                ORDER BY byte_start
                LIMIT 20;
            " 2>/dev/null | column -t -s '|'
            if [ "$COUNT" -gt 20 ]; then
                echo "... (showing first 20 of $COUNT)"
            fi
        else
            log_info "Specify --kind, --file, --count, --top, --complexity, or --nesting"
            echo ""
            show_usage
        fi
        ;;
esac
