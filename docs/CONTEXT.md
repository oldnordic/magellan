# Magellan: Context & Current State

**Created**: 2026-02-03
**Version**: v2.0.0
**Purpose**: Quick grounding for AI sessions — read this first

---

## What Magellan Is

**Magellan is a code graph indexing engine for 7 programming languages.**

It provides EXACT symbol locations (byte spans) from AST parsing. No guessing, no text searching — actual parsed structure.

```
You think: "Where is function X?"
Magellan KNOWS: "file.rs, byte 1234-1456, line 42-47"
```

---

## Part of The Larger Vision

Magellan is the TRUTH layer of the grounding substrate:

```
┌─────────────────────────────────────────────────────────────┐
│                     OdinCode (Orchestration)                  │
│  Internal prompts: "Use Magellan first, never Read files"   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       Magellan (Truth)                       │
│  "Where is symbol X?"  "Who calls Y?"  "What's reachable?" │
│                                                              │
│  Persistent .codemcp/codegraph.db survives context resets    │
└─────────────────────────────────────────────────────────────┘
```

**Read `/home/feanor/Projects/VISION.md` for the full context.**

---

## Actual Current State (2026-02-03)

**Another LLM is working on Magellan fixes.** This document is for reference.

### What Works ✅

| Component | Status | Notes |
|-----------|--------|-------|
| Multi-language parsing | ✅ Working | 7 languages, tree-sitter |
| Symbol discovery | ✅ Working | `find_symbols` with byte spans |
| File watching | ✅ Working | Auto-reindex on changes |
| Graph algorithms | ✅ Working | Reachability, dead code, cycles, paths, slicing |
| Database schema | ✅ Working | v6 with BLAKE3 stable IDs, migrations |
| CLI interface | ✅ Working | 20+ commands, JSON output |
| Test coverage | ✅ Passing | 175+ tests |

**Health score: ~95%** — Core solid, production ready

---

## Supported Languages (All 7)

| Language | Parser | Status |
|----------|--------|--------|
| Rust | tree-sitter-rust | ✅ Full |
| Python | tree-sitter-python | ✅ Full |
| C | tree-sitter-c | ✅ Full |
| C++ | tree-sitter-cpp | ✅ Full |
| Java | tree-sitter-java | ✅ Full |
| JavaScript | tree-sitter-javascript | ✅ Full |
| TypeScript | tree-sitter-typescript | ✅ Full |

---

## CLI Quick Reference

```bash
# Start watcher (keeps DB updated)
magellan watch --root ./src --db .codemcp/codegraph.db

# Find symbol definition
magellan find --name "my_function"

# Find references
magellan refs --name "my_function" --path src/file.rs

# List symbols in file
magellan query --file src/file.rs

# Reachability analysis
magellan reachable --symbol <ID>

# Dead code detection
magellan dead-code --entry <ID>

# Cycle detection (SCCs)
magellan cycles
```

---

## Integration Points

### Used By

- **Splice** — Gets symbol locations for refactoring
- **llmgrep** — Uses Magellan database for queries
- **OdinCode** — Direct DB queries via `.codemcp/codegraph.db`

### Database Location

```
.codemcp/codegraph.db  ← SQLite database, survives sessions
```

---

## Code Organization

```
src/
├── graph/
│   ├── symbols.rs       ← Symbol indexing
│   ├── references.rs    ← Reference edges with byte offsets
│   ├── call_ops.rs      ← Call graph operations
│   ├── ambiguity.rs     ← Ambiguity modeling
│   └── mod.rs           ← Graph operations
├── query/
│   └── mod.rs           ← Query operations
├── cmd/
│   ├── find_cmd.rs      ← find command
│   ├── refs_cmd.rs      ← references command (returns byte offsets)
│   └── ...              ← Other commands
└── lib.rs               ← Main interface
```

---

## For AI Sessions

**When working with Magellan:**

1. **Always check watcher status first** — `magellan status`
2. **Use find_symbols for location** — Never grep
3. **Use refs for incoming/outgoing** — Don't guess call graphs
4. **Query database directly** — It's the source of truth

**When I say "let me grep for that":**
- Stop me
- Say "use magellan find"
- Say "use llmgrep search"

**When I say "let me read that file":**
- Stop me
- Say "use magellan query --file"
- Say "use magellan find --name X"

---

## Known Limitations

1. **No macro reference tracking** — Macros expand, refs not tracked
2. **No fully-qualified paths** — Uses simple names
3. **Single-hop re-exports** — Limited depth following
4. **No gitignore auto-detection** — Manual `--root` required

---

## Database Schema

Key tables:
- `symbols` — All definitions with byte spans
- `edges` — Relationships (REFERENCES, CALLS, DEFINES, etc.)
- `files` — Indexed files with timestamps
- `snapshots` — Database snapshots for rollback

---

## Performance

| Operation | Latency | Notes |
|-----------|---------|-------|
| Initial index | 2-10s | Depends on codebase size |
| Incremental update | <1s | Single file changes |
| find_symbols | 10-50ms | O(log n) lookup |
| refs query | 50-200ms | Graph traversal |
| reachability | 100-500ms | Algorithm query |

---

## The Real Priority

| Priority | Item | Effort | Status |
|----------|------|--------|--------|
| 🟡 P1 | Gitignore integration | 2-3 days | Not started |
| 🟢 P2 | `--explain-query` flag | 1 day | Already implemented |

---

*Last updated: 2026-02-04*
*Read `/home/feanor/Projects/VISION.md` for the full ecosystem context*
