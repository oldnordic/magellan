# Magellan API Integration

**Version:** 4.2.0 (unreleased)

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

- Magellan database schema: `17`
- JSON response schema: `1.0.0`
- Explore JSON contract: `1.0.0` (see Explore API below)

Consumers should treat both as compatibility boundaries.

## SymbolNavigator API (v4.2.0+)

`SymbolNavigator` provides programmatic stepable graph traversal:

```rust
use magellan::CodeGraph;

let graph = CodeGraph::open("code.db")?;
let nav = graph.navigator();

// Resolve by name
let syms = nav.resolve("index_file")?;
let syms = nav.resolve_by_prefix("index_")?;

// Symbol info
let info = nav.info(syms[0].id)?;

// Expand connected entities
let neighbors = nav.expand(syms[0].id)?;
let typed = nav.expand_typed(syms[0].id, "REFERENCES")?;

// Depth-aware call graph
let callers = nav.k_hop_callers(syms[0].id, 2)?;  // depth 2 callers
let callees = nav.k_hop_callees(syms[0].id, 3)?;  // depth 3 callees
let refs = nav.k_hop_references(syms[0].id, 1)?;

// Chain traversal (sqlitegraph ChainStep)
let chain = nav.chain(syms[0].id, &[
    (">CALLER".into(), false),
    (">CALLS".into(), false),
])?;

// Pattern matching
let matches = nav.pattern(&["Symbol", "Call"], &[("CALLER", "CALLS")])?;
```

`SymbolInfo` fields: `id`, `name`, `kind` (normalized), `file`, `line` (start_line).
`DepthSymbol` adds `depth` (call-hop count).

### Explore CLI API

```bash
magellan explore --db code.db --symbol "index_file" --json
magellan explore --db code.db --id 42 --callers --depth 2 --json
magellan explore --db code.db --id 42 --callees --depth 3 --json
magellan explore --db code.db --id 42 --edges --json
magellan explore --db code.db --id 42 --chain ">CALLER,>CALLS" --json
```

JSON response:

```json
{
  "node": { "id": 42, "name": "index_file", "kind": "fn", "file": "src/lib.rs", "line": 10 },
  "resolve": [...],
  "edges": [...],
  "callers": [{ "depth": 1, "node": {...} }],
  "callees": [{ "depth": 1, "node": {...} }],
  "chain": [...]
}
```

This JSON contract is the prototype for envoy/atheneum HTTP endpoints.

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

The daemon exposes a JSON-RPC API over a Unix domain socket. The socket path
follows `XDG_RUNTIME_DIR` when the variable is set (e.g.
`/run/user/1000/magellan.sock` under systemd user services); otherwise it falls
back to `/tmp/magellan.sock`.

All messages are newline-delimited JSON. Start the daemon with
`magellan service start` (or `systemctl --user start magellan`) before
connecting.

### Wire Format

**Request** (params are flattened at the top level via `#[serde(flatten)]`):

```json
{ "id": "req-1", "method": "ping" }
{ "id": "req-2", "method": "register", "name": "myproject", "root": "/path" }
```

**Success response:**

```json
{ "id": "req-1", "result": { ... } }
```

**Error response:**

```json
{ "id": "req-1", "error": { "code": -32603, "message": "..." } }
```

Error codes: `-32001` (not implemented), `-32002` (dispatch closed),
`-32003` (meta-db error), `-32005` (project not found),
`-32006` (candidate not found), `-32602` (invalid params),
`-32603` (internal error).

### Project Management Methods

| Method | Params | Returns |
|--------|--------|---------|
| `ping` | *(none)* | `{ "pong": true }` |
| `list` | *(none)* | `{ "projects": ["name1", ...] }` (enabled only) |
| `status` | *(none)* | `{ "projects": [{name, root, db, enabled, source}] }` |
| `register` | `name?`, `root?`, `source?`, `include?`, `exclude?` | `{ "registered": "name" }` |
| `unregister` | `name` | `{ "removed": bool }` |
| `pause` | `name` | `{ "paused": bool }` |
| `resume` | `name` | `{ "resumed": bool }` |
| `watch` | `tag?`, `paths?` | `{ "queued": "tag", "files": N }` |
| `stop` | *(none)* | `{ "stopping": true }` |
| `stats` | *(none)* | `{ "projects": [{name, file_count, symbol_count, last_reindexed}] }` |
| `events` | `project?`, `event_type?`, `since_hours?`, `limit?` | `{ "events": [DaemonEvent] }` |

**`register`** details:

- `name` defaults to `"unnamed"`, `root` defaults to `"."`
- `include` and `exclude` are `string[]` glob patterns for file filtering
- Spawns a filesystem watcher if the daemon has a watcher map
- Stores the project's database at `~/.magellan/<name>/<name>.db`

**`watch`** enqueues files for indexing under a project tag. Use this when
forge needs to trigger indexing of specific files.

**`events`** returns daemon event log entries with fields: `id`, `event_type`,
`project_name`, `file_path`, `details`, `created_at`, `execution_id`.

### Cross-Project Query Methods

| Method | Params | Returns |
|--------|--------|---------|
| `query.find` | `name`, `file?`, `depth?`, `callers?`, `callees?` | `{ "query": "...", "matches": [SymbolMatch] }` |
| `query.context` | `name`, `file?`, `callers?`, `callees?`, `depth?` | Same as find with expanded caller/callee arrays |
| `query.compare` | `name`, `projects: [string]` | `{ "comparisons": [..., similarity_score?] }` |
| `query.suggest` | `from_project`, `name`, `to_project?` | `{ "suggestions": [{project, symbol, file, similarity_score}] }` |
| `query.build-index` | *(none)* | `{ "pairs_inserted": N }` |

`SymbolMatch` fields: `project`, `name`, `kind`, `file_path`, `start_line`,
`start_col`, `end_line`, `end_col`.

`query.build-index` populates `pattern_cross_refs` via pairwise cosine
similarity across all enabled registry projects (threshold 0.70).

### Evolution Loop Methods

| Method | Params | Returns |
|--------|--------|---------|
| `evolve.analyze` | `project?`, `limit?` | `{ "candidates": [HotspotCandidate] }` |
| `evolve.retrieve` | `project`, `symbol`, `to_project?`, `limit?` | `{ "analogues": [...] }` |
| `evolve.propose` | `project`, `symbol`, `patch_diff`, `candidate_id?`, `analogue?` | `{ "candidate_id": "...", "status": "pending" }` |
| `evolve.candidates` | `project`, `status?`, `limit?` | `{ "candidates": [CandidateRecord] }` |
| `evolve.verify` | `project`, `candidate_id` | `{ "status": "verified" \| "rejected", "passed": bool, ... }` |
| `evolve.promote` | `project`, `candidate_id` | `{ "candidate_id": "...", "status": "promoted" }` |
| `evolve.reject` | `project`, `candidate_id`, `rejection_reason?` | `{ "candidate_id": "...", "status": "rejected" }` |

`evolve.verify` creates a temp worktree, applies `patch_diff` via `patch -p0`,
auto-detects the test harness (`cargo test` / `pytest` / `npm test`), runs
tests, and updates candidate status.

Candidate lifecycle: `pending` → `verified` or `rejected` → `promoted`.

### Forge Integration Example

Forge can use the socket API to:

1. **Register projects**: `{"method": "register", "name": "...", "root": "...", "include": ["src/"]}`
2. **Query symbols**: `{"method": "query.find", "name": "parse_args"}`
3. **Get context**: `{"method": "query.context", "name": "parse_args", "callers": true}`
4. **Find hotspots**: `{"method": "evolve.analyze", "project": "magellan"}`
5. **Propose changes**: `{"method": "evolve.propose", "project": "...", "symbol": "...", "patch_diff": "..."}`
6. **Verify changes**: `{"method": "evolve.verify", "project": "...", "candidate_id": "..."}`
7. **Trigger indexing**: `{"method": "watch", "tag": "myproject", "paths": ["/path/to/file.rs"]}`

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
