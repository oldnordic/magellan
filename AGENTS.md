---
description: Magellan agent for code graph indexing with exact symbol location using tree-sitter. Enforces epistemic discipline - NEVER GUESS, ALWAYS VERIFY.
mode: subagent
temperature: 0.1
permission:
  edit: allow
  bash: allow
  webfetch: allow
tools:
  write: true
  edit: true
---

# Development Rules - NON-NEGOTIABLE

**Last Updated:** 2026-01-09
**Status**: MANDATORY for ALL code changes

---

## EPISTEMIC DISCIPLINE (NON-NEGOTIABLE)

### Rule #1: NEVER GUESS - ALWAYS VERIFY

Before ANY code change:

```bash
# 1. READ the source code first
Read /path/to/file.rs

# 2. CHECK the actual database schema
sqlite3 path/to.db ".schema"

# 3. VERIFY with compiler/tests
cargo test
cargo check
```

**FORBIDDEN:**
- ❌ Assuming what a function does without reading it
- ❌ Guessing database table/column names
- ❌ Writing code based on "how it should work"
- ❌ Using grep/rg to find symbols (use Magellan query instead)

---

### Rule #2: STORE ARCHITECTURAL DECISIONS

Before ANY code change, document using `operations.db`:

**For Bugfixes:**
```sql
INSERT INTO architectural_decisions (
    project, decision, reasoning, alternatives, trade_offs,
    session_id, created_at_epoch, created_at
) VALUES (
    'magellan',
    'Fix: [brief description]',
    'Root cause: [PROVEN with evidence]. Fix addresses [specific issue].',
    'Alternative approaches considered...',
    'Risk: [what could break]. Mitigation: [how we prevent regression].',
    'session-id',
    strftime('%s', 'now'),
    datetime('now')
);
```

---

### Rule #3: TDD - PROVE IT FIRST

1. Write a test that FAILS for the expected reason
2. Show the failure output
3. Fix the code
4. Show the test now passes

```bash
cargo test test_name
```

---

### Rule #4: USE PROPER TOOLS

| Task | Use This | NEVER Use |
|------|----------|-----------|
| Find symbols | Magellan query / `find_symbols` | grep/rg |
| Read code | `Read` tool | cat/head/tail |
| Check schema | `sqlite3 .db ".schema"` | guessing |

---

### Rule #5: CITE YOUR SOURCES

Before making changes, cite EXACTLY what you read:

```
I read /home/feanor/Projects/magellan/src/file.rs:123-456
The function `do_thing` takes parameters X, Y, Z
I checked codegraph.db schema
Table `symbols` has columns: id, name, kind, file_path, ...
Therefore I will change...
```

---

### Rule #6: NO DIRTY FIXES

- ❌ "TODO: fix later"
- ❌ `#[allow(dead_code)]` to silence warnings
- ❌ Commenting out broken code
- ❌ Minimal/half-hearted fixes

**ONLY**: Complete, tested, documented code.

---

## RUST-SPECIFIC STANDARDS

### Code Quality
- Max 300 LOC per file (600 with justification)
- No `unwrap()` in prod paths
- Proper error handling with `anyhow::Result`

### Tree-Sitter Integration
- Always validate parsing results before indexing
- Handle language edge cases explicitly
- Test with real-world code samples

---

## Project-Specific Guidelines

### Project Structure & Module Organization
`src/main.rs` wires together the CLI while the `src/graph/` modules own persistence, counting, and query logic, and `src/ingest/` houses the tree-sitter based language adapters (Rust, Python, C/C++, Java, JS/TS). Command implementations live near the root (for example, `src/watch_cmd.rs`, `src/query_cmd.rs`). Integration-style tests are under `tests/` and mirror CLI flows (`cli_smoke_tests.rs`, `watcher_tests.rs`, etc.), while docs describing architecture and roadmaps live in `docs/`. Sample SQLite databases (`magellan.db` and variants) live at the repo root for quick experimentation.

### Build, Test, and Development Commands
- `cargo build` / `cargo build --release` – compile the CLI (release is what CI publishes).
- `cargo run -- watch --root <DIR> --db <FILE> --scan-initial` – exercise the watcher end-to-end against a fixture.
- `cargo test` – run unit + integration suites in `tests/`.
- `cargo fmt -- --check` and `cargo clippy --all-targets` – ensure style and lint gates pass before sending a PR.

### Coding Style & Naming Conventions
We follow Rust 2021 defaults: four-space indentation, snake_case modules/files, CamelCase types, and SCREAMING_SNAKE_CASE constants. Always format with `cargo fmt` prior to committing; annotate tricky branches with concise comments rather than restating code. Prefer early returns plus `anyhow::Result` for CLI fallible paths, and keep command parsing isolated to the corresponding `*_cmd.rs` file to reduce churn.

### Testing Guidelines
Add scenario-driven tests in `tests/` when changing CLI behavior (e.g., add `foo_cmd_tests.rs`). Use descriptive function names like `test_watch_reports_create_events` and keep fixtures under `tests/data/` if needed. Run `cargo test -- --nocapture` when debugging watcher output. For database changes, seed a temp sqlitegraph via the helpers in `tests/indexer_tests.rs` and assert on row counts or graph edges.

### Commit & Pull Request Guidelines
The Git history uses Conventional Commits (`fix:`, `chore:`, `bump:`). Keep subjects in the imperative mood and scope them when helpful (`fix(watch): debounce parser errors`). PRs should describe the user-visible effect, reference related issues, and include CLI transcripts or JSON diffs when touching the graph schema. Screenshots/log excerpts are appreciated for watcher races, and every PR should state which commands/tests were run locally.

---

## When In Doubt

1. Read the source code
2. Check the database schema
3. Run tests
4. Store a decision
5. Ask for clarification

**DO NOT GUESS.**
