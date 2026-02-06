#!/usr/bin/env bash
# Module Dependency Analysis
# Shows file-to-file dependencies and hotspots
#
# Usage: ./scripts/module-deps.sh [--format table|dot|json]

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

show_usage() {
    cat <<'EOF'
Usage: ./scripts/module-deps.sh [--format table|dot|json] [--min-refs N]

Analyzes module-level dependencies and hotspots.

OPTIONS:
  --format table|dot|json  Output format (default: table)
  --min-refs N             Show only modules with at least N references (default: 1)
  --limit N                Limit results (default: 20)

ENVIRONMENT VARIABLES:
  PROJECT_NAME    Database name (default: magellan)
  DB_DIR          Database directory (default: .codemcp)

DESCRIPTION:
  table     Show dependency matrix as ASCII table
  dot       Generate Graphviz DOT format for visualization
  json      Output machine-readable JSON

Examples:
  ./scripts/module-deps.sh
  ./scripts/module-deps.sh --format dot > deps.dot && dot -Tpng deps.dot -o deps.png
  ./scripts/module-deps.sh --format json --min-refs 5
EOF
}

FORMAT="table"
MIN_REFS=1
LIMIT=20

while [ $# -gt 0 ]; do
    case "$1" in
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --min-refs)
            MIN_REFS="$2"
            shift 2
            ;;
        --limit)
            LIMIT="$2"
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

# Check if jq is available
if ! command -v jq &> /dev/null; then
    log_error "jq not found in PATH (required for JSON parsing)"
    exit 3
fi

# Check if llmgrep is available
if ! command -v llmgrep &> /dev/null; then
    log_error "llmgrep not found in PATH"
    exit 3
fi

log_section "MODULE DEPENDENCY ANALYSIS for $PROJECT_NAME"
log_info "Format: $FORMAT | Min refs: $MIN_REFS | Limit: $LIMIT"
echo ""

# First, try to use file_metrics table if available (Phase 34+)
if sqlite3 "$DB_FILE" "SELECT name FROM sqlite_master WHERE name='file_metrics'" 2>/dev/null | grep -q file_metrics; then
    log_info "Using file_metrics table (Phase 34 schema)"

    case "$FORMAT" in
        table)
            sqlite3 "$DB_FILE" "SELECT file_path, symbol_count, loc, fan_in, fan_out, complexity_score
                                         FROM file_metrics
                                         WHERE fan_in >= $MIN_REFS OR fan_out >= $MIN_REFS
                                         ORDER BY complexity_score DESC
                                         LIMIT $LIMIT" 2>/dev/null | column -t -s '|'
            ;;
        json)
            sqlite3 "$DB_FILE" "SELECT json_object(
                'file_path', file_path,
                'symbol_count', symbol_count,
                'loc', loc,
                'fan_in', fan_in,
                'fan_out', fan_out,
                'complexity_score', complexity_score
            ) FROM file_metrics
            WHERE fan_in >= $MIN_REFS OR fan_out >= $MIN_REFS
            ORDER BY complexity_score DESC
            LIMIT $LIMIT" 2>/dev/null | jq -r '.'
            ;;
        dot)
            log_error "DOT format requires file_metrics table (Phase 34+)"
            ;;
    esac
else
    log_info "file_metrics table not found, using direct queries"

    # Fallback: Build dependency matrix from graph_entities
    case "$FORMAT" in
        table)
            echo "File                                              | In     | Out    | Total"
            echo "-------------------------------------------------- | ------ | ------ | -----"

            # Get unique files with function counts
            # Query database directly for cross-file references
            sqlite3 "$DB_FILE" "
            SELECT
                r.file_path,
                (SELECT COUNT(*) FROM graph_edges e
                 JOIN graph_entities t ON e.to_id = t.id
                 WHERE e.from_id = r.id AND t.file_path != r.file_path) as outgoing,
                (SELECT COUNT(*) FROM graph_edges e
                 JOIN graph_entities f ON e.from_id = f.id
                 WHERE e.to_id = r.id AND f.file_path != r.file_path) as incoming
            FROM graph_entities r
            WHERE r.kind = 'Symbol' AND r.file_path LIKE '%/src/%'
            GROUP BY r.file_path
            HAVING (outgoing + incoming) >= $MIN_REFS
            ORDER BY (outgoing + incoming) DESC
            LIMIT $LIMIT;" 2>/dev/null | while IFS='|' read -r file outgoing incoming; do
                total=$((outgoing + incoming))
                printf "%-50s | %6d | %6d | %6d\n" "$file" "$incoming" "$outgoing" "$total"
            done
            ;;
        json)
            echo "["
            first=true
            sqlite3 "$DB_FILE" "
            SELECT
                '{\"file\": \"' || r.file_path || '\", \"refs\": ' ||
                ((SELECT COUNT(*) FROM graph_edges e
                  JOIN graph_entities t ON e.to_id = t.id
                  WHERE e.from_id = r.id AND t.file_path != r.file_path) +
                 (SELECT COUNT(*) FROM graph_edges e
                  JOIN graph_entities f ON e.from_id = f.id
                  WHERE e.to_id = r.id AND f.file_path != r.file_path)) || '}'
            FROM graph_entities r
            WHERE r.kind = 'Symbol' AND r.file_path LIKE '%/src/%'
            GROUP BY r.file_path
            ORDER BY (SELECT COUNT(*) FROM graph_edges e WHERE e.from_id = r.id) DESC
            LIMIT $LIMIT;" 2>/dev/null | while read -r line; do
                [ "$first" = true ] && first=false || echo ","
                echo "$line"
            done
            echo "]"
            ;;
        dot)
            log_error "DOT format requires file_metrics table"
            ;;
    esac
fi
echo ""
log_info "Done."
