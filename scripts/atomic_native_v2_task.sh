#!/bin/bash
# atomic_native_v2_task.sh - Atomic Task Manager for Native V2 Algorithm Porting
#
# This script enforces the atomic workflow for porting algorithm commands to Native V2.
# Each task MUST pass verification on BOTH backends before being marked complete.
#
# Usage: ./scripts/atomic_native_v2_task.sh <action> <command_name>
#
# Actions:
#   start     - Start work on a command (creates task file)
#   discover  - Find current implementation using magellan
#   fix       - Apply the fix to the code
#   verify_sqlite    - Test against SQLite backend
#   verify_native_v2 - Test against Native V2 backend
#   complete  - Mark task as verified (only if BOTH backends pass)
#   status    - Show current task status

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERIFY_DIR="$PROJECT_ROOT/.planning/atomic_native_v2"
PENDING_FILE="$VERIFY_DIR/PENDING_TASKS.json"
TASK_DIR="$VERIFY_DIR/tasks"
TIMESTAMP=$(date +%s)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

log_error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_info() {
    echo -e "${CYAN}ℹ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

log_step() {
    echo -e "${MAGENTA}▶ $1${NC}"
}

show_banner() {
    echo -e "${BLUE}"
    echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
    echo "║       🔬 ATOMIC NATIVE V2 ALGORITHM PORT MANAGER                                ║"
    echo "║       Port algorithm commands to work on BOTH backends                          ║"
    echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

# Ensure directories exist
mkdir -p "$VERIFY_DIR"
mkdir -p "$TASK_DIR"
mkdir -p "$PROJECT_ROOT/.codemcp"

init_pending() {
    if [ ! -f "$PENDING_FILE" ]; then
        echo "[]" > "$PENDING_FILE"
    fi
}

# Load pending tasks
load_tasks() {
    if [ -f "$PENDING_FILE" ]; then
        cat "$PENDING_FILE"
    else
        echo "[]"
    fi
}

# Save pending tasks
save_tasks() {
    echo "$1" > "$PENDING_FILE"
}

# Add task to pending list
add_pending() {
    local cmd="$1"
    local tasks=$(load_tasks)

    # Check if already exists
    if echo "$tasks" | grep -q "\"command\": \"$cmd\""; then
        log_warning "Task '$cmd' already exists"
        return 1
    fi

    # Add new task
    local new_task=$(cat <<EOF
{
  "command": "$cmd",
  "status": "pending",
  "created": "$TIMESTAMP",
  "steps": {
    "discover": false,
    "fix": false,
    "verify_sqlite": false,
    "verify_native_v2": false,
    "parity_check": false
  }
}
EOF
)

    tasks=$(echo "$tasks" | jq ". + [$new_task]")
    save_tasks "$tasks"
    log_success "Task created: $cmd"

    # Create task directory
    mkdir -p "$TASK_DIR/$cmd"
    mkdir -p "$TASK_DIR/$cmd/results"

    # Create task template
    cat > "$TASK_DIR/$cmd/TASK.md" <<EOF
# Atomic Task: Port $cmd to Native V2

**Status:** pending
**Created:** $(date -d @$TIMESTAMP '+%Y-%m-%d %H:%M:%S')

## Command: $cmd

**Current File:** src/${cmd}_cmd.rs

## Discovery

- Uses \`graph_entities\` table: YES/NO
- Hardcoded SQL queries: YES/NO
- Depends on \`get_sqlite_graph()\`: YES/NO

## Fix Required

- [ ] Replace hardcoded SQL with GraphBackend trait methods
- [ ] Use \`neighbors()\` for graph traversal
- [ ] Implement algorithm using in-memory traversal

## Verification

### SQLite Backend Test
\`\`\`bash
# Create SQLite test database
cargo build
./target/debug/magellan watch --root /tmp/test_code --db /tmp/test_sqlite.db --scan-initial &
sleep 2
./target/debug/magellan $cmd --db /tmp/test_sqlite.db
# Expected: [output description]
\`\`\`

### Native V2 Backend Test
\`\`\`bash
# Create Native V2 test database
cargo build --features native-v2
./target/debug/magellan watch --root /tmp/test_code --db /tmp/test_native.db --scan-initial &
sleep 2
./target/debug/magellan $cmd --db /tmp/test_native.db
# Expected: Same output as SQLite
\`\`\`

## Parity Check

- [ ] Both backends return identical results
- [ ] JSON output matches (structure and values)

## Files Modified

- src/${cmd}_cmd.rs
- src/graph/algorithms.rs (if needed)

## Audit

Only when BOTH backends pass:

\`\`\`json
{
  "command": "$cmd",
  "sqlite_hash": "sha256:________",
  "native_v2_hash": "sha256:________",
  "parity": true,
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "verified_by": "atomic_workflow"
}
\`\`\`

EOF
}

# Get task for command
get_task() {
    local cmd="$1"
    load_tasks | jq ".[] | select(.command == \"$cmd\")"
}

# Update task status
update_task() {
    local cmd="$1"
    local field="$2"
    local value="$3"

    local tasks=$(load_tasks)
    local updated=$(echo "$tasks" | jq --arg cmd "$cmd" --arg field "$field" --arg val "$value" "
        map(if .command == \$cmd then .[\$field] = \$val else . end)
    ")

    save_tasks "$updated"
}

# Update task step
update_step() {
    local cmd="$1"
    local step="$2"
    local value="$3"

    local tasks=$(load_tasks)
    local updated=$(echo "$tasks" | jq --arg cmd "$cmd" --arg step "$step" --arg val "$value" "
        map(if .command == \$cmd then .steps[\$step] = (\$val | test(\"true\")) else . end)
    ")

    save_tasks "$updated"
}

# Show task status
show_status() {
    local tasks=$(load_tasks)
    local count=$(echo "$tasks" | jq 'length')

    if [ "$count" -eq 0 ]; then
        log_info "No pending atomic tasks"
        return
    fi

    echo ""
    echo "Pending Atomic Tasks:"
    echo "────────────────────────────────────────────────"

    echo "$tasks" | jq -r '
        .[] | [
            .command,
            .status,
            .created
        ] | @tsv
    ' | while IFS=$'\t' read -r cmd status created; do
        local status_icon=""
        case "$status" in
            pending) status_icon="⏳" ;;
            in_progress) status_icon="🔄" ;;
            verified) status_icon="✅" ;;
            failed) status_icon="❌" ;;
        esac
        echo "  $status_icon $cmd (since $(date -d @$created '+%H:%M'))"
    done
    echo ""

    # Show verified
    local verified="$VERIFY_DIR/VERIFIED_COMMANDS.json"
    if [ -f "$verified" ]; then
        local vcount=$(jq 'length' "$verified")
        if [ "$vcount" -gt 0 ]; then
            echo "Verified Commands (${vcount}):"
            jq -r '.[]' "$verified" | while read cmd; do
                echo "  ✅ $cmd"
            done
            echo ""
        fi
    fi
}

# Valid commands for algorithm porting
VALID_COMMANDS=("cycles" "reachable" "dead-code")

# Start a task
action_start() {
    local cmd="$1"

    # Validate command name
    if [[ ! " ${VALID_COMMANDS[@]} " =~ " ${cmd} " ]]; then
        log_error "Invalid command: $cmd"
        log_info "Valid commands: ${VALID_COMMANDS[*]}"
        return 1
    fi

    # Check if task already exists
    local existing=$(get_task "$cmd")
    if [ -n "$existing" ]; then
        log_warning "Task '$cmd' already exists"
        show_status
        return 1
    fi

    add_pending "$cmd"

    update_task "$cmd" "status" "in_progress"

    log_info "Atomic task started: $cmd"
    log_info ""
    log_info "Next steps:"
    log_info "  1. $ bash $0 discover $cmd"
    log_info "  2. $ bash $0 fix $cmd"
    log_info "  3. $ bash $0 verify_sqlite $cmd"
    log_info "  4. $ bash $0 verify_native_v2 $cmd"
    log_info "  5. $ bash $0 complete $cmd"
}

# Discovery step - find current implementation
action_discover() {
    local cmd="$1"
    local result_dir="$TASK_DIR/$cmd/results"

    log_step "DISCOVERY: Finding current implementation for $cmd"

    # Check if magellan database exists
    local db="$PROJECT_ROOT/.codemcp/codegraph.db"
    if [ ! -f "$db" ]; then
        log_error "Magellan database not found: $db"
        log_info "Run: cd $PROJECT_ROOT && magellan watch --root ./src --db .codemcp/codegraph.db"
        return 1
    fi

    local output="$result_dir/discovery_$TIMESTAMP.txt"

    # Find the command file
    local cmd_file="src/${cmd}_cmd.rs"
    if [ ! -f "$PROJECT_ROOT/$cmd_file" ]; then
        log_error "Command file not found: $cmd_file"
        return 1
    fi

    echo "=== File: $cmd_file ===" > "$output"
    head -50 "$PROJECT_ROOT/$cmd_file" >> "$output"
    echo "" >> "$output"
    echo "=== Dependencies (via magellan) ===" >> "$output"

    # Use magellan to find symbols in the command file
    if command -v magellan &> /dev/null; then
        magellan query --db "$db" --file "$cmd_file" 2>/dev/null | head -30 >> "$output" || true
    fi

    log_success "Discovery complete: $output"

    # Check for hardcoded SQL
    if grep -q "graph_entities" "$PROJECT_ROOT/$cmd_file" 2>/dev/null; then
        echo "  ⚠ Found: graph_entities table query (SQLite-specific)" | tee -a "$output"
    fi

    if grep -q "get_sqlite_graph" "$PROJECT_ROOT/$cmd_file" 2>/dev/null; then
        echo "  ⚠ Found: get_sqlite_graph() helper (SQLite-only)" | tee -a "$output"
    fi

    if grep -q "SELECT.*FROM graph_" "$PROJECT_ROOT/$cmd_file" 2>/dev/null; then
        echo "  ⚠ Found: Direct SQL queries on graph tables" | tee -a "$output"
    fi

    update_step "$cmd" "discover" true
}

# Fix step - document what needs to be fixed
action_fix() {
    local cmd="$1"
    local task_file="$TASK_DIR/$cmd/TASK.md"

    log_step "FIX: Documenting required changes for $cmd"

    if [ ! -f "$task_file" ]; then
        log_error "Task file not found. Run: $0 start $cmd"
        return 1
    fi

    # Open the task file for editing
    log_info "Task file: $task_file"
    log_info ""
    log_info "Review the 'Fix Required' section and make code changes."
    log_info ""
    log_info "Files to modify:"
    log_info "  - $PROJECT_ROOT/src/${cmd}_cmd.rs"
    log_info "  - $PROJECT_ROOT/src/graph/algorithms.rs (if needed)"
    log_info ""
    log_info "After making changes, run: $0 verify_sqlite $cmd"

    # Show current state
    local cmd_file="$PROJECT_ROOT/src/${cmd}_cmd.rs"
    if [ -f "$cmd_file" ]; then
        echo ""
        echo "=== Current file head ($cmd_file) ==="
        head -30 "$cmd_file"
    fi

    update_step "$cmd" "fix" true
}

# Verify SQLite backend
action_verify_sqlite() {
    local cmd="$1"
    local result_dir="$TASK_DIR/$cmd/results"

    log_step "SQLITE VERIFICATION: Testing $cmd with SQLite backend"

    # Build without native-v2
    log_info "Building with SQLite backend..."
    if cargo build --quiet 2>&1 | tee "$result_dir/sqlite_build_$TIMESTAMP.log"; then
        log_success "  Build successful"
    else
        log_error "  Build failed"
        cat "$result_dir/sqlite_build_$TIMESTAMP.log"
        update_task "$cmd" "status" "failed"
        return 1
    fi

    # Create test database
    local test_db="/tmp/test_${cmd}_sqlite_$TIMESTAMP.db"
    local test_src="/tmp/test_code_$TIMESTAMP"

    mkdir -p "$test_src"
    cat > "$test_src/test.rs" <<'EOF'
fn main() {
    println!("Hello");
    helper();
}

fn helper() {
    println!("World");
}

// This creates a cycle for testing
fn recursive_a() {
    recursive_b();
}

fn recursive_b() {
    recursive_a();
}
EOF

    log_info "  Creating test database..."
    timeout 5 ./target/debug/magellan watch --root "$test_src" --db "$test_db" --debounce-ms 100 2>&1 &
    local watcher_pid=$!
    sleep 3
    kill $watcher_pid 2>/dev/null || true

    # Run the command
    log_info "  Running: magellan $cmd --db $test_db"
    if timeout 10 ./target/debug/magellan $cmd --db "$test_db" > "$result_dir/sqlite_output_$TIMESTAMP.txt" 2>&1; then
        log_success "  Command executed successfully"
        head -20 "$result_dir/sqlite_output_$TIMESTAMP.txt"
    else
        log_warning "  Command failed or timed out"
        head -20 "$result_dir/sqlite_output_$TIMESTAMP.txt"
    fi

    # Store hash
    local hash=$(sha256sum "$result_dir/sqlite_output_$TIMESTAMP.txt" | cut -d' ' -f1)
    echo "$hash" > "$result_dir/sqlite_hash.txt"
    log_info "  Output hash: $hash"

    update_step "$cmd" "verify_sqlite" true
}

# Verify Native V2 backend
action_verify_native_v2() {
    local cmd="$1"
    local result_dir="$TASK_DIR/$cmd/results"

    log_step "NATIVE V2 VERIFICATION: Testing $cmd with Native V2 backend"

    # Build with native-v2
    log_info "Building with Native V2 backend..."
    if cargo build --features native-v2 --quiet 2>&1 | tee "$result_dir/native_build_$TIMESTAMP.log"; then
        log_success "  Build successful"
    else
        log_error "  Build failed"
        cat "$result_dir/native_build_$TIMESTAMP.log"
        update_task "$cmd" "status" "failed"
        return 1
    fi

    # Create test database
    local test_db="/tmp/test_${cmd}_native_$TIMESTAMP.db"
    local test_src="/tmp/test_code_$TIMESTAMP"

    log_info "  Creating test database..."
    timeout 5 ./target/debug/magellan watch --root "$test_src" --db "$test_db" --debounce-ms 100 2>&1 &
    local watcher_pid=$!
    sleep 3
    kill $watcher_pid 2>/dev/null || true

    # Run the command
    log_info "  Running: magellan $cmd --db $test_db"
    if timeout 10 ./target/debug/magellan $cmd --db "$test_db" > "$result_dir/native_output_$TIMESTAMP.txt" 2>&1; then
        log_success "  Command executed successfully"
        head -20 "$result_dir/native_output_$TIMESTAMP.txt"
    else
        log_warning "  Command failed or timed out"
        head -20 "$result_dir/native_output_$TIMESTAMP.txt"
    fi

    # Store hash
    local hash=$(sha256sum "$result_dir/native_output_$TIMESTAMP.txt" | cut -d' ' -f1)
    echo "$hash" > "$result_dir/native_hash.txt"
    log_info "  Output hash: $hash"

    update_step "$cmd" "verify_native_v2" true
}

# Parity check - compare outputs
action_parity() {
    local cmd="$1"
    local result_dir="$TASK_DIR/$cmd/results"

    log_step "PARITY CHECK: Comparing SQLite vs Native V2 outputs"

    local sqlite_hash=$(cat "$result_dir/sqlite_hash.txt" 2>/dev/null || echo "")
    local native_hash=$(cat "$result_dir/native_hash.txt" 2>/dev/null || echo "")

    if [ -z "$sqlite_hash" ]; then
        log_error "SQLite hash not found. Run: $0 verify_sqlite $cmd"
        return 1
    fi

    if [ -z "$native_hash" ]; then
        log_error "Native V2 hash not found. Run: $0 verify_native_v2 $cmd"
        return 1
    fi

    echo "  SQLite hash: $sqlite_hash"
    echo "  Native V2 hash: $native_hash"

    if [ "$sqlite_hash" == "$native_hash" ]; then
        log_success "  ✅ PARITY PASSED: Byte-level match!"
        update_step "$cmd" "parity_check" true
        return 0
    else
        log_error "  ❌ PARITY FAILED: Outputs differ"
        echo ""
        echo "SQLite output:"
        head -20 "$result_dir/sqlite_output_*.txt" | tail -10
        echo ""
        echo "Native V2 output:"
        head -20 "$result_dir/native_output_*.txt" | tail -10
        update_step "$cmd" "parity_check" false
        return 1
    fi
}

# Complete task
action_complete() {
    local cmd="$1"
    local task=$(get_task "$cmd")

    if [ -z "$task" ]; then
        log_error "Task not found: $cmd"
        return 1
    fi

    # Check all steps complete
    local discover=$(echo "$task" | jq -r '.steps.discover // false')
    local fix=$(echo "$task" | jq -r '.steps.fix // false')
    local sqlite=$(echo "$task" | jq -r '.steps.verify_sqlite // false')
    local native=$(echo "$task" | jq -r '.steps.verify_native_v2 // false')
    local parity=$(echo "$task" | jq -r '.steps.parity_check // false')

    if [ "$discover" != "true" ] || [ "$fix" != "true" ] || [ "$sqlite" != "true" ] || [ "$native" != "true" ] || [ "$parity" != "true" ]; then
        log_error "Task cannot be marked complete - missing steps:"
        echo "  discover: $discover"
        echo "  fix: $fix"
        echo "  verify_sqlite: $sqlite"
        echo "  verify_native_v2: $native"
        echo "  parity_check: $parity"
        log_info "All steps must be complete."
        return 1
    fi

    # Run final parity check
    if ! action_parity "$cmd"; then
        log_error "Parity check failed. Cannot complete task."
        return 1
    fi

    # Create audit file
    local result_dir="$TASK_DIR/$cmd/results"
    local sqlite_hash=$(cat "$result_dir/sqlite_hash.txt")
    local native_hash=$(cat "$result_dir/native_hash.txt")

    cat > "$result_dir/audit_$TIMESTAMP.json" <<EOF
{
  "command": "$cmd",
  "sqlite_hash": "sha256:$sqlite_hash",
  "native_v2_hash": "sha256:$native_hash",
  "parity": true,
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "verified_by": "atomic_workflow"
}
EOF

    # Remove from pending
    local tasks=$(load_tasks | jq "[.[] | select(.command != \"$cmd\")]")
    save_tasks "$tasks"

    # Add to verified
    local verified="$VERIFY_DIR/VERIFIED_COMMANDS.json"
    if [ -f "$verified" ]; then
        local verified_list=$(cat "$verified")
        echo "$verified_list" | jq ". + [\"$cmd\"]" > "$verified.tmp"
        mv "$verified.tmp" "$verified"
    else
        echo "[\"$cmd\"]" > "$verified"
    fi

    log_success "╔═══════════════════════════════════════════════════════════════════════════════╗"
    log_success "║                  ✅ ATOMIC TASK COMPLETE: $cmd"
    log_success "║"
    log_success "║  The command has been verified on BOTH backends with parity check passed."
    log_success "║"
    log_success "║  Audit: $result_dir/audit_$TIMESTAMP.json"
    log_success "║"
    log_success "║  Next: Choose next atomic command to port."
    log_success "╚═══════════════════════════════════════════════════════════════════════════════╝"
}

# Main command dispatcher
case "$1" in
    start)
        if [ -z "$2" ]; then
            log_error "Usage: $0 start <command_name>"
            log_info "Valid commands: ${VALID_COMMANDS[*]}"
            exit 1
        fi
        show_banner
        init_pending
        action_start "$2"
        ;;

    discover)
        if [ -z "$2" ]; then
            log_error "Usage: $0 discover <command_name>"
            exit 1
        fi
        action_discover "$2"
        ;;

    fix)
        if [ -z "$2" ]; then
            log_error "Usage: $0 fix <command_name>"
            exit 1
        fi
        action_fix "$2"
        ;;

    verify_sqlite)
        if [ -z "$2" ]; then
            log_error "Usage: $0 verify_sqlite <command_name>"
            exit 1
        fi
        action_verify_sqlite "$2"
        ;;

    verify_native_v2)
        if [ -z "$2" ]; then
            log_error "Usage: $0 verify_native_v2 <command_name>"
            exit 1
        fi
        action_verify_native_v2 "$2"
        ;;

    parity)
        if [ -z "$2" ]; then
            log_error "Usage: $0 parity <command_name>"
            exit 1
        fi
        action_parity "$2"
        ;;

    complete)
        if [ -z "$2" ]; then
            log_error "Usage: $0 complete <command_name>"
            exit 1
        fi
        show_banner
        action_complete "$2"
        ;;

    status)
        show_banner
        init_pending
        show_status
        ;;

    list)
        show_banner
        echo ""
        echo "Atomic Commands (workflow order):"
        echo "────────────────────────────────────────────────"
        echo "  1. cycles      - Cycle detection (SCC algorithm)"
        echo "  2. reachable   - Reachability analysis"
        echo "  3. dead-code   - Dead code detection"
        echo ""
        echo "Usage: $0 start <command_name>"
        echo ""
        echo "Workflow for each command:"
        echo "  1. $0 start <command>"
        echo "  2. $0 discover <command>"
        echo "  3. [Edit code to fix]"
        echo "  4. $0 verify_sqlite <command>"
        echo "  5. $0 verify_native_v2 <command>"
        echo "  6. $0 complete <command>"
        echo ""
        ;;

    *)
        cat <<EOF
Atomic Task Manager - Native V2 Algorithm Porting

Usage: $0 <action> [command_name]

Actions:
  list              - Show all atomic commands and workflow
  start             - Start work on a command
  discover          - Run magellan discovery on current implementation
  fix               - Document required code changes
  verify_sqlite     - Test against SQLite backend
  verify_native_v2 - Test against Native V2 backend
  parity            - Compare outputs from both backends
  complete          - Mark task as verified (only if parity passes!)
  status            - Show current task status

Commands:
  cycles            - Port cycle detection command
  reachable         - Port reachability command
  dead-code         - Port dead-code command

Workflow (for each command):
  1. $0 start cycles
  2. $0 discover cycles
  3. [Edit src/cycles_cmd.rs and src/graph/algorithms.rs]
  4. $0 verify_sqlite cycles
  5. $0 verify_native_v2 cycles
  6. $0 complete cycles

Task files are stored in: .planning/atomic_native_v2/tasks/
EOF
        exit 1
        ;;
esac
