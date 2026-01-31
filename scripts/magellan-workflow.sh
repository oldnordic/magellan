#!/bin/bash
# Magellan Workflow - Project-Agnostic Database Management
#
# This is a template script - copy to your project and customize DB_FILE, SRC_DIR, and PROJECT_NAME
#
# Usage: ./scripts/magellan-workflow.sh [command] [args]
#
# Version: 1.8.0 - Supports metrics, chunks, safe UTF-8 extraction

set -e

# Project Configuration - CUSTOMIZE THESE FOR YOUR PROJECT
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_NAME="${PROJECT_NAME:-magellan}"  # Override with env var
DB_DIR="${DB_DIR:-$PROJECT_ROOT/.codemcp}"
DB_FILE="$DB_DIR/${PROJECT_NAME}.db"
PID_FILE="$DB_DIR/magellan-watcher.pid"
SRC_DIR="${SRC_DIR:-$PROJECT_ROOT/src}"

# Ensure db directory exists
mkdir -p "$DB_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_cmd() { echo -e "${CYAN}[CMD]${NC} $1"; }
log_success() { echo -e "${MAGENTA}[OK]${NC} $1"; }

# Check if magellan is installed
check_tools() {
    if ! command -v magellan &> /dev/null; then
        log_error "magellan not found in PATH"
        log_info "Install from: https://github.com/elsaland/magellan"
        log_info "Or build with: cargo install --path ."
        exit 1
    fi
    if ! command -v llmgrep &> /dev/null; then
        log_error "llmgrep not found in PATH"
        log_info "Install from: https://github.com/elsaland/magellan"
        exit 1
    fi
}

# Check if watcher is running
is_running() {
    if [ -f "$PID_FILE" ]; then
        local pid=$(cat "$PID_FILE" 2>/dev/null || echo "")
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

# Start the watcher
start_watcher() {
    check_tools

    if is_running; then
        log_warn "Magellan watcher is already running (PID: $(cat $PID_FILE))"
        return 0
    fi

    log_info "Starting Magellan watcher for $PROJECT_NAME..."
    log_info "  Source: $SRC_DIR"
    log_info "  Database: $DB_FILE"

    magellan watch --root "$SRC_DIR" --db "$DB_FILE" --debounce-ms 500 &
    local pid=$!
    echo $pid > "$PID_FILE"

    # Give it a moment to start
    sleep 2

    if kill -0 $pid 2>/dev/null; then
        log_info "Watcher started (PID: $pid)"
        log_info "Database will auto-update on file changes"
    else
        log_error "Failed to start watcher"
        rm -f "$PID_FILE"
        exit 1
    fi
}

# Stop the watcher
stop_watcher() {
    if ! is_running; then
        log_warn "Magellan watcher is not running"
        rm -f "$PID_FILE"
        return 0
    fi

    local pid=$(cat "$PID_FILE")
    log_info "Stopping Magellan watcher (PID: $pid)..."
    kill $pid 2>/dev/null || true
    rm -f "$PID_FILE"

    # Wait for process to die
    local count=0
    while kill -0 $pid 2>/dev/null; do
        sleep 0.1
        count=$((count + 1))
        if [ $count -gt 50 ]; then
            log_warn "Process did not stop gracefully, forcing..."
            kill -9 $pid 2>/dev/null || true
            break
        fi
    done

    log_info "Watcher stopped"
}

# Show status
show_status() {
    check_tools

    echo ""
    echo "=== Magellan Status for $PROJECT_NAME ==="
    echo ""

    if is_running; then
        log_info "Watcher: RUNNING (PID: $(cat $PID_FILE))"
    else
        log_warn "Watcher: NOT RUNNING"
    fi

    echo ""
    echo "Database: $DB_FILE"

    if [ -f "$DB_FILE" ]; then
        echo ""
        magellan status --db "$DB_FILE"
    else
        log_warn "Database file does not exist (start watcher to create)"
    fi

    echo ""
}

# Search with llmgrep
search_symbol() {
    check_tools

    if [ -z "$1" ]; then
        log_error "Usage: $0 search <query> [--mode symbols|references|calls] [--output format]"
        exit 1
    fi

    local query="$1"
    local mode="symbols"
    local output_format="human"
    local with_snippet=""
    local with_context=""

    # Parse optional arguments
    shift
    while [ $# -gt 0 ]; do
        case "$1" in
            --mode)
                mode="$2"
                shift 2
                ;;
            --output)
                output_format="$2"
                shift 2
                ;;
            --with-snippet)
                with_snippet="--with-snippet"
                shift
                ;;
            --with-context)
                with_context="--with-context"
                shift
                ;;
            *)
                shift
                ;;
        esac
    done

    # Default for human output is to include snippets
    if [ "$output_format" = "human" ] && [ -z "$with_snippet" ]; then
        with_snippet="--with-snippet"
    fi

    log_cmd "llmgrep --db $DB_FILE search --query \"$query\" --mode $mode $with_snippet $with_context --output $output_format"
    echo ""
    llmgrep --db "$DB_FILE" search --query "$query" --mode "$mode" $with_snippet $with_context --output "$output_format"
}

# Get symbol with full context
get_symbol() {
    check_tools

    if [ -z "$1" ]; then
        log_error "Usage: $0 get <symbol_name> [--file <path>]"
        exit 1
    fi

    local name="$1"
    local file="$2"

    if [ -n "$file" ]; then
        magellan get --db "$DB_FILE" --file "$file" --symbol "$name" --with-context
    else
        llmgrep --db "$DB_FILE" search --query "$name" --with-snippet --with-context --output pretty
    fi
}

# Check wire-up (API drift detection)
# Returns number of references. Zero = NOT WIRED.
check_wire() {
    if [ -z "$1" ]; then
        log_error "Usage: $0 check-wire <symbol_name>"
        exit 1
    fi

    local name="$1"

    # Get reference count
    local output
    output=$(llmgrep --db "$DB_FILE" search --query "$name" --mode references --output json 2>/dev/null || echo "null")

    if [ "$output" = "null" ]; then
        log_error "Database query failed"
        return 2
    fi

    local count
    count=$(echo "$output" | jq -r '.data.results | length // 0' 2>/dev/null || echo "0")

    if [ "$count" -eq 0 ]; then
        log_error "NOT WIRED: '$name' has 0 references"
        return 1
    else
        log_info "WIRED: '$name' has $count reference(s)"
        echo "$output" | jq -r '.data.results[] | "\(.span.file_path):\(.span.start_line)"' 2>/dev/null || true
        return 0
    fi
}

# Find references
find_refs() {
    check_tools

    if [ -z "$1" ]; then
        log_error "Usage: $0 refs <symbol_name> [--file <path>]"
        exit 1
    fi

    local name="$1"
    local file="$2"

    if [ -n "$file" ]; then
        magellan refs --db "$DB_FILE" --name "$name" --path "$file" --direction in
    else
        llmgrep --db "$DB_FILE" search --query "$name" --mode references --output pretty
    fi
}

# List files in database
list_files() {
    check_tools
    magellan files --db "$DB_FILE" --symbols
}

# Query a specific file
query_file() {
    check_tools

    if [ -z "$1" ]; then
        log_error "Usage: $0 query-file <file_path>"
        exit 1
    fi

    magellan query --db "$DB_FILE" --file "$1" --with-context
}

# Rebuild database from scratch
rebuild() {
    log_info "Rebuilding database..."
    stop_watcher
    rm -f "$DB_FILE"
    start_watcher
}

# Show hotspots (files with highest complexity)
show_hotspots() {
    check_tools

    local limit="${1:-20}"

    log_info "Top $limit files by complexity score..."
    echo ""

    # Query using file_metrics table if available (Phase 34+)
    if sqlite3 "$DB_FILE" "SELECT name FROM sqlite_master WHERE name='file_metrics'" 2>/dev/null | grep -q .; then
        echo -e "file_path\tsymbols\tLOC\tfan_in\tfan_out\tcomplexity"
        sqlite3 "$DB_FILE" "SELECT file_path, symbol_count, loc, fan_in, fan_out, complexity_score
                                     FROM file_metrics
                                     ORDER BY complexity_score DESC
                                     LIMIT $limit" 2>/dev/null | column -t -s '|'
    else
        log_warn "file_metrics table not found (requires Phase 34 schema)"
        log_info "Showing top files by symbol count instead..."
        magellan files --db "$DB_FILE" | head -n $((limit + 1))
    fi
}

# Show all code chunks
show_chunks() {
    check_tools

    local limit="${1:-50}"
    local file_filter="${2:-}"
    local kind_filter="${3:-}"
    local output_format="${4:-human}"

    log_cmd "magellan chunks --db $DB_FILE --limit $limit ${file:+--file $file_filter} ${kind:+--kind $kind} --output $output_format"
    magellan chunks --db "$DB_FILE" --limit "$limit" ${file_filter:+--file "$file_filter"} ${kind_filter:+--kind "$kind_filter"} --output "$output_format"
}

# Get chunk by symbol name
chunk_by_symbol() {
    check_tools

    if [ -z "$1" ]; then
        log_error "Usage: $0 chunk-by-symbol <symbol_name> [--file <pattern>] [--output <format>]"
        exit 1
    fi

    local symbol="$1"
    local file_pattern=""
    local output_format="human"

    shift
    while [ $# -gt 0 ]; do
        case "$1" in
            --file)
                file_pattern="$2"
                shift 2
                ;;
            --output)
                output_format="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done

    magellan chunk-by-symbol --db "$DB_FILE" --symbol "$symbol" ${file_pattern:+--file "$file_pattern"} --output "$output_format"
}

# Get chunk by byte span
chunk_by_span() {
    check_tools

    if [ -z "$3" ]; then
        log_error "Usage: $0 chunk-by-span <file_path> <start> <end> [--output <format>]"
        exit 1
    fi

    local file_path="$1"
    local start="$2"
    local end="$3"
    local output_format="${4:-human}"

    magellan chunk-by-span --db "$DB_FILE" --file "$file_path" --start "$start" --end "$end" --output "$output_format"
}

# Show symbol metrics for a file
show_file_metrics() {
    check_tools

    local file_path="$1"

    if [ -z "$file_path" ]; then
        log_error "Usage: $0 file-metrics <file_path>"
        exit 1
    fi

    log_info "Metrics for $file_path:"
    echo ""

    sqlite3 "$DB_FILE" "SELECT * FROM file_metrics WHERE file_path = '$file_path'" 2>/dev/null || {
        log_warn "No metrics found for this file"
    }
}

# Trigger metrics backfill
backfill_metrics() {
    check_tools

    log_info "Triggering metrics backfill..."
    log_warn "This requires opening the database with CodeGraph"
    log_info "Run: magellan migrate --db $DB_FILE"
    log_info "Or delete and reindex: $0 rebuild"
}

# Main command dispatcher
case "${1:-status}" in
    start)
        start_watcher
        ;;
    stop)
        stop_watcher
        ;;
    status)
        show_status
        ;;
    restart)
        stop_watcher
        sleep 1
        start_watcher
        ;;
    search)
        shift  # Remove 'search' from args
        search_symbol "$@"
        ;;
    get)
        shift  # Remove 'get' from args
        get_symbol "$@"
        ;;
    refs)
        shift  # Remove 'refs' from args
        find_refs "$@"
        ;;
    check-wire|wire-check)
        shift
        check_wire "$@"
        ;;
    files)
        list_files
        ;;
    query-file)
        query_file "$2"
        ;;
    rebuild)
        rebuild
        ;;
    hotspots)
        show_hotspots "${2:-20}"
        ;;
    chunks)
        show_chunks "${2:-50}" "${3:-}" "${4:-}" "${5:-human}"
        ;;
    chunk-by-symbol)
        shift
        chunk_by_symbol "$@"
        ;;
    chunk-by-span)
        shift
        chunk_by_span "$@"
        ;;
    file-metrics)
        show_file_metrics "$2"
        ;;
    backfill)
        backfill_metrics
        ;;
    *)
        cat << 'EOF'
Usage: ./scripts/magellan-workflow.sh <command> [args]

Commands:
  start              Start the Magellan watcher
  stop               Stop the Magellan watcher
  status             Show watcher and database status
  restart            Restart the watcher
  search <query>     Search for symbols (with --mode and --output options)
  get <name>         Get symbol code with context
  refs <name>        Find references to a symbol
  check-wire <name>  Check if symbol is wired (has callers/references)
  files              List all indexed files
  query-file <path>  Query symbols in a specific file
  rebuild            Rebuild database from scratch
  hotspots [n]        Show top N files by complexity score
  chunks [n] [file] [kind] [fmt]  List code chunks (n=50, filter by file/kind)
  chunk-by-symbol <name> [--file <pat>]  Get all chunks for a symbol
  chunk-by-span <file> <start> <end>   Get chunk by byte range
  file-metrics <path> Show metrics for a specific file
  backfill           Trigger metrics backfill (requires rebuild)

Environment Variables:
  PROJECT_NAME       Database name (default: magellan)
  DB_DIR             Database directory (default: .codemcp)
  SRC_DIR            Source directory (default: src)

Examples:
  ./scripts/magellan-workflow.sh start
  ./scripts/magellan-workflow.sh search "MyFunction"
  ./scripts/magellan-workflow.sh search "load" --mode references --output json
  ./scripts/magellan-workflow.sh get "my_function"
  ./scripts/magellan-workflow.sh check-wire "some_function"
  ./scripts/magellan-workflow.sh hotspots 50
  ./scripts/magellan-workflow.sh chunks 100 "src/graph" "fn"
  ./scripts/magellan-workflow.sh chunk-by-symbol "CodeGraph" --file "src/graph"
  ./scripts/magellan-workflow.sh file-metrics "src/main.rs"

Database: $DB_FILE
EOF
        exit 1
        ;;
esac
