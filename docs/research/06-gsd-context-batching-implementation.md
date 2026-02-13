# GSD Context-Aware Task Batching Implementation

**Date:** 2026-02-11
**Purpose:** Implement self-monitoring context batching to prevent unbounded context growth in GSD task execution

## Problem Statement

GSD's task execution had two related context issues:

1. **STATE.md grows unbounded** - Each subagent loads entire STATE.md, appends decisions/sessions, writes back. Over 100+ tasks, STATE.md becomes massive.

2. **Long-running agents bloat context** - A single agent executing 20+ tasks accumulates conversation history, degrading performance and increasing token usage.

## Solution Implemented

### 1. Context-Aware Task Batching

Agents now self-monitor context usage and create checkpoints at ~50% capacity:

- **Detection:** Slower responses, 3-5 tasks completed, multiple decisions/deviations
- **Action:** Stop after current task, create checkpoint, exit cleanly
- **Handoff:** Next agent resumes from checkpoint with fresh context

### 2. Checkpoint System

#### gsd-tools.js Commands Added

```bash
# Create checkpoint
checkpoint create --plan <path> --phase <N> --completed <1,2,3> \
  --current <4> --commits <hash1,hash2> --decisions <N>

# Load checkpoint
checkpoint load --plan <path>

# List checkpoints
checkpoint list [--phase <N>]

# Clear checkpoint (after completion)
checkpoint clear --plan <path>
```

#### Checkpoint Format

Stored at: `.planning/phases/XX-name/.checkpoints/{plan-name}-checkpoint.json`

```json
{
  "version": "1.0",
  "timestamp": "2026-02-11T12:00:00Z",
  "plan_path": ".planning/phases/01-initial-setup/01-01-PLAN.md",
  "phase": "01",
  "phase_dir": "01-initial-setup",
  "plan_name": "01-01",
  "completed_tasks": [1, 2, 3, 4, 5],
  "current_task": 6,
  "total_completed": 5,
  "commits": ["abc123", "def456"],
  "decisions_added": 2,
  "deviations": 1,
  "batch_complete": true
}
```

### 3. Workflow Updates

#### execute-plan.md

Added `<context_batching>` section with:
- Monitoring protocol (indicators of context saturation)
- Checkpoint creation instructions
- Resume protocol for next batch
- Benefits comparison table

Updated `identify_plan` step to:
- Check for existing checkpoints before starting
- Load and parse checkpoint data
- Resume from `current_task`

Added `clear_checkpoint` step to:
- Archive checkpoint after plan completion
- Move to `.checkpoints/archive/` for reference

#### execute-phase.md

Added `<batch_checkpoint_handling>` step with:
- Detection of batch handoff messages
- Checkpoint acknowledgment and reporting
- Continuation agent spawning instructions
- Success criteria for batched execution

Updated `<context_efficiency>` note to document batching behavior.

## Benefits

| Aspect | Before | After |
|---------|---------|--------|
| Context per task | Grows to 100% | Capped at ~50% |
| Quality | Degrades over time | Consistent |
| Token usage | Linear growth | Bounded |
| Recovery | Full replay | Minimal checkpoint |
| STATE.md size | Unbounded | Ready for rolling window (next phase) |

## Next Steps

1. **Implement STATE.md rolling window** - Keep only last 5 decisions, 3 sessions
2. **Archive old items** - Move to `.planning/archive/decisions.jsonl`
3. **Add metrics** - Track batch sizes, optimize threshold

## Files Modified

- `/home/feanor/.claude/get-shit-done/bin/gsd-tools.js`
  - Added: `cmdCheckpointCreate`, `cmdCheckpointLoad`, `cmdCheckpointList`, `cmdCheckpointClear`
  - Updated: Usage help, command routing

- `/home/feanor/.claude/get-shit-done/workflows/execute-plan.md`
  - Added: `<context_batching>` section
  - Updated: `identify_plan`, added `clear_checkpoint`

- `/home/feanor/.claude/get-shit-done/workflows/execute-phase.md`
  - Added: `<batch_checkpoint_handling>` section
  - Updated: `<context_efficiency>` note

## Testing

```bash
# Test create
node gsd-tools.js checkpoint create --plan ".planning/phases/01-test/01-01-PLAN.md" \
  --phase "01" --completed "1,2,3" --current "4" --commits "abc123" \
  --decisions "2" --deviations "1"

# Test list
node gsd-tools.js checkpoint list

# Test load
node gsd-tools.js checkpoint load --plan ".planning/phases/01-test/01-01-PLAN.md"

# Test clear
node gsd-tools.js checkpoint clear --plan ".planning/phases/01-test/01-01-PLAN.md"
```

All commands verified working.
