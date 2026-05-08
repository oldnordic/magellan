#!/usr/bin/env fish
# read-gate.fish — Token-aware read gate for source files
#
# PURPOSE: Intercept large source file reads and nudge agents toward
# graph queries (magellan/llmgrep/mirage) instead of full file reads.
# Advisory only — never blocks on day 1. Logs all decisions.
#
# ENVIRONMENT:
#   READ_GATE_OVERRIDE=1  — bypass all checks (user-directed read)
#   READ_GATE_SKIP=1      — skip entirely for this call
#
# INTEGRATION: Runs as PreToolUse hook on Read. Reads stdin for tool call JSON.
# Claude Code: { "tool_name": "Read", "tool_input": { "file_path": "..." } }
# Hermes: reads files via read_file tool — this hook adapts to both.

set -l PROJECT_DIR "$CLAUDE_PROJECT_DIR"
if test -z "$PROJECT_DIR"
    # Resolve main repo root so hooks work from worktrees and subdirectories
    set PROJECT_DIR (dirname (git rev-parse --git-common-dir))
end

cd "$PROJECT_DIR" 2>/dev/null || exit 0

# === Configuration ===
set -l SOURCE_EXTENSIONS .rs .py .ts .tsx .js .jsx .go .java .c .cpp .h .hpp .cs .swift .kt
set -l EXCLUDE_PATTERNS target/ node_modules/ dist/ build/ .git/ vendor/ __pycache__/ .venv/
set -l EXCLUDE_FILES Cargo.lock package-lock.json yarn.lock go.sum
set -l LINE_THRESHOLD 200
set -l BYTE_THRESHOLD 8192
set -l LOG_DIR ".claude"
set -l SESSION_LOG "$LOG_DIR/read-gate-session.jsonl"
set -l SUMMARY_LOG "$LOG_DIR/token-savings-summary.json"
set -l GRACE_PERIOD_MINUTES 5

# === Override checks ===
if test "$READ_GATE_OVERRIDE" = "1"
    echo '{"decision":"allow","reason":"READ_GATE_OVERRIDE=1"}'
    exit 0
end
if test "$READ_GATE_SKIP" = "1"
    exit 0
end

# === Read stdin for tool call info ===
set -l stdin_data ""
set -l FILE_PATH ""
if isatty stdin
    # No stdin — called manually or from shell, not hook framework
    # Check arguments instead
    set FILE_PATH $argv[1]
else
    set stdin_data (cat)
    # Try to extract file_path from Claude Code JSON format
    set FILE_PATH (echo "$stdin_data" | jq -r '.tool_input.file_path // .file_path // empty' 2>/dev/null)
    if test -z "$FILE_PATH"
        # Hermes format or other — try first argument
        set FILE_PATH $argv[1]
    end
end

if test -z "$FILE_PATH"
    exit 0  # Can't determine file — allow
end

# === Check extension ===
set -l IS_SOURCE false
for ext in $SOURCE_EXTENSIONS
    if string match -q "*$ext" "$FILE_PATH"
        set IS_SOURCE true
        break
    end
end

if test "$IS_SOURCE" = false
    exit 0  # Not a source file — allow silently
end

# === Check exclude patterns ===
for pat in $EXCLUDE_PATTERNS
    if string match -q "*$pat*" "$FILE_PATH"
        exit 0
    end
end
for excl in $EXCLUDE_FILES
    if string match -q "*$excl" (basename "$FILE_PATH")
        exit 0
    end
end

# === Check file exists and size ===
if not test -f "$FILE_PATH"
    exit 0  # File doesn't exist — not our problem
end

set -l FILE_LINES (wc -l < "$FILE_PATH" 2>/dev/null | string trim)
set -l FILE_BYTES (stat -c %s "$FILE_PATH" 2>/dev/null || stat -f %z "$FILE_PATH" 2>/dev/null)

if test -z "$FILE_LINES"
    set FILE_LINES 0
end
if test -z "$FILE_BYTES"
    set FILE_BYTES 0
end

# Below threshold — allow silently
if test "$FILE_LINES" -lt $LINE_THRESHOLD; and test "$FILE_BYTES" -lt $BYTE_THRESHOLD
    exit 0
end

# === Find magellan database ===
set -l DB_PATH ""
for candidate in ".magellan/magellan.db" ".magellan/llmgrep.db" ".magellan/mirage.db"
    if test -f "$PROJECT_DIR/$candidate"
        set DB_PATH "$PROJECT_DIR/$candidate"
        break
    end
end

# === Discover project DB by convention ===
if test -z "$DB_PATH"
    # Extract project name from directory
    set -l project_name (basename "$PROJECT_DIR")
    set -l convention_db ".magellan/$project_name.db"
    if test -f "$PROJECT_DIR/$convention_db"
        set DB_PATH "$PROJECT_DIR/$convention_db"
    end
end

# === Calculate tokens ===
set -l EST_TOKENS (math -s0 "ceil($FILE_BYTES / 4)")

# === Build nudge message ===
set -l TIMESTAMP (date -Iseconds)
set -l NUDGE ""
set -l DECISION "allow"
set -l REASON "advisory_nudge"

set DB_REL ""
set FILE_REL (string replace "$PROJECT_DIR/" "" "$FILE_PATH")
set SYMBOL_COUNT 0
set LLMGREP_FALLBACK false
set SYMBOL_SOURCE "no_database"

if test -n "$DB_PATH"
    # File is indexed — strong nudge with specific commands
    set DB_REL (string replace "$PROJECT_DIR/" "" "$DB_PATH")
    set SYMBOLS_JSON (magellan query --db "$DB_PATH" --file "$FILE_REL" --output json 2>/dev/null)
    if test -n "$SYMBOLS_JSON"
        set SYMBOL_COUNT (echo "$SYMBOLS_JSON" | jq 'if .data.symbols then (.data.symbols | length) elif type == "array" then length else 0 end' 2>/dev/null)
        if test -z "$SYMBOL_COUNT"
            set SYMBOL_COUNT 0
        end
    end

    # Fallback: if magellan query returned 0 symbols, try llmgrep prefix search
    if test "$SYMBOL_COUNT" -eq 0
        set FILE_STEM (basename "$FILE_REL" | sed 's/\.[^.]*$//')
        set LLMGREP_JSON (llmgrep --db "$DB_PATH" search --query "$FILE_STEM" --output json 2>/dev/null)
        if test -n "$LLMGREP_JSON"
            set LLMGREP_COUNT (echo "$LLMGREP_JSON" | jq 'if .data then (.data | length) elif type == "array" then length else 0 end' 2>/dev/null)
            if test -n "$LLMGREP_COUNT"; and test "$LLMGREP_COUNT" -gt 0
                set SYMBOL_COUNT "$LLMGREP_COUNT"
                set LLMGREP_FALLBACK true
            end
        end
    end

    set SYMBOL_SOURCE "indexed"
    if test "$LLMGREP_FALLBACK" = true
        set SYMBOL_SOURCE "found via llmgrep prefix search"
    else if test "$SYMBOL_COUNT" -eq 0
        set SYMBOL_SOURCE "no symbols found (file may not be indexed)"
    end

    set NUDGE (string join "\n" \
        "" \
        "READ GATE: This file is in the graph ($SYMBOL_COUNT symbols, $SYMBOL_SOURCE)." \
        "File: $FILE_REL (~$FILE_LINES lines, ~$EST_TOKENS tokens)" \
        "" \
        "Before reading the full file, consider:" \
        "  magellan query --db $DB_REL --file $FILE_REL          # list all symbols in file" \
        "  llmgrep --db $DB_REL search --query \"<term>\"          # semantic search" \
        "  magellan context symbol --db $DB_REL --name <sym>      # get symbol context" \
        "  mirage --db $DB_REL hotspots                          # find hot paths" \
        "" \
        "Set READ_GATE_OVERRIDE=1 to read without nudging." \
        "")

    # Check for repeated reads in this session
    if test -f "$SESSION_LOG"
        set -l READ_COUNT (grep -c "\"file\":\"$FILE_REL\"" "$SESSION_LOG" 2>/dev/null; or echo 0)
        # Ensure READ_COUNT is a single integer (guard against multi-word values)
        set -l READ_COUNT (string match -r '^[0-9]+$' -- $READ_COUNT[1]; or echo 0)
        if test "$READ_COUNT" -ge 2
            set NUDGE (string join "\n" \
                "" \
                "READ GATE: You have already read $FILE_REL $READ_COUNT times this session." \
                "Total wasted tokens: ~"(math "$READ_COUNT * $EST_TOKENS") \
                "Use graph queries instead. Set READ_GATE_OVERRIDE=1 to force." \
                "")
            set DECISION "warn_repeated"
            set REASON "repeated_read_$READ_COUNT"
        end
    end
else
    # No database found — gentle nudge with index suggestion
    set NUDGE (string join "\n" \
        "" \
        "READ GATE: No graph database found for this project." \
        "File: $FILE_REL (~$FILE_LINES lines, ~$EST_TOKENS tokens)" \
        "" \
        "To enable graph queries:" \
        "  magellan watch --root ./src --db .magellan/(basename (pwd)).db --scan-initial" \
        "" \
        "Set READ_GATE_OVERRIDE=1 to read without nudging." \
        "")
    set REASON "no_database"
end

# === Log decision ===
mkdir -p "$LOG_DIR" 2>/dev/null

set -l LOG_ENTRY (jq -n \
    --arg ts "$TIMESTAMP" \
    --arg file "$FILE_PATH" \
    --arg lines "$FILE_LINES" \
    --arg bytes "$FILE_BYTES" \
    --arg tokens "$EST_TOKENS" \
    --arg decision "$DECISION" \
    --arg reason "$REASON" \
    '{timestamp:$ts, file:$file, lines:($lines|tonumber), bytes:($bytes|tonumber), tokens:($tokens|tonumber), decision:$decision, reason:$reason}')

# Deduplicate: skip if last entry for this file has same timestamp
set -l SKIP_LOG false
if test -f "$SESSION_LOG"
    set -l LAST_ENTRY (tail -1 "$SESSION_LOG" 2>/dev/null)
    set -l LAST_TS (echo "$LAST_ENTRY" | jq -r '.timestamp' 2>/dev/null)
    set -l LAST_FILE (echo "$LAST_ENTRY" | jq -r '.file' 2>/dev/null)
    if test "$LAST_TS" = "$TIMESTAMP"; and test "$LAST_FILE" = "$FILE_PATH"
        set SKIP_LOG true
    end
end

if test "$SKIP_LOG" = false
    echo "$LOG_ENTRY" >> "$SESSION_LOG"
end

# === Emit nudge ===
echo "$NUDGE"

# === Update summary ===
if test -f "$SUMMARY_LOG"
    set -l CURRENT (cat "$SUMMARY_LOG" 2>/dev/null)
    set -l TOTAL_NUDGED (echo "$CURRENT" | jq '.total_nudged // 0' 2>/dev/null)
    set -l TOTAL_TOKENS (echo "$CURRENT" | jq '.total_tokens_avoided // 0' 2>/dev/null)
    if test -z "$TOTAL_NUDGED"; set TOTAL_NUDGED 0; end
    if test -z "$TOTAL_TOKENS"; set TOTAL_TOKENS 0; end
    set -l NEW_NUDGED (math "$TOTAL_NUDGED + 1")
    set -l NEW_TOKENS (math "$TOTAL_TOKENS + $EST_TOKENS")
    jq -n \
        --arg updated "$TIMESTAMP" \
        --argjson nudged "$NEW_NUDGED" \
        --argjson tokens "$NEW_TOKENS" \
        '{last_updated:$updated, total_nudged:$nudged, total_tokens_avoided:$tokens}' > "$SUMMARY_LOG"
else
    jq -n \
        --arg ts "$TIMESTAMP" \
        --argjson nudged 1 \
        --argjson tokens "$EST_TOKENS" \
        '{last_updated:$ts, total_nudged:$nudged, total_tokens_avoided:$tokens}' > "$SUMMARY_LOG"
end

exit 0  # Advisory — always allow
