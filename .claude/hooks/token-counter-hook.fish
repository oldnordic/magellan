#!/usr/bin/env fish
# token-counter-hook.fish — Per-session token tracking for file reads
#
# Tracks token usage when agents read source files. Logs to JSONL and
# maintains a rolling summary. Runs as PreToolUse hook on Read.
#
# Output: nudge text (same as read-gate) + token log entry
# Exit: always 0 (advisory only)

set -l PROJECT_DIR "$CLAUDE_PROJECT_DIR"
if test -z "$PROJECT_DIR"
    # Resolve main repo root so hooks work from worktrees and subdirectories
    set PROJECT_DIR (dirname (git rev-parse --git-common-dir))
end

cd "$PROJECT_DIR" 2>/dev/null || exit 0

set -l LOG_DIR ".claude"
set -l SESSION_LOG "$LOG_DIR/token-session.jsonl"
set -l SUMMARY_LOG "$LOG_DIR/token-session-summary.json"
set -l TIMESTAMP (date -Iseconds)

# === Read stdin for tool call info ===
set -l stdin_data ""
set -l FILE_PATH ""
if isatty stdin
    set FILE_PATH $argv[1]
else
    set stdin_data (cat)
    set FILE_PATH (echo "$stdin_data" | jq -r '.tool_input.file_path // .file_path // empty' 2>/dev/null)
    if test -z "$FILE_PATH"
        set FILE_PATH $argv[1]
    end
end

if test -z "$FILE_PATH"
    exit 0
end

# Resolve to absolute path
set -l FILE_ABS "$FILE_PATH"
if not string match -q "/*" "$FILE_PATH"
    set FILE_ABS "$PROJECT_DIR/$FILE_PATH"
end

set -l FILE_REL (string replace "$PROJECT_DIR/" "" "$FILE_ABS")

# === Get file size ===
if not test -f "$FILE_ABS"
    exit 0
end

set -l FILE_BYTES (stat -c %s "$FILE_ABS" 2>/dev/null || stat -f %z "$FILE_ABS" 2>/dev/null)
if test -z "$FILE_BYTES"
    set FILE_BYTES 0
end

set -l EST_TOKENS (math -s0 "ceil($FILE_BYTES / 4)")

# === Log entry ===
mkdir -p "$LOG_DIR" 2>/dev/null

set -l LOG_ENTRY (jq -n \
    --arg ts "$TIMESTAMP" \
    --arg tool "Read" \
    --arg file "$FILE_REL" \
    --argjson bytes "$FILE_BYTES" \
    --argjson tokens "$EST_TOKENS" \
    '{timestamp:$ts, tool:$tool, file:$file, bytes:$bytes, est_tokens:$tokens}')

echo "$LOG_ENTRY" >> "$SESSION_LOG"

# === Update rolling summary ===
if test -f "$SUMMARY_LOG"
    set -l CURRENT (cat "$SUMMARY_LOG" 2>/dev/null)
    set -l SESSION_START (echo "$CURRENT" | jq -r '.session_start // empty' 2>/dev/null)
    if test -z "$SESSION_START"
        set SESSION_START "$TIMESTAMP"
    end
    set -l TOTAL_TOKENS (echo "$CURRENT" | jq '.total_tokens // 0' 2>/dev/null)
    set -l TOTAL_READS (echo "$CURRENT" | jq '.total_reads // 0' 2>/dev/null)
    if test -z "$TOTAL_TOKENS"; set TOTAL_TOKENS 0; end
    if test -z "$TOTAL_READS"; set TOTAL_READS 0; end

    set -l NEW_TOKENS (math "$TOTAL_TOKENS + $EST_TOKENS")
    set -l NEW_READS (math "$TOTAL_READS + 1")

    # Update by_tool
    set -l BY_TOOL (echo "$CURRENT" | jq -r '.by_tool // {}' 2>/dev/null)
    set -l READ_TOKENS (echo "$BY_TOOL" | jq '.Read // 0' 2>/dev/null)
    set -l NEW_READ_TOKENS (math "$READ_TOKENS + $EST_TOKENS")
    set -l BY_TOOL (echo "$BY_TOOL" | jq --argjson t "$NEW_READ_TOKENS" '.Read = $t' 2>/dev/null)

    # Update by_file
    set -l BY_FILE (echo "$CURRENT" | jq -r '.by_file // {}' 2>/dev/null)
    set -l FILE_TOKENS (echo "$BY_FILE" | jq --arg f "$FILE_REL" '.[$f] // 0' 2>/dev/null)
    set -l NEW_FILE_TOKENS (math "$FILE_TOKENS + $EST_TOKENS")
    set -l BY_FILE (echo "$BY_FILE" | jq --arg f "$FILE_REL" --argjson t "$NEW_FILE_TOKENS" '.[$f] = $t' 2>/dev/null)

    jq -n \
        --arg ss "$SESSION_START" \
        --argjson tt "$NEW_TOKENS" \
        --argjson tr "$NEW_READS" \
        --argjson bt "$BY_TOOL" \
        --argjson bf "$BY_FILE" \
        --arg updated "$TIMESTAMP" \
        '{session_start:$ss, last_updated:$updated, total_tokens:$tt, total_reads:$tr, by_tool:$bt, by_file:$bf}' > "$SUMMARY_LOG"
else
    jq -n \
        --arg ts "$TIMESTAMP" \
        --argjson tokens "$EST_TOKENS" \
        --arg file "$FILE_REL" \
        '{session_start:$ts, last_updated:$ts, total_tokens:$tokens, total_reads:1, by_tool:{Read:$tokens}, by_file:{($file):$tokens}}' > "$SUMMARY_LOG"
end

exit 0
