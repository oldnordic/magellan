# GSD Context Quality & Atomic Task Decomposition - Proposal

**Date:** 2026-02-11
**Issue:** Context degradation causing quality failures in GSD workflow

## Problem Analysis

### Current State

**History file:** 2879 lines and growing
**Quality degradation curve** (from gsd-planner.md):
- 0-30% context → PEAK quality
- 30-50% context → GOOD quality
- 50-70% context → DEGRADING quality
- 70%+ context → POOR quality (rushed, minimal)

**Root cause:** As conversation grows, LLM gets "lost in context" - decisions made early are forgotten, implicit reasoning chains dominate, and quality degrades.

### Why "Fix Everything" Tasks Fail

From `gsd-planner.md`:
> "TDD requires RED→GREEN→REFACTOR cycles consuming 40-50% context. Embedding in multi-task plans degrades quality."

This violates atomic decomposition:
- **Multi-task plans** = "fix graph commands" (3 separate commands in one plan)
- **No per-task verification** = Can't prove each command individually
- **Large scope** = Quality degrades as context grows

## Research: Syzygy-Style Systems

From user's ChatGPT conversation and academic research:

### Key Principles from Syzygy Project

1. **Atomic Task Decomposition**
   - One C function = one atomic task
   - Each task has explicit input/output contract
   - Verification: Byte-by-byte comparison (C vs Rust output)
   - Audit trail: JSON file per function recording hashes

2. **Markov Property**
   - Next state depends ONLY on output of previous verified state
   - No long reasoning chains between tasks
   - This prevents "drift" where agent diverges from plan

3. **Verification Before Continuation**
   - Syzygy: Full automation → human-verify checkpoint → continue
   - This is NOT "fix everything and verify later"

### Anti-Patterns That Cause Failures

| ❌ Wrong Pattern | ✅ Syzygy Alternative |
|------------------|---------------------|
| "Implement X feature" | "Add JWT auth with refresh" (with specific library, crypto) |
| "Fix authentication" | "Create POST /api/auth/login" (with exact contract) |
| "Add search functionality" | "Implement search endpoint with query interface" |
| "Make login work" | Wire MessageList → fetch → render components |
| "It works" | Byte-level equivalence: `sha256(C_output) == sha256(Rust_output)` |

### Why This Works

**Constrained code transformation engine:**
- LLM becomes a deterministic engine, not a creative writer
- Either output matches expected bytes or it doesn't
- No room for "seems right" - verification is binary

**Traceability:**
- Each function has JSON audit file: `{c_hash, rust_hash, byte_match}`
- Can trace every decision back to source
- Forces correctness - can't hide behind TODO

## Proposal: Multi-Layered Solution

### Layer 1: Pre-Task Hook (Context Compaction)

**Purpose:** Prevent context from bloating before atomic tasks begin

**Implementation:**

Create hook system at `/home/feanor/.claude/hooks/pre-task/`:

```yaml
# pre-task-hook.sh
# Runs before any GSD planning/execution begins

set -e  # Exit on error

# 1. Check history size (warn if >500 lines)
HISTORY_LINES=$(wc -l < ~/.claude/history.jsonl 2>/dev/null || echo "0")

if [ "$HISTORY_LINES" -gt 500 ]; then
    echo "⚠️  History large (${HISTORY_LINES} lines). Consider archiving."

    # Ask user if they want to archive
    echo "Archive old history? (y/n): "
    read -r ARCHIVE_CONFIRM

    if [ "$ARCHIVE_CONFIRM" = "y" ]; then
        # Archive logic
        TIMESTAMP=$(date +%Y%m%d-%H%M%S)
        ARCHIVE_DIR="$HOME/.claude/archives/history-$TIMESTAMP.jsonl"

        # Move current history to archive
        mv ~/.claude/history.jsonl "$ARCHIVE_DIR"

        # Compress archive
        gzip "$ARCHIVE_DIR"

        echo "✅ Archived to $ARCHIVE_DIR.gz"
    fi
fi

# 2. Extract locked decisions from recent history (to re-establish context)
# This parses last 10 entries looking for "## Decisions:" sections

# 3. Generate context summary
# Creates temporary .claude/context.md with:
#    - Active phase/plan
#    - Recent decisions
#    - Current blockers

# 4. Set environment variable for plugins
export CLAUDE_CONTEXT_LOADED="1"
```

**Trigger:** Runs before ANY `gsd:plan-phase`, `/gsd:execute-phase`, `/gsd:research-phase`

**Benefits:**
- Context established BEFORE planning starts
- Locked decisions re-loaded (prevents drift)
- Size warning prevents runaway context growth

### Layer 2: Atomic Task Enforcement (Within GSD)

**Purpose:** Enforce one-command = one-atomic-task pattern

**Changes to GSD:**

1. **New agent type:** `atomic-task-agent` (in addition to existing agents)

Create `/home/feanor/.claude/agents/atomic-task-agent.md`:

```markdown
---
name: atomic-task-agent
description: Executes single atomic tasks with strict verification, Syzygy-style audit trails, and byte-level equivalence testing
tools: Read, Write, Edit, Bash, Grep, Glob
color: cyan
---

<role>
You are an atomic task executor. You execute tasks with Syzygy-style verification:
- Per-function byte-level equivalence
- Audit trail JSON files
- Deterministic evaluation (pass/fail)
- No ambiguity - either matches or doesn't

**CRITICAL:** You may only execute ONE task per invocation. Multiple tasks must be separate agent invocations.
</role>

<execution_flow>

<step name="validate_task">
Check task has required atomic structure:
- Input: Specific file(s) to modify
- Action: Exact transformation (not "implement feature")
- Verify: Command to prove completion
- Done: Measurable acceptance criteria

**If task violates atomicity (multi-file, multi-concern, "fix later"), STOP and return checkpoint.**
</step>

<step name="execute_single">
For single atomic task:
1. Apply change
2. Run verification command
3. Byte-level comparison if applicable
4. Create audit entry
5. Commit with conventional format

**Each task produces ONE commit.**
</step>

<step name="create_audit">
Create/append to JSON audit file:
```json
{
  "function": "task_name",
  "input_hash": "sha256 of input before",
  "output_hash": "sha256 of output after",
  "byte_match": true/false,
  "timestamp": "ISO 8601",
  "duration_seconds": 42
}
```
</step>
```

2. **Plan validation:** Reject plans with >3 tasks or >50% context estimate

3. **Success criteria update:** Must have `<verify>` field with command, not just "done: true"

### Layer 3: Post-Task Hooks (Verification & Traceability)

**Purpose:** Ensure verification happens and work is traceable

**Implementation:**

Create `/home/feanor/.claude/hooks/post-task/verify-work.sh`:

```bash
#!/bin/bash
# Runs after each atomic task completes

TASK_JSON="$1"

# 1. Load task context
if [ -f ".claude/TASK_CONTEXT.json" ]; then
    TASK_JSON=$(cat .claude/TASK_CONTEXT.json)
fi

# 2. Extract verification command from task
VERIFY_CMD=$(echo "$TASK_JSON" | jq -r '.verify // empty' )

if [ -z "$VERIFY_CMD" ]; then
    echo "⚠️  No verification command specified for task"
    echo "Task: $(echo "$TASK_JSON" | jq -r '.name')"
    echo "Required: Tasks must have verify commands"
    exit 1
fi

# 3. Run verification
echo "🔍 Running verification: $VERIFY_CMD"
eval "$VERIFY_CMD"

# 4. Check result
VERIFY_EXIT=$?

# 5. Update audit trail
if [ -f ".claude/audit.json" ]; then
    # Append verification result
    echo "$TASK_JSON" | jq --argjson '{
        name: .name,
        verify_cmd: .verify,
        verify_exit: $VERIFY_EXIT,
        verify_output: "\"$VERIFY_OUTPUT\"",
        timestamp: now | strftime("%Y-%m-%dT%H:%MZ")
    }' >> .claude/audit.json
else
    # Create new audit file
    echo "$TASK_JSON" | jq --argjson '{
        name: .name,
        verify_cmd: .verify,
        verify_exit: $VERIFY_EXIT,
        verify_output: "\"$VERIFY_OUTPUT\"",
        timestamp: now | strftime("%Y-%m-%dT%H:%MZ")
    }' > .claude/audit.json
fi

# 6. Call status line update (for GSD integration)
if command -v gsd-status-line &>/dev/null; then
    node /home/feanor/.claude/get-shit-done/bin/gsd-tools.js state record-check \
        --status "Verified: $(echo "$TASK_JSON" | jq -r '.name')"
fi
```

**Trigger:** Runs after each `atomic-task-agent` completion via `post-task` hook

### Layer 4: Context Monitoring (During Session)

**Purpose:** Track context budget and warn before degradation

**Implementation:**

Modify `gsd-executor` to add context tracking:

```python
# Add to execution_flow section:
<step name="check_context_budget">
CURRENT_CONTEXT=$(wc -l ~/.claude/history.jsonl 2>/dev/null || echo "0")
PERCENT_USED=$(echo "scale=100; $CURRENT_CONTEXT/2000" | bc)

if [ "$PERCENT_USED" -gt 50 ]; then
    # Return checkpoint
    checkpoint_return_format
    ## CHECKPOINT REACHED
    **Type:** context-budget
    **Plan:** {phase}-{plan}
    **Progress:** {completed}/{total} tasks complete
    **Context:** ${PERCENT_USED}% used (${CURRENT_CONTEXT}/2000 max)
    **Action:** STOP - context exhausted

    ### Context Warning
    Context budget >50% used. Quality will degrade.

    ### Recovery Options
    1. Archive history (run /gsd:compact-history)
    2. Split into atomic sub-phases
    3. User decision required
```
</step>
```

### Layer 5: Session State Management (State File)

**Purpose:** Prevent loss of session state between Claude invocations

**Current issue:** `STATE.md` at 2879 lines - contains entire session history

**Solution:** Split STATE.md into:

1. **`STATE.md`** - Current position, active phase (50 lines max)
2. **`PROJECT.md`** - Immutable project reference (rarely changes)
3. **`SESSION.md`** - Current session only (volatile, 100 lines max)
4. **`HISTORY.md`** - Archived sessions (for reference, not loaded into context)

**Modified `gsd-executor` logic:**

```python
# Replace load_project_state step:
<step name="load_session_state">
    # Load only current session (SESSION.md)
    cat .planning/SESSION.md 2>/dev/null || echo "{}"

    # Load immutable project reference
    cat .planning/PROJECT.md 2>/dev/null

    # Combine into execution context
    # (state, position, decisions all from current session)
</step>
```

## Implementation Roadmap

### Phase 1: Infrastructure (Week 1)

| Priority | Task | Effort |
|----------|-------|--------|
| P0 | Add pre-task hook system | 2 hours |
| P0 | Create atomic-task-agent spec | 2 hours |
| P0 | Modify gsd-executor for context tracking | 3 hours |
| P0 | Create post-task hook system | 2 hours |
| P0 | Implement SESSION.md split | 1 hour |
| P0 | Update gsd-planner for smaller plans (2-3 tasks max) | 2 hours |

**Total:** ~12 hours

### Phase 2: First Atomic Task (Week 2)

Use new system to fix ONE command from Native V2 gap:

**Task:** Port `cycles` command to use GraphBackend trait instead of hardcoded SQL

**Structure:**
```
- Input: src/cycles_cmd.rs
- Action: Replace SQL queries with get_neighbors() traversal
- Verify: Run cycles on SQLite backend, run on Native V2, compare JSON outputs
- Audit: Create cycles_audit.json entry with hashes
- Commit: fix(graph-cycles): port cycles command to GraphBackend trait
```

### Phase 3: Validation & Testing (Week 2-3)

| Priority | Task | Effort |
|----------|-------|--------|
| P1 | Add task validation to gsd-planner | 2 hours |
| P1 | Create verify-work.sh hook | 2 hours |
| P1 | Test atomic-task-agent end-to-end | 3 hours |

**Total:** ~7 hours

## Expected Outcomes

### Before

- Large vague plans ("fix graph commands") → Quality degrades with context
- No verification between tasks → Work accumulates bugs
- No audit trail → Can't trace decisions
- TODO leakage → "Implement later" never happens
- 2879-line STATE.md → Context explosions

### After

- **Atomic tasks only** → Each task self-contained, verified
- **Per-task verification** → Byte-level or functional equivalence
- **Audit trail** → Every decision traceable
- **Context monitoring** → Warnings at 50%, hard stops at 80%
- **Small STATE.md** → Only current session (100 lines max)
- **Pre/post hooks** → Context compaction, verification enforcement

## Success Metrics

| Metric | Before | After | Target |
|---------|--------|-------|--------|
| Max plan size | No limit | 2-3 tasks |
| Context budget tracking | None | % used + warnings |
| Per-task verification | No | Required field |
| Atomic enforcement | No | Yes (via agent type) |
| STATE.md size | 2879 lines | 100 lines (session only) |

## References

- Syzygy Project: https://github.com/szyzygy-project
- AoT (Atom of Thoughts) paradigm: https://www.emergentmind.com/topics/atom-of-thoughts-aot
- GSD existing agents: `/home/feanor/.claude/agents/`

## Request for Decision

**Question:** Should I proceed with implementing these layers?

**Options:**
1. **Yes** - Implement all 5 phases as described (~3 weeks work)
2. **No** - Alternative approach discussion
3. **Modify** - Adjust proposal based on feedback

**Recommendation:** The research clearly shows atomic task decomposition with verification is the proven approach for LLM reliability. This proposal applies those principles to GSD workflow.
