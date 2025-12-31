# Repository Guidelines

## Project Structure & Module Organization
`src/main.rs` wires together the CLI while the `src/graph/` modules own persistence, counting, and query logic, and `src/ingest/` houses the tree-sitter based language adapters (Rust, Python, C/C++, Java, JS/TS). Command implementations live near the root (for example, `src/watch_cmd.rs`, `src/query_cmd.rs`). Integration-style tests are under `tests/` and mirror CLI flows (`cli_smoke_tests.rs`, `watcher_tests.rs`, etc.), while docs describing architecture and roadmaps live in `docs/`. Sample SQLite databases (`magellan.db` and variants) live at the repo root for quick experimentation.

## Build, Test, and Development Commands
- `cargo build` / `cargo build --release` – compile the CLI (release is what CI publishes).
- `cargo run -- watch --root <DIR> --db <FILE> --scan-initial` – exercise the watcher end-to-end against a fixture.
- `cargo test` – run unit + integration suites in `tests/`.
- `cargo fmt -- --check` and `cargo clippy --all-targets` – ensure style and lint gates pass before sending a PR.

## Coding Style & Naming Conventions
We follow Rust 2021 defaults: four-space indentation, snake_case modules/files, CamelCase types, and SCREAMING_SNAKE_CASE constants. Always format with `cargo fmt` prior to committing; annotate tricky branches with concise comments rather than restating code. Prefer early returns plus `anyhow::Result` for CLI fallible paths, and keep command parsing isolated to the corresponding `*_cmd.rs` file to reduce churn.

## Testing Guidelines
Add scenario-driven tests in `tests/` when changing CLI behavior (e.g., add `foo_cmd_tests.rs`). Use descriptive function names like `test_watch_reports_create_events` and keep fixtures under `tests/data/` if needed. Run `cargo test -- --nocapture` when debugging watcher output. For database changes, seed a temp sqlitegraph via the helpers in `tests/indexer_tests.rs` and assert on row counts or graph edges.

## Commit & Pull Request Guidelines
The Git history uses Conventional Commits (`fix:`, `chore:`, `bump:`). Keep subjects in the imperative mood and scope them when helpful (`fix(watch): debounce parser errors`). PRs should describe the user-visible effect, reference related issues, and include CLI transcripts or JSON diffs when touching the graph schema. Screenshots/log excerpts are appreciated for watcher races, and every PR should state which commands/tests were run locally.
