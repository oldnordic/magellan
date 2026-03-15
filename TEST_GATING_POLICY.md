# Test Gating Policy

**Version:** 1.0  
**Applies to:** Magellan, llmgrep, Mirage

## Policy

For any change touching the following areas, the change is **NOT complete** unless all four test tiers pass:

1. **Unit tests** pass
2. **Component tests** pass  
3. **Integration tests** pass
4. **Real workflow/system harness** passes

## Scope

This policy applies to changes in these categories:

### Watch/Indexing
- File watching logic
- Directory scanning
- Event processing
- Dirty path handling
- Re-index semantics

### Persistence/Save/Reopen
- Database save/load
- Format changes
- Schema migrations
- Reopen behavior

### Backend Routing
- Backend selection
- Feature-gated code paths
- Backend-specific optimizations
- Cross-backend compatibility

### Symbol Resolution
- Name lookup
- FQN resolution
- Ambiguity handling
- Path disambiguation
- Numeric ID resolution

### CFG/Graph Analysis
- CFG extraction
- Graph algorithms (cycles, dominators, paths)
- Call graph construction
- Analysis command output

### CLI Orchestration
- Command dispatch
- Argument parsing
- Error handling
- Output formatting

## Test Tiers

### 1. Unit Tests
- Fast, isolated tests
- Mock/stub where appropriate
- Located in `src/*/tests/` or inline

### 2. Component Tests
- Test module interactions
- Real filesystem where needed
- Located in `tests/*_tests.rs`

### 3. Integration Tests
- Cross-module workflows
- Real .geo databases
- Real subprocess execution
- Located in `tests/integration_*.rs` or `tests/system_*.rs`

### 4. System/Workflow Harness
- End-to-end real tool execution
- Uses installed binaries
- Tests actual user workflows
- Located in `tests/cross_tool_system_harness.rs`

## Verification Commands

Before marking any change complete, run:

```bash
# Magellan
cargo test --features geometric-backend
cargo test --test system_workflow_tests --features geometric-backend
cargo test --test cross_tool_system_harness --features geometric-backend

# llmgrep
cargo test
cargo test --test system_integration_tests

# Mirage
cargo test
cargo test --test system_integration_tests
```

## CI Requirements

All PRs must pass:
- [ ] Unit test suite
- [ ] Component test suite
- [ ] Integration test suite
- [ ] System harness (if modified files in policy scope)

## Exceptions

Documentation-only changes (no code) are exempt from this policy.

Test-only changes are exempt from the system harness requirement if they don't modify production code paths.

## Enforcement

This policy is enforced by:
1. CI blocking merge until all tiers pass
2. Code review checklist
3. Release blockers

---

**Adopted:** 2026-03-12  
**Enforced by:** CI pipeline + code review
