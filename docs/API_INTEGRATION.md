# Magellan API Integration

**Version:** 4.1.0 (unreleased)

This guide is for downstream tools that use Magellan as a Rust library or invoke
the CLI and parse JSON output.

## Primary API

Use `CodeGraph` with a SQLite `.db` path:

```rust
use magellan::CodeGraph;

let mut graph = CodeGraph::open("code.db")?;
let source = std::fs::read("src/main.rs")?;
graph.index_file("src/main.rs", &source)?;

let symbols = graph.symbols_in_file("src/main.rs")?;
```

`CodeGraph::open()` creates a new database when the path does not exist. For
existing databases it runs compatibility checks before writing.

## Common Operations

```rust
let file_count = graph.count_files()?;
let symbol_count = graph.count_symbols()?;
let reference_count = graph.count_references()?;
let call_count = graph.count_calls()?;
let chunk_count = graph.count_chunks()?;

let symbols = graph.symbols_in_file("src/lib.rs")?;
let refs = graph.references_to_symbol("symbol_name")?;
```

For precise lookup across ambiguous names, prefer stable `symbol_id` fields from
query output.

## CLI JSON Contract

For tool integration through the CLI, use:

```bash
magellan <command> --output json
magellan <command> --output pretty
```

JSON responses include:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "hex-timestamp-hex-pid",
  "data": {}
}
```

See [JSON_EXPORT_FORMAT.md](JSON_EXPORT_FORMAT.md) for the response wrapper and
common span types.

## Stable Status Shape

`status` always includes coverage fields:

```json
{
  "files": 10,
  "symbols": 100,
  "references": 20,
  "calls": 15,
  "code_chunks": 100,
  "coverage": {
    "available": false,
    "covered_blocks": 0,
    "covered_edges": 0
  }
}
```

When coverage data exists, `source`, `revision`, and `ingested_at` are present
when known.

## Import/Export Integration

Magellan can exchange data through CLI formats:

```bash
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip --output graph.scip
magellan export --db code.db --format lsif --output graph.lsif
magellan import-lsif --db code.db dependency.lsif
```

## Framework API (v4.1.0+)

`MagellanFramework` provides a single entry point for cross-project queries in
downstream Rust tools:

```rust
use magellan::{MagellanFramework, FrameworkSymbol};

// Open all projects from the user registry (~/.config/magellan/registry.toml)
let fw = MagellanFramework::from_registry()?;

// Cross-project symbol search
let hits: Vec<FrameworkSymbol> = fw.find("parse_args")?;
for hit in &hits {
    println!("{} — {} ({}:{})", hit.name, hit.project, hit.file, hit.line);
}

// Intent-routed natural language query across all projects
let output: String = fw.ask("who calls index_file")?;
println!("{}", output);

// Scope to one project
let proj = fw.project("magellan")?;
let syms = proj.graph().symbols_in_file("src/main.rs")?;
```

Constructors:

| Method | Source |
|--------|--------|
| `MagellanFramework::from_registry()` | `~/.config/magellan/registry.toml` |
| `MagellanFramework::from_registry_file(path)` | Explicit registry file |
| `MagellanFramework::from_db_paths(entries)` | `Vec<(name, db_path)>` |

`FrameworkSymbol` and `ProjectHandle` are re-exported from the crate root.

## Navigate API (v4.1.0+)

`navigate_cmd::run_navigate` generates grounded investigation packets in-process:

```rust
use magellan::navigate_cmd::{run_navigate, NavigateConfig};
use std::path::PathBuf;

run_navigate(NavigateConfig {
    db_path: PathBuf::from(".magellan/code.db"),
    task: "parse_watch_args error handling".into(),
    depth: 2,
    budget: 4000,
    limit: 5,
    concise: false,
    with_llmgrep: false,
    with_mirage: false,
})?;
```

The packet is written to stdout as markdown. Use `--concise` + `--budget N` for
token-constrained output (1 token ≈ 4 chars). `extract_terms()` and
`truncate_to_budget()` are also public for reuse.

## Schema Versions

- Magellan database schema: `16`
- JSON response schema: `1.0.0`

Consumers should treat both as compatibility boundaries.

## Graph Memory API

Magellan provides CLI commands for managing external document sources and extracted
facts:

### Source Inventory

```bash
# List indexed source documents
magellan source-inventory --db code.db --list

# Show stale documents (changed since last scan)
magellan source-inventory --db code.db --stale

# Scan a directory for source documents
magellan source-inventory --db code.db --scan ./wiki wiki
magellan source-inventory --db code.db --scan ./docs markdown

# Filter by kind
magellan source-inventory --db code.db --list --kind wiki
```

### Candidate Facts

```bash
# Submit a candidate fact
magellan candidate-fact submit --db code.db \
  --from-source <DOC_ID> \
  --subject-type Symbol \
  --subject-key "parse" \
  --predicate "has_complexity" \
  --properties '{"value": 8}'

# List facts by status
magellan candidate-fact list --db code.db --status pending
magellan candidate-fact list --db code.db --status accepted

# Validate (accept) a fact
magellan candidate-fact validate --db code.db --candidate-id cf_abc123

# Review queue
magellan candidate-fact review-queue --db code.db --limit 20
```

Fact statuses: `pending` → `accepted` or `rejected`. Candidate IDs are
auto-generated UUIDs when omitted.

## Service Daemon Socket API (v4.1.0+)

The daemon exposes a JSON-RPC API over a Unix domain socket.  The socket path
follows `XDG_RUNTIME_DIR` when the variable is set (e.g.
`/run/user/1000/magellan.sock` under systemd user services); otherwise it falls
back to `/tmp/magellan.sock`.  All callers use `socket_path()` for portability.

All messages are newline-delimited JSON. Start the daemon with
`magellan service start` (or `systemctl --user start magellan`) before
connecting.

Wire format:

```json
{ "method": "ping", "params": {} }
```

### Admin Methods

| Method | Params | Returns |
|--------|--------|---------|
| `ping` | `{}` | `{ "pong": true }` |
| `list` | `{}` | `{ "projects": [...] }` |
| `status` | `{ "name": "myproject" }` | project entry |
| `register` | `{ "name"`, `"root"`, `"source?"` } | `{ "registered": "name" }` |
| `unregister` | `{ "name" }` | `{ "unregistered": true }` |
| `pause` | `{ "name" }` | `{ "paused": true }` |
| `resume` | `{ "name" }` | `{ "resumed": true }` |

The `register` method stores the project's database at the canonical path
`~/.magellan/<name>/<name>.db` (derived from the registry, not a param).
`root` defaults to `"."` if omitted.

### Cross-Project Query Methods

| Method | Key Params | Returns |
|--------|-----------|---------|
| `query.find` | `name`, `file?`, `depth?`, `callers?`, `callees?` | `{ "results": [ProjectSymbolMatch] }` |
| `query.context` | `name`, `file?`, `callers?`, `callees?`, `depth?` | per-project symbol match with caller/callee arrays |
| `query.compare` | `name`, `projects: [string]` | `{ "comparisons": [..., "similarity_score"?] }` |
| `query.build-index` | `{}` | `{ "pairs_inserted": N }` |
| `query.suggest` | `from_project`, `name`, `to_project?` | `{ "suggestions": [{ project, symbol, file, similarity_score }] }` |

`query.build-index` populates `pattern_cross_refs` via pairwise cosine
similarity across all enabled registry projects (threshold 0.70).

### Evolution Loop Methods

| Method | Key Params | Returns |
|--------|-----------|---------|
| `evolve.analyze` | `project?`, `limit?` | `{ "candidates": [HotspotCandidate] }` |
| `evolve.retrieve` | `project`, `symbol`, `to_project?`, `limit?` | `{ "analogues": [...] }` |
| `evolve.propose` | `project`, `symbol`, `patch_diff`, `candidate_id?` | `{ "candidate_id": "..." }` |
| `evolve.candidates` | `project?`, `status?`, `limit?` | `{ "candidates": [CandidateRecord] }` |
| `evolve.verify` | `candidate_id`, `project_root` | `{ "status": "verified" \| "rejected" }` |
| `evolve.promote` | `candidate_id` | `{ "promoted": true }` |
| `evolve.reject` | `candidate_id`, `reason?` | `{ "rejected": true }` |

`evolve.verify` creates a temp worktree copy of `project_root`, applies
`patch_diff` via `patch -p0`, auto-detects the test harness (`cargo test` /
`pytest` / `npm test`), runs tests, and updates candidate status.

Error code `-32006` is returned when a `candidate_id` is not found.

Candidate status lifecycle: `pending` → `verified` or `rejected` → `promoted`.

## Path Handling

Normalize file paths consistently when integrating:

- pass project-relative paths when possible
- use `--root` on commands that accept it
- do not persist SQLite row IDs as cross-run identifiers
- persist `symbol_id`, `span_id`, FQN, and file path instead

## Optional Features

The default public integration path does not require optional features.

`external-tools-cfg` can be enabled for additional C/C++ and Java CFG extraction
using installed external tools. Integrators should treat this as an optional
enhancement and gracefully handle its absence.
