# Magellan Toolchain - Master Plan

**Date:** 2026-05-01
**Tools:** magellan, llmgrep, mirage, splice
**Philosophy:** LLM-native code intelligence. No stubs. No premature completion. Validate before mutation.

---

## MASTER PLAN - READ THIS FIRST

### Before Any Code Change

1. **Check for existing stubs** - `grep -rE 'TODO\(|unimplemented!|panic!' src/`
2. **Run `cargo check`** - must pass before writing new code
3. **Verify database health** - `magellan status --db .magellan/magellan.db`

### After Any Code Change

1. **Run `cargo build`** - must compile
2. **Run `cargo test`** - must pass
3. **Run `cargo clippy`** - must be clean
4. **Verify no new stubs** - `grep -rE 'TODO\(|unimplemented!' src/`
5. **Verify symbols indexed** - `magellan find --name "<new_symbol>"`

### Before Claiming "Done"

All of the above must pass. "Done" is NOT:
- Code that compiles ✓ (tests might fail)
- Tests that pass ✓ (stubs might exist)
- PR approved ✓ (validation might be bypassed)
- Merged ✓ (hooks might not have fired)

**"Done" means: ALL verification gates passed, zero stubs in codebase.**

---

## Part 1: Philosophy - Why This Toolchain Exists

### The Problem with Human-Oriented Tools

Traditional tools (grep, ripgrep, find, cat) were built for humans:

| Human Tool | Output | LLM Problem |
|------------|--------|-------------|
| `grep -r "pattern"` | Lines of text | LLM must parse and guess which symbol is which |
| `find . -name "*.rs"` | File paths | No symbol info, no call graph, no context |
| `cat file.rs` | Full file (500+ lines) | Wastes tokens, no structure |
| `rg "fn" -A5` | Text matches | No span precision, no complexity info |

### LLM-Native Approach

This toolchain returns **structured graph data**:
```
llmgrep search --query "function" --output json
→ [{"symbol_id": "abc123", "name": "process", "file": "src/proc.rs", "line": 42,
    "complexity": 12, "fan_in": 5, "fan_out": 8}]

mirage cfg --function "process" --output json
→ {"blocks": [{"id": 0, "coord_x": 0, "coord_y": 0, "coord_z": 0, "edges": [...]}],
   "paths": 47, "loops": 2}
```

**Token savings:** ~200 tokens vs ~15,000 for full file read.

### Core Principles

1. **No guessing** — LLMs get precise symbol IDs, spans, coordinates
2. **No full reads** — Structured queries return exactly what is needed
3. **No stubs** — Every feature either works or fails explicitly
4. **Validate before mutation** — Splice uses LSP to verify before changes
5. **LLM enforcement** — Hooks and skills prevent premature completion

---

## Part 2: LLM Enforcement - Preventing Incomplete Work

### The Problem

LLM coding agents consistently fail at the finish:
- Write code → declare "implemented" → tokens run low → move on
- Leave `TODO()`, `unimplemented!()`, placeholder functions
- Don't run tests, don't verify build, don't check clippy

**This toolchain MUST enforce completion, not trust the LLM's self-reporting.**

### Enforcement Layers

#### Layer 1: Pre-Commit Hook (Required)

Add to `.claude/settings.json`:
```json
{
  "hooks": {
    "pre-commit": [
      {
        "command": "grep -rE 'TODO\\(|unimplemented!|panic!' src/ --include='*.rs' || true",
        "stop_on_nonzero": true,
        "report": "STUBS DETECTED - remove before commit"
      },
      {
        "command": "cargo test 2>&1 | tail -5",
        "stop_on_nonzero": true,
        "report": "Tests failed - cannot commit"
      },
      {
        "command": "cargo clippy --all-targets --all-features 2>&1 | grep -E '^error' | head -3",
        "stop_on_nonzero": true,
        "report": "Clippy errors - cannot commit"
      }
    ]
  }
}
```

#### Layer 2: Verification Skill (Required Before "Done")

```
superpowers:verification-before-completion

Before claiming ANY task complete, MUST run:
1. grep -rE 'TODO\(|unimplemented!|panic!' src/ → 0 matches
2. cargo check → exit 0
3. cargo test → exit 0
4. cargo clippy --all-targets --all-features → 0 errors
5. magellan find --name "<new>" → symbol found

DO NOT claim "done" until ALL checks pass.
```

#### Layer 3: Validation Pipeline Script

Create `scripts/validate-completion.sh`:
```bash
#!/bin/bash
set -e

echo "=== VALIDATION PIPELINE ==="

# Check 1: No stubs
STUB_COUNT=$(grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs' 2>/dev/null | wc -l)
if [ $STUB_COUNT -gt 0 ]; then
  echo "❌ FAILED: $STUB_COUNT stubs found"
  exit 1
fi

# Check 2: Build
cargo check 2>&1 | tail -3

# Check 3: Tests
cargo test 2>&1 | tail -5

# Check 4: Clippy
cargo clippy --all-targets --all-features 2>&1 | grep -E '^error' | head -3

# Check 5: DB health
magellan status --db .magellan/magellan.db | grep -E "files:|symbols:"

echo "=== ALL CHECKS PASSED ==="
```

---

## Part 3: Current State - Command Inventory

### 3.1 Magellan (Core CLI)

| Command | Status | Fix Priority |
|---------|--------|-------------|
| `status` | ✅ WORKS | - |
| `find` | ✅ WORKS | - |
| `refs` | ✅ WORKS | - |
| `query` | ✅ WORKS | - |
| `doctor` | ⚠️ PARTIAL | Fix --fix |
| `watch` | ✅ WORKS | - |
| `backfill` | ❌ BROKEN | **P0** - shared backend regression |
| `dead-code` | ❌ BROKEN | **P0** - Node ID format mismatch |
| `cross-file-refs` | ✅ WORKS | - |

### 3.2 Splice (Refactoring Kernel)

| Command | Status | Fix Priority |
|---------|--------|-------------|
| `status` | ✅ WORKS | - |
| `find` | ✅ WORKS | - |
| `cycles` | ✅ WORKS | - |
| `dead-code` | ❌ BROKEN | **P0** - symbol lookup logic broken |
| `reachable` | ❌ BROKEN | **P0** - same root cause |
| `query` | ⚠️ CONFUSING | P1 - inconsistent --label vs --file |

### 3.3 Mirage (CFG Analysis)

| Command | Status | Fix Priority |
|---------|--------|-------------|
| `status` | ✅ WORKS | - |
| `cfg` | ✅ WORKS | - |
| `loops` | ✅ WORKS | - |
| `paths` | ✅ WORKS | - |
| `hotspots` | ❌ BROKEN | **P0** - no entry point |
| `unreachable` | ❌ BROKEN | **P0** - wrong argument name |
| `hotpaths` | ❌ BROKEN | P1 - no function discovery |

### 3.4 LLMgrep (Semantic Search)

| Command | Status | Fix Priority |
|---------|--------|-------------|
| `search` | ⚠️ INCONSISTENT | P1 - "fn" returns 0 |
| `ast` | ✅ WORKS | - |

---

## Part 4: Implementation Roadmap

### Phase 0: LLM Enforcement Setup (Before Anything Else)

**Purpose:** Prevent stubs from entering the codebase during fixes.

- [ ] Add pre-commit hook to `.claude/settings.json`
- [ ] Create `scripts/validate-completion.sh`
- [ ] Create `superpowers:verification-before-completion` skill
- [ ] Create `superpowers:no-stubs-enforcement` skill
- [ ] Test enforcement against intentionally stubbed code

### Phase 1: Fix P0 Commands (1-2 days)

**Purpose:** Make the toolchain trustworthy. LLMs must be able to rely on query results.

1. Fix `magellan backfill` — Update shared backend connection pattern
2. Fix `magellan dead-code` — Accept numeric and string IDs
3. Fix `splice dead-code` — Fix symbol lookup in file path logic
4. Fix `splice reachable` — Same fix
5. Fix `mirage hotspots` — Add --entry flag, auto-detect main
6. Fix `mirage unreachable` — Fix argument name

### Phase 2: Consistency (1-2 days)

1. Fix `llmgrep search --query "fn"` — Handle Rust keywords
2. Fix `splice query --file` — Add alias for consistency with magellan
3. Add `mirage functions` — Function discovery command
4. Error format standardization — All tools same JSON structure

### Phase 3: Cross-Project Federation (2-3 days)

1. `magellan registry scan` — Discover .magellan/*.db files
2. `magellan registry list` — List registered projects
3. Cross-DB queries — Federate find/refs/query across DBs
4. `--all-projects` flag — Apply to all tools

### Phase 4: LLM Integration (2-3 days)

1. Config file parsing — `~/.config/magellan/config.toml`
2. Embedding endpoint — For semantic search
3. Completion endpoint — For pattern analysis
4. `llmgrep search --semantic` — Vector similarity search

### Phase 5: Validation Pipeline (2-3 days)

1. Splice LSP validation — Before every mutation
2. Pre-commit hooks — Cycle detection, complexity check
3. Proof/checksum system — Audit trail for changes
4. Verification commands — Path integrity, symbol refs

---

## Part 5: Verification Gates

Every phase must pass these gates before proceeding:

### Gate 1: No Stubs
```bash
grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs'
# Must return: (no output)
```

### Gate 2: Build
```bash
cargo check --all-features
# Must exit 0
```

### Gate 3: Tests
```bash
cargo test --all-features
# Must exit 0
```

### Gate 4: Clippy
```bash
cargo clippy --all-targets --all-features
# Must have 0 errors
```

### Gate 5: Database
```bash
magellan status --db .magellan/magellan.db
# Must show files > 0, symbols > 0
```

---

## Part 6: Token Pressure Mitigation

When tokens run low, stub temptation increases. Mitigation:

1. **Reserve 20% tokens for verification** — Don't allow claim unless tokens > 80%
2. **Split large tasks** — Small tasks = less tokens = less stub risk
3. **Auto-verify on low tokens** — If tokens < 20%, run validation pipeline
4. **Checkpoint every 100 lines** — Verification gate every 100 lines of code

---

## Part 7: Cross-Reference

### Command → Failure Mode Map

| Command Fails | LLM Failure Mode Prevented |
|--------------|---------------------------|
| `magellan find` broken | LLM guesses symbol name, creates duplicate |
| `magellan refs` broken | LLM doesn't trace callers, leaves stale refs |
| `mirage cfg` broken | LLM doesn't check complexity, writes spaghetti |
| `splice rename` broken | LLM changes symbol without updating callers |
| `splice cycles` broken | LLM introduces circular dependencies |

### Tool → LLM Benefit Map

| Tool | LLM Gets | LLM Avoids |
|------|----------|------------|
| magellan | symbol IDs, call graph, refs | wrong symbol, stale refs |
| llmgrep | semantic search, complexity filters | full file reads, context waste |
| mirage | CFG with 4D coords, path enumeration | complex code, missed paths |
| splice | LSP-validated mutations, proof trail | broken edits, unreviewed changes |

---

## Quick Reference

### Before Any Work
```bash
magellan status --db .magellan/magellan.db
cargo check
grep -rE 'TODO\(|unimplemented!' src/ || true
```

### After Any Change
```bash
cargo build
cargo test
cargo clippy --all-targets
./scripts/validate-completion.sh
```

### Before "Done"
```bash
grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs'
cargo test --all-features
cargo clippy --all-targets --all-features
```

---

*Document version 3.0 - 2026-05-01*
*Master plan - read before any work. Enforcement before implementation.*