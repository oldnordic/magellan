# Magellan Manual

**Version:** 4.2.1

This manual documents the current user-facing Magellan CLI. The supported normal
workflow uses a SQLite `.db` database.

## Database

Use a `.db` file:

```bash
magellan watch --root . --db .magellan/code.db --scan-initial
```

SQLite is the source-of-truth storage path for current Magellan usage. Older
alternative backend instructions are intentionally absent from this manual
because they are not part of the supported public workflow.

## Project Initialization

### Init

```bash
magellan init [--path <DIR>]
```

Creates a `.magellan.toml` configuration file in the project root with sensible
defaults. If `--path` is omitted, uses the current directory. Detects the
project name from the directory or from `Cargo.toml` if present.

The generated `.magellan.toml`:

```toml
[project]
name = "myproject"

[index]
include = ["src/"]
exclude = []

[watch]
debounce_ms = 500
gitignore_aware = true
scan_initial = true
```

`init` refuses to overwrite an existing `.magellan.toml`.

## Output Formats

Most commands accept:

```bash
--output human
--output json
--output pretty
```

JSON output is wrapped in a stable response envelope:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "hex-timestamp-hex-pid",
  "data": {}
}
```

## Indexing

### Watch A Project

```bash
magellan watch --root <DIR> --db <FILE> [--scan-initial] [--watch-only]
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--scan-initial` | Scan source files before watching |
| `--watch-only` | Watch future changes without an initial scan |
| `--debounce-ms <N>` | Debounce filesystem events |
| `--validate` | Run validation checks |
| `--validate-only` | Validate without indexing |
| `--gitignore-aware` | Honor ignore rules |
| `--no-gitignore` | Disable ignore filtering |

### Index One File

```bash
magellan index --db code.db --file src/lib.rs [--root .]
```

### Delete One File

```bash
magellan delete --db code.db --file src/lib.rs [--root .]
```

### Refresh From Git

```bash
magellan refresh --db code.db [--dry-run] [--include-untracked]
magellan refresh --db code.db [--staged | --unstaged]
magellan refresh --db code.db --force
```

`refresh` uses git status to re-index changed files and remove deleted files.

## Status And Health

### Status

```bash
magellan status --db code.db
magellan status --db code.db --output pretty
```

Status reports file, symbol, reference, call, chunk, and coverage counts.

JSON status always includes:

```json
{
  "coverage": {
    "available": false,
    "covered_blocks": 0,
    "covered_edges": 0
  }
}
```

### Doctor

```bash
magellan doctor --db code.db
magellan doctor --db code.db --fix
```

`doctor` checks database readability, schema state, indexes, and coverage schema
health. `--fix` applies supported repairs.

### Migration

```bash
magellan migrate --db code.db
magellan migrate --db code.db --dry-run
magellan migrate --db code.db --no-backup
```

Current Magellan schema version: `17`.

**Schema v12 changes:** Added FTS5 full-text search index for fast prefix search.
Migration is automatic and creates a backup. See [docs/SCHEMA_SQLITE.md](docs/SCHEMA_SQLITE.md)
for FTS5 performance details and limitations.

## Query Commands

### Symbols In A File

```bash
magellan query --db code.db --file src/main.rs
magellan query --db code.db --file src/main.rs --kind fn
magellan query --db code.db --symbol parse_args --show-extent
```

Rich output flags:

```bash
--with-context
--with-callers
--with-callees
--with-semantics
--with-checksums
--context-lines <N>
```

### Find Symbols

```bash
magellan find --db code.db --name parse_args
magellan find --db code.db --name parse_args --path src/main.rs
magellan find --db code.db --symbol-id <SYMBOL_ID>
magellan find --db code.db --ambiguous parse_args

# Cross-project: search all registered projects
magellan find --all --name parse_args

# Cross-project: search one named project from the registry
magellan find --project magellan --name parse_args
```

### References And Calls

```bash
magellan refs --db code.db --name parse_args --direction in
magellan refs --db code.db --name parse_args --direction out
magellan refs --db code.db --symbol-id <SYMBOL_ID> --direction out
```

### Cross-File References

```bash
magellan cross-file-refs --db code.db --fqn crate::module::symbol
magellan cross-file-refs --db code.db --fqn crate::module::symbol --output pretty
```

### Registry (Cross-Project Discovery)

The registry lives at `~/.config/magellan/registry.toml`. Each entry maps a
project name to a root directory and an optional database path.

```toml
version = "1"

[[project]]
name = "myproject"
root = "/home/user/Projects/myproject"
db = "/home/user/Projects/myproject/.magellan/code.db"   # optional; falls back to ~/.magellan
source = "manual"
enabled = true
```

If the `db` field is omitted, Magellan stores the database at:
```
~/.magellan/<name>/<name>.db
```

Register via CLI:

```bash
# Register a project with an explicit db path
magellan service register --root /path/to/project --name myproject

# Register with the default db location (~/.magellan/myproject/myproject.db)
magellan service register --root /path/to/project

# Register with include/exclude filters (only watch src/ and tests/, skip target/)
magellan service register --root /path/to/project --name myproject \
  --include src/ --include tests/ --exclude target/

# List registered projects
magellan service list

# Remove a project from the registry
magellan service unregister myproject

# Pause / resume a project (disable / re-enable indexing)
magellan service pause myproject
magellan service resume myproject
```

For bulk discovery, the `magellan registry scan` command finds Git repositories:

```bash
magellan registry scan --root /home/feanor/Projects
magellan registry scan --root . --output json

magellan registry list
magellan registry add --name myproject --root /path/to/project
magellan registry remove --name myproject
```

### Natural Language Query (`ask`)

`ask` detects the intent of a question and routes to the right tool automatically.

```bash
# Single project
magellan ask --db code.db "who calls index_file"
magellan ask --db code.db "cfg for parse_watch_args"
magellan ask --db code.db "blast zone of handle_request"
magellan ask --db code.db "cycles in the call graph"
magellan ask --db code.db "find CodeGraph"
magellan ask --db code.db "search for error handling retry logic"

# Cross-project (requires registry to be populated)
magellan ask --all "who calls index_file"
magellan ask --project magellan "find MagellanBackend"
```

Detected intents and their routing:

| Intent keyword | Routes to |
|---------------|-----------|
| `who calls`, `callers of`, `who uses` | `magellan refs --direction in` |
| `callees of`, `calls from`, `outgoing calls` | `magellan refs --direction out` |
| `cfg for`, `control flow of` | `mirage cfg` |
| `blast zone`, `hot paths` | `mirage blast-zone` |
| `cycles`, `circular`, `strongly connected` | in-process cycle detection |
| `impact of`, `affected by` | in-process impact analysis |
| `complex`, `high complexity` | `llmgrep --min-complexity 10` |
| `search`, `semantic`, `find code` | `llmgrep search` |
| *(anything else)* | symbol find |

### Navigate (Grounded Investigation Packet)

`navigate` extracts code terms from a task description and runs a full investigation:
symbol find → callers → callees → impact → affected → context with source.
Optionally invokes `llmgrep` and `mirage` for deeper analysis.

```bash
magellan navigate --db code.db "who calls index_file"
magellan navigate --db code.db "parse_watch_args error handling" --with-mirage
magellan navigate --db code.db "search for retry logic" --with-llmgrep
magellan navigate --db code.db "CodeGraph open_dual sync" --depth 3
magellan navigate --db code.db "handle_request" --concise
```

| Flag | Meaning |
|------|---------|
| `<TASK>` | Natural-language task or question (required) |
| `--depth <N>` | Impact/affected traversal depth (default: 2) |
| `--limit <N>` | Max symbols per extracted term (default: 5) |
| `--concise` | Only top symbol's context (faster, lower token cost) |
| `--with-llmgrep` | Also run semantic search via `llmgrep` |
| `--with-mirage` | Also run CFG analysis via `mirage` for top symbols |

Output is a markdown investigation packet with a token estimate.

### Explore (Stepable Graph Navigation)

`explore` provides stepable graph traversal for interactive and programmatic
navigation of the code graph. It resolves symbols by name or ID, then traverses
edges, callers, callees, and chains.

```bash
# Resolve a symbol and show its node info as JSON
magellan explore --db code.db --symbol "function_name" --json

# Show callers up to depth 3
magellan explore --db code.db --symbol "function_name" --callers --depth 3

# Show callees up to depth 3
magellan explore --db code.db --symbol "function_name" --callees --depth 3

# Show all edges from a specific entity ID
magellan explore --db code.db --id 42 --edges

# Chain traversal through the call graph
magellan explore --db code.db --id 42 --chain ">CALLER,>CALLS"
```

| Flag | Meaning |
|------|---------|
| `--symbol <NAME>` | Resolve symbol by name |
| `--id <ID>` | Use a specific entity ID |
| `--edges` | Show all edges from the resolved entity |
| `--callers` | Show k-hop callers (depth-aware BFS) |
| `--callees` | Show k-hop callees (depth-aware BFS) |
| `--chain <STEPS>` | Chain traversal (e.g. `>CALLER,>CALLS`, `<CALLER`) |
| `--depth <N>` | Traversal depth for callers/callees (default: 1) |
| `--json` / `-j` | Structured JSON output (contract for envoy/atheneum) |

### HopGraph (Embedding-Based Symbol Search)

HopGraph finds symbols by semantic similarity using an HNSW vector index.
Embeddings find entry points (the door), then graph walk retrieves connected
knowledge (the room).

**Disabled by default** — zero overhead when not configured. To enable:

```toml
# ~/.config/magellan/config.toml
[embeddings]
enabled = true
base_url = "http://localhost:11434"
model = "nomic-embed-text"
```

```bash
# Search for symbols by natural language query
magellan hopgraph "parse rust files" --db code.db
magellan hopgraph "error handling" --db code.db --k 10 --output json
```

| Flag | Meaning |
|------|---------|
| `<query>` | Search query (positional argument) |
| `--db <PATH>` | Database path |
| `--k <N>` | Number of results (default: 10) |
| `--output human\|json\|pretty` | Output format |

When `enabled = false` (default), `hopgraph` returns empty results and no HNSW
index is created. The `HashEmbedder` (128-dim structural hash) is used for
testing without ollama; it matches on token overlap in symbol names/FQNs, not
true semantic similarity.

### Configuration

```bash
# Show current configuration
magellan config show
magellan config show --output json

# Initialize default config
magellan config init
```

Config is stored in `~/.config/magellan/config.toml`.

## Service Daemon

The daemon provides a long-running indexer with per-project filesystem watchers
and a JSON-RPC control socket.

**Watcher architecture:** The daemon uses `notify::RecommendedWatcher` (Linux
inotify backend) with custom time-based debouncing. Read-only filesystem events
(ACCESS, OPEN, CLOSE_NOWRITE) are filtered at the source — only write-side
mutations (CREATE, MODIFY, REMOVE) trigger indexing. This prevents the feedback
loop where `reconcile_file_path` reads a file and the resulting ACCESS event
would cause re-indexing.

The debounce window is configurable via `debounce_ms` in `.magellan.toml`
(default: 500 ms). Within the window, paths are collected, deduplicated, and
emitted as a single sorted batch.

Socket path follows the
[XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/)
spec:

- **Default:** `$XDG_RUNTIME_DIR/magellan.sock` (created automatically by
  `magellan service start` and under systemd user services)
- **Fallback:** `/tmp/magellan.sock` when `XDG_RUNTIME_DIR` is unavailable

```bash
# Start background daemon (spreads per-project watchers)
magellan service start

# Or run as a systemd user service (recommended for autostart)
mkdir -p ~/.config/systemd/user
cp /path/to/magellan/systemd/magellan.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now magellan
# Check status
systemctl --user status magellan
```

# Stop the daemon
magellan service stop

# List registered projects
magellan service list

# Register a new project for indexing (auto-assigns name if --name omitted)
magellan service register --root /path/to/project --name myproject

# Register with include/exclude filters
magellan service register --root /path/to/project --name myproject \
  --include src/ --include tests/ --exclude target/

# Remove a project from indexing
magellan service unregister myproject

# Pause / resume indexing for a project
magellan service pause myproject
magellan service resume myproject

# Show daemon status (all projects with metadata)
magellan service status
magellan service stats

# Query daemon event log (audit trail)
magellan service events
magellan service events --project myproject --limit 20
magellan service events --type batch_received --json
magellan service events --since 24
```

### Database Paths

The daemon stores project databases at `~/.magellan/<name>/<name>.db`. This is
the centralized daemon DB path, separate from project-local `.magellan/*.db`
files used by standalone `magellan watch`. Query daemon-indexed symbols with:

```bash
magellan find --db ~/.magellan/myproject/myproject.db --name "symbol_name"
magellan query --db ~/.magellan/myproject/myproject.db --file src/main.rs
```

### Socket API for External Tools

The daemon's JSON-RPC socket is the primary integration point for downstream
tools like forge. See `docs/API_INTEGRATION.md` for the full method reference.
Key methods:

```bash
# Send a request (any Unix socket client works)
echo '{"id":"1","method":"ping"}' | nc -U /run/user/$(id -u)/magellan.sock

# Register a project with include/exclude filters
echo '{"id":"1","method":"register","name":"myproj","root":"/path","include":["src/"]}' \
  | nc -U /run/user/$(id -u)/magellan.sock

# Query symbols across projects
echo '{"id":"1","method":"query.find","name":"parse_args"}' \
  | nc -U /run/user/$(id -u)/magellan.sock

# Trigger indexing of specific files
echo '{"id":"1","method":"watch","tag":"myproj","paths":["/path/to/file.rs"]}' \
  | nc -U /run/user/$(id -u)/magellan.sock
```

## Source Retrieval

```bash
magellan get --db code.db --file src/main.rs --symbol main
magellan get-file --db code.db --file src/main.rs
magellan chunks --db code.db --limit 20
magellan chunk-by-span --db code.db --file src/main.rs --start 0 --end 100
magellan chunk-by-symbol --db code.db --symbol main
```

## AST Queries

```bash
magellan ast --db code.db --file src/main.rs
magellan ast --db code.db --file src/main.rs --position 120
magellan find-ast --db code.db --kind function_item
```

## Labels And Collisions

```bash
magellan label --db code.db --list
magellan label --db code.db --label rust --label fn
magellan label --db code.db --label fn --count

magellan collisions --db code.db
magellan collisions --db code.db --field fqn --limit 20
```

## Graph Algorithms

Algorithm commands use stable symbol IDs.

```bash
magellan reachable --db code.db --symbol <SYMBOL_ID>
magellan reachable --db code.db --symbol <SYMBOL_ID> --reverse

magellan dead-code --db code.db --entry <SYMBOL_ID>
magellan cycles --db code.db
magellan cycles --db code.db --symbol <SYMBOL_ID>
magellan condense --db code.db --members

magellan paths --db code.db --start <SYMBOL_ID> --end <SYMBOL_ID>
magellan paths --db code.db --start <SYMBOL_ID> --max-depth 8 --max-paths 50

magellan slice --db code.db --target <SYMBOL_ID> --direction backward
magellan slice --db code.db --target <SYMBOL_ID> --direction forward --verbose
```

## Context Analysis Commands

Context commands provide symbol-centric context for automated code analysis — definition, callers, callees, impact analysis, and source code snippets.

### Build Context Index

```bash
magellan context build --db code.db
```

Builds the `.magellan/<project>.context.json` summary index. Required once per database before using summary commands.

### Project Summary

```bash
magellan context summary --db code.db
```

Shows project name, version, language, file/symbol counts, and entry points.

### List Symbols (Paginated)

```bash
magellan context list --db code.db
magellan context list --db code.db --kind fn --page 2 --project magellan
magellan context list --db code.db --output json
```

Multi-DB: pass a directory to `--db` and all `.magellan/*.db` files are queried.

### Symbol Detail

```bash
magellan context symbol --db code.db --name parse_args
magellan context symbol --db code.db --name parse_args --callers --callees
magellan context symbol --db code.db --name parse_args --with-source --depth 2
magellan context symbol --db code.db --name parse_args --file src/main.rs --output json
```

Flags:

| Flag | Meaning |
|------|---------|
| `--name <NAME>` | Symbol name to look up (**required**) |
| `--file <PATH>` | Limit search to specific file (optional) |
| `--callers` | Include caller references |
| `--callees` | Include callee references |
| `--with-source` | Include source code snippet |
| `--depth <N>` | Recursive lookup depth (default: 1) |
| `--project <NAME>` | Filter to single project in multi-DB mode |
| `--output <FORMAT>` | `human` (default), `json`, or `pretty` |

### File Context

```bash
magellan context file --db code.db --path src/main.rs
```

Shows symbols in file, language, public symbols, imports.

### Impact Analysis (Blast Radius)

Find all symbols that transitively call the target — "what breaks if I change this?"

```bash
magellan context impact --db code.db --name parse_args --depth 3
magellan context impact --db code.db --name parse_args --file src/main.rs --depth 2 --output json
```

### Affected Analysis (Dependency Reach)

Find all symbols that the target transitively calls — "what does this symbol depend on?"

```bash
magellan context affected --db code.db --name run_main --depth 3
magellan context affected --db code.db --name run_main --output json
```

### Multi-DB Queries

All context commands except `build`, `summary`, and `file` support multi-DB mode:

```bash
magellan context list --db .magellan/ --output json
magellan context symbol --db .magellan/ --name main --callers
magellan context impact --db .magellan/ --symbol extract_symbols --depth 2
```

When `--db` points to a directory, all `.magellan/*.db` files are queried and results are tagged by project name.

## Coverage

```bash
magellan ingest-coverage --db code.db --lcov coverage/lcov.info
magellan status --db code.db --output pretty
```

Coverage is stored in side tables:

- `cfg_block_coverage`
- `cfg_edge_coverage`
- `cfg_coverage_meta`

## Import And Export

```bash
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip --output graph.scip
magellan export --db code.db --format dot --output graph.dot
magellan export --db code.db --format lsif --output graph.lsif

magellan import-lsif --db code.db path/to/index.lsif
```

Export filters:

```bash
--no-symbols
--no-references
--no-calls
--include-collisions
--collisions-field <fqn|display_fqn|canonical_fqn>
--minify
```

## Context And Enrichment

```bash
magellan context build --db code.db
magellan context summary --db code.db
magellan context list --db code.db --kind fn --page-size 50
magellan context symbol --db code.db --name main --callers --callees
magellan context file --db code.db --path src/main.rs

magellan enrich --db code.db
magellan enrich --db code.db --file src/main.rs --timeout 30
```

`enrich` uses available language tools such as rust-analyzer, clangd, or jdtls
when present. Missing tools degrade gracefully.

## External CFG Tools

Default builds use internal parsers. Optional external CFG support can be built:

```bash
cargo build --release --features external-tools-cfg
cargo test --features external-tools-cfg --test external_tools_tests
```

This feature uses installed external tools for C/C++ and Java CFG extraction.

## Source Inventory

Index non-code documents (wiki pages, messages, specs) into the `source_documents` table for cross-referencing with code symbols.

```bash
# Scan a directory for markdown files
magellan source-inventory --db code.db --scan ./wiki markdown

# List indexed documents by kind
magellan source-inventory --db code.db --kind wiki

# Show stale documents (content hash changed)
magellan source-inventory --db code.db --stale

# JSON output
magellan source-inventory --db code.db --output json
```

| Flag | Description |
|------|-------------|
| `--scan <dir> <kind>` | Scan directory for documents of given kind |
| `--kind <kind>` | Filter listed documents by source kind |
| `--stale` | Show documents whose content hash has changed |
| `--output <format>` | Output format: human, json, pretty |

Extracted metadata includes title, author, tags, wikilinks, and frontmatter fields.

## Candidate Facts

Submit and manage structured knowledge triples (subject-predicate-object) linked to source documents.

```bash
# Submit a fact (candidate_id auto-generated if omitted)
magellan candidate-fact submit --db code.db \
  --from-source 1 \
  --subject-type Task --subject-key "impl-feature" \
  --predicate assigned_to \
  --object-type Agent --object-key "claude-1"

# List pending facts
magellan candidate-fact list --db code.db --status pending

# Review queue (rejected + ambiguous facts)
magellan candidate-fact review-queue --db code.db

# Validate a fact against the ontology
magellan candidate-fact validate --db code.db --candidate-id cf_001

# JSON output
magellan candidate-fact list --db code.db --output json
```

| Flag | Description |
|------|-------------|
| `--from-source <id>` | Source document ID (required for submit) |
| `--candidate-id <id>` | Unique identifier (auto-generated if omitted) |
| `--subject-type <type>` | Entity type: Task, Agent, Event, Failure, Module |
| `--subject-key <key>` | Entity identifier |
| `--predicate <rel>` | Relation: assigned_to, caused_by, depends_on, implements, tests |
| `--object-type <type>` | Object entity type (optional) |
| `--object-key <key>` | Object entity identifier (optional) |
| `--status <status>` | Filter by status: pending, accepted, rejected, ambiguous |
| `--limit <n>` | Maximum results |

## Supported Languages

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| TypeScript | `.ts`, `.tsx` |
| Go | `.go` |
| CUDA | `.cu`, `.cuh` |
| HIP | `.hip` (detected as C++) |

Unsupported extensions are ignored during directory scans and return zero
symbols when indexed directly.
