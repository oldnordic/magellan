# Magellan Development Rules - Grounded Tool Workflow

**Project:** Magellan - Graph Database Indexer
**Crate Version:** 3.3.5 | **Schema Version:** 14
**Last Updated:** 2026-05-10

---

## Shared Agent Workflow

Follow `/home/feanor/Projects/CLAUDE.md` for the shared rules: state assumptions before coding, use Magellan/llmgrep/Mirage for code-structure claims, keep edits surgical, preserve dirty worktree changes, and report fresh verification evidence before claiming completion. Repo-specific rules below add Magellan architecture, schema, CLI, and test requirements.

## Rust Code Standards (Microsoft Rust Guidelines + CI Self-Heal)

This project enforces **Microsoft's Pragmatic Rust Guidelines** (MIT license).
The pre-commit hook blocks commits that violate these rules. Subagent hooks
also enforce them on task completion.

**Mandatory rules:**
- `Result<T>` over `panic!()` — no unwrap/expect in non-test code
- `todo!()` / `unimplemented!()` are blocking violations — implement or remove
- `#[allow(dead_code)]` is forbidden — remove unused code, don't hide it
- `#[allow(...)]` requires `reason = "..."` — suppressions must be justified
- Public types must derive `Debug`
- No AI/LLM terminology in public docs (README.md, MANUAL.md, CHANGELOG.md)
- License: GPL-3.0 only (not GPL-3.0-or-later)
- No "production-ready" claims

**CI self-heal (mandatory after every push):**
After pushing to GitHub, you MUST check CI results and fix failures:
1. `gh run list --repo oldnordic/magellan --branch <branch> --limit 1`
2. If failed: `gh run view <id> --repo oldnordic/magellan --log-failed`
3. Classify: fmt | clippy | compile | test | terminology | license | claims | other
4. Fix in source, verify locally (`cargo fmt && cargo check && cargo test`)
5. Push fix: `git commit -m 'fix(ci): <description>' && git push`
6. If ambiguous: open a GitHub issue with diagnosis instead of guessing
7. Never stop working while CI is failing — the ci-check hook will block you

## Quick Start

```bash
cargo build --release                    # sqlite backend (default)
cargo test                               # run tests
cargo clippy --all-targets --all-features # lint
```

**Note:** `geometric-backend` requires `geographdb-core` at `../geographdb-core`. `cargo build --features geometric-backend` fails without it.

---

## Architecture

```
src/
├── main.rs, lib.rs, cli.rs          # CLI entry points
├── graph/                           # Core graph engine (40+ files)
│   ├── backend.rs                   # Backend abstraction (sqlite + geometric)
│   ├── ops.rs, call_ops.rs          # Graph mutations and call graph
│   ├── ast_*.rs, cfg_*.rs           # AST/CFG extraction
│   ├── scan.rs, indexer.rs          # File scanning and indexing
│   └── geo_*.rs                     # Geometric backend (3D spatial)
├── watcher/                         # Filesystem watch + debounce
├── lsp/                             # LSP server (tower-lsp feature)
└── *_cmd.rs                         # CLI subcommands
```

**Key design:** `graph::backend::GraphBackend` abstracts storage.
- `sqlite-backend` (default) — stable, rusqlite
- `geometric-backend` — experimental 3D spatial CFG indexing

## Database Convention

All projects: `.magellan/<project>.db`
- magellan → `.magellan/magellan.db`
- mirage → `.magellan/mirage.db`
- sqlitegraph → `.magellan/sqlitegraph.db`

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `sqlite-backend` | yes | Stable SQLite storage |
| `geometric-backend` | no | 3D spatial CFG (requires `../geographdb-core`) |
| `web-ui` | no | Axum web UI server |
| `llvm-cfg` | no | LLVM IR CFG for C/C++ |
| `telemetry` | no | Race/loop detection telemetry |
| `benchmarks` | no | Geometric benchmark suites |

---

## Code Quality Standards

**NO PLACEHOLDER CODE — EVER.** Strictly forbidden: `todo!()`, `unimplemented!()`, `// TODO:`, `// FIXME:`, stubs, mocks, `#[allow(dead_code)]`, commented-out code. Implement properly or remove. If deferring: file a GitHub issue + `#[cfg(test)]` guard.

## Mandatory AI Protocol

**Before ANY code change:**
```bash
magellan status --db .magellan/magellan.db 2>/dev/null || \
    magellan watch --root ./src --db .magellan/magellan.db --debounce-ms 500 &
```

**Then query before acting:**
```bash
magellan find --db .magellan/magellan.db --name "symbol_name"
llmgrep --db .magellan/magellan.db search --query "symbol_name"
magellan refs --db .magellan/magellan.db --name "func" --path src/file.rs --direction in|out
```

## Essential Commands

```bash
magellan status --db .magellan/magellan.db                    # DB health
magellan find --db .magellan/magellan.db --name "symbol"       # Find symbol
magellan refs --db .magellan/magellan.db --name "sym" --path src/file.rs --direction in|out
magellan query --db .magellan/magellan.db --file src/file.rs  # List symbols in file
magellan cycles --db .magellan/magellan.db                     # Cycle detection
magellan reachable --db .magellan/magellan.db --symbol <ID>    # Reachability
magellan dead-code --db .magellan/magellan.db --entry <ID>     # Dead code
```

All tools support `--output human|json|pretty`.

## Test Patterns

```bash
cargo test                                              # sqlite backend
cargo test --features geometric-backend --test geometric_cfg_tests  # geometric
cargo test --test system_workflow_tests                 # integration
cargo test --test migration_tests                       # migrations
cargo test --test cfg_mir_parity_tdd                    # MIR parity
```

## When to Re-Index

Only when: schema mismatch, first time, or `magellan status` shows 0 files/symbols.
```bash
pkill -f "magellan watch"
rm -rf .magellan/*.db*
magellan watch --root ./src --db .magellan/magellan.db --debounce-ms 500
```

## Known Build Quirks

- `unexpected_cfgs` warning for `debug-prints` — harmless. Suppress: `RUSTFLAGS='--allow=unexpected_cfgs' cargo build`
- `geometric-backend` requires local `../geographdb-core`

## Quick Reference: When to Use Which Tool

| Question | Tool | Command |
|----------|------|---------|
| Find complex functions | llmgrep | `search --query "x" --min-complexity 10` |
| What does this call? | Magellan | `refs --direction out` |
| What calls this? | Magellan | `refs --direction in` |
| Find CFG structure | Mirage | `cfg --function "name"` |
| Find cycles | splice | `cycles` |
| Find dead code | splice | `dead-code --entry "main"` |
| Edit with safety | splice | `patch --file src/lib.rs --symbol name --with impl.rs --dry-run` |

## Gotchas

- FTS5 prefix search is 2.5x faster than glob matching for symbol names: `sqlite3 <db> "SELECT e.id, e.name FROM graph_entities e JOIN symbol_fts fts ON e.id = fts.rowid WHERE fts.symbol_fts MATCH 'prefix*' LIMIT 50;"`
- `magellan find --list-glob "*"` returns empty if DB is unhealthy — use `magellan files --db <path>` instead.
- Rebuild FTS5 after manual changes: `sqlite3 <db> "INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild');"`
