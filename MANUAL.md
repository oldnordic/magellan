# Magellan Manual

**Version:** 4.11.1

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

Current Magellan schema version: `18`.

**Schema v12 changes:** Added FTS5 full-text search index for fast prefix search.
Migration is automatic and creates a backup. See [docs/SCHEMA_SQLITE.md](docs/SCHEMA_SQLITE.md)
for FTS5 performance details and limitations.

**Schema v18 changes:** Added repository snapshot tables and temporal query support for commit-history analysis in the same SQLite database.

## Orient — Codebase Snapshot

`orient` prints a single-screen orientation snapshot useful when starting work on an unfamiliar codebase or after a long break.

```bash
magellan orient --db code.db
magellan orient --db code.db --repo .
magellan orient --db code.db --repo . --top 15
magellan orient --db code.db --repo . --output json
```

Output sections:

| Section | Content |
|---------|---------|
| DB | Symbol count, call-edge count, indexed file count |
| Temporal | Snapshot coverage range (first..last commit). Absent if `temporal-sweep` has not been run — prints a hint instead. |
| Top churn | Symbols present in the most commit snapshots, ranked descending. Requires temporal data. |
| Contributors | Commit counts per author from `git log`. Requires `--repo`. |

`--top N` controls how many churn symbols and contributors to show (default 10).

`--repo` is optional. Omitting it skips the Contributors section. `--db` is required.

## Temporal Commands

### Sweep Repository History

```bash
magellan temporal-sweep --db code.db --repo .
magellan temporal-sweep --db code.db --repo . --every 10
magellan temporal-sweep --db code.db --repo . --tags-only
magellan temporal-sweep --db code.db --repo . --merge-commits-only
magellan temporal-sweep --db code.db --repo . --since 1718841600 --until 1718928000
```

`temporal-sweep` ingests sampled commits through detached temporary worktrees and persists snapshot, file, symbol, and edge history.

### Inspect Temporal State

```bash
magellan temporal-status --db code.db
magellan as-of --db code.db --commit <oid> --symbol parse_args
magellan temporal-barcode --db code.db --symbol <stable-id>
magellan temporal-barcode --db code.db --edge-source <stable-id> --edge-target <stable-id> --kind CALLS
magellan temporal-barcode --db code.db --scc
```

- `temporal-status` reports snapshot and version-table counts
- `as-of` resolves symbol matches in a specific stored commit snapshot
- `temporal-barcode` reports symbol, edge, or SCC lifetime across snapshots. `--symbol` accepts either a symbol name (e.g. `parse_args`) or a raw stable ID hash — name resolution is automatic.

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

To see every database magellan knows about, use `magellan catalog`. It reads
the canonical registry and reports a live table of status, entity/edge counts,
and stored entity kinds:

```bash
magellan catalog

# Example output:
#   magellan catalog — 25 databases (21 live, 4 stale)
#
#   NAME        STATUS   ENTITY   EDGE   KINDS
#   envoy       live     3027     3123   Call,Reference,Symbol,Import,File
#   magellan    live     19436    19724  Call,Reference,Symbol,Import,File
#   ...
```

The catalog is read-only and self-contained — it inspects only magellan's own
registry (`~/.magellan/meta.db`) and databases. Use `magellan service
register` / `magellan service unregister` (above) to change which projects are
indexed.

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

### HopGraph (Semantic Symbol Search)

HopGraph lets you search your codebase using plain English instead of exact
symbol names. Unlike `magellan find`, which requires knowing (or guessing) a
function or type name, HopGraph understands the *meaning* of your query and
returns the most relevant symbols — even if their names are completely different
from what you typed.

**How it works (briefly):** Magellan converts every indexed symbol into a
numerical vector (an "embedding") using a language model. When you run a
HopGraph query, your search text is converted to the same kind of vector, and
Magellan finds the closest matches. Think of it like a search engine for code:
you describe what you want, and it finds it.

**New in v4.6.0:** Results now show real symbol names, file paths, and line
numbers (previously they showed raw numeric IDs). You can also use `--hops N`
to expand results through the call/reference graph — for example, if a query
matches a function, `--hops 1` also surfaces functions that call it or types it
references.

#### Step 1: Enable embeddings (one-time setup)

HopGraph is **disabled by default** because it needs a language model to
generate embeddings. You need an embedding model running locally via
[Ollama](https://ollama.com) (recommended) or any OpenAI-compatible API.

**Install and run the embedding model:**

```bash
# Install Ollama (if not already installed)
# See https://ollama.com for platform-specific instructions

# Download the embedding model (nomic-embed-text is small and fast, ~274MB)
ollama pull nomic-embed-text

# Verify it's running
ollama list
```

**Configure Magellan to use it:**

Create or edit `~/.config/magellan/config.toml`:

```toml
[embeddings]
enabled = true
base_url = "http://localhost:11434"
model = "nomic-embed-text"
```

| Setting | What it means |
|---------|--------------|
| `enabled = true` | Turns on embedding generation. Without this, `hopgraph` returns nothing. |
| `base_url` | Where Ollama (or your embedding server) is listening. Default Ollama port is 11434. |
| `model` | Which embedding model to use. `nomic-embed-text` is the recommended default. |

**Alternative embedding providers:** You can use any OpenAI-compatible
embedding API. Set `base_url` to the API endpoint and `model` to the model
name. The provider must return embeddings in the OpenAI format.

#### Step 2: Build the search index

After enabling embeddings, you need to build the HNSW index (the data structure
that makes fast vector search possible). This reads all symbols from your
database and generates an embedding for each one.

```bash
# Build the index (do this after indexing your codebase with 'watch' or 'index')
magellan embed --db code.db

# If you already have an index and want to rebuild it from scratch
magellan embed --db code.db --force

# Control batch size (default is 16 texts per HTTP request)
magellan embed --db code.db --batch-size 32

# Parallel requests (default is 4; should match OLLAMA_NUM_PARALLEL on server)
magellan embed --db code.db --num-parallel 8
```

**How long does this take?** For a codebase with ~3,000 symbols using
`nomic-embed-text` on a modern machine with parallel embedding enabled,
expect 30-90 seconds. The model must process each symbol's name, kind, and
source code to produce its embedding. The default concurrency (4 parallel
requests with 16 texts each) gives ~64 texts in flight at once.

**Parallel embedding:** By default, magellan sends 4 concurrent HTTP requests
to your embedding provider. Each request carries up to 16 texts (configurable
via `--batch-size`). If you're using Ollama, set `OLLAMA_NUM_PARALLEL` to
match or exceed your `--num-parallel` value so Ollama can handle all requests
without queuing.

**When to re-run:** You only need to re-embed when symbols change significantly
(new functions, renamed types, etc.). Running `magellan embed` again will add
new symbols to the existing index without rebuilding from scratch.

#### Step 3: Search for symbols

```bash
# Basic search — describe what you're looking for in plain English
magellan hopgraph "parse command line arguments" --db code.db
magellan hopgraph "error handling and recovery" --db code.db
magellan hopgraph "database connection pool" --db code.db
```

**What you'll see:**

```text
HopGraph results for 'parse command line arguments':
  #1: Symbol parse_watch_args [src/cli/parsers/watch.rs:42] score=0.142
  #2: Symbol parse_hopgraph_args [src/cli/parsers/semantic.rs:855] score=0.156
  #3: Symbol route_search [src/ask_cmd.rs:283] score=0.165
3 result(s)
```

Each result shows:
- **Rank** (#1, #2, ...) — sorted by relevance (lower score = better match)
- **Kind** — what kind of symbol this is (Symbol = function/type/struct, Reference = a reference to one)
- **Name** — the symbol's actual name in your code
- **File and line** — where to find it, shortened to the `src/...` path
- **Score** — how close the match is (lower is better; this is cosine distance, not similarity)

#### Step 4: Keep embeddings fresh

`magellan watch` updates the graph structure automatically when files change,
but it **does not update embeddings**. After significant coding, your HopGraph
results may be stale. HopGraph will warn you when this happens:

```text
⚠️  Embeddings are ~5 minutes stale. Run `magellan embed --db code.db` to refresh.
```

To keep semantic search accurate, re-embed after major changes:

```bash
# Incremental: skips symbols already embedded (fast)
magellan embed --db code.db

# Force full re-embed (slow, use after large refactors)
magellan embed --db code.db --force
```

#### Step 5: Expand results through the graph (--hops)

Sometimes the best match isn't the function itself, but something connected to
it. `--hops N` expands your results by following the reference graph — the same
call/reference relationships that `magellan refs` uses.

**What `--hops` does:** After finding the top vector matches, Magellan follows
the REFERENCES edges in the graph for each match, up to N steps away. This
surfaces symbols that are *structurally connected* to your search results even
if they aren't semantically similar to your query text.

```bash
# Search + expand one hop (finds direct callers and referenced types)
magellan hopgraph "embedding search function" --db code.db --k 3 --hops 1
```

**Output with expansion:**

```text
HopGraph results for 'embedding search function': (1 hop expansion)
  #1: Symbol test_detect_intent_search [src/ask_cmd.rs:361] score=0.141
  #2: Symbol route_search [src/ask_cmd.rs:283] score=0.149
  #3: Symbol extract_quoted_symbol [src/ask_cmd.rs:162] score=0.165
  #4: Symbol route_complex [src/ask_cmd.rs:263] score=0.166
  #5: Symbol detect_intent [src/ask_cmd.rs:22] score=0.167
  #6: Symbol Intent [src/ask_cmd.rs:9] score=0.168
  #7: Reference ref to extract_quoted_symbol [src/ask_cmd.rs:387] hop=1 score=0.266
  #8: Reference ref to extract_quoted_symbol [src/ask_cmd.rs:394] hop=1 score=0.266
8 result(s)
```

Notice what happened:
- Results #1-#6 are **vector matches** — symbols whose embeddings are closest to
  "embedding search function". These are the direct semantic matches.
- Results #7-#8 are **graph-expanded** — they reference `extract_quoted_symbol`
  (result #3) through the reference graph. They're marked `hop=1` meaning they
  are one graph step away from a vector match.
- Graph-expanded results always appear **after** vector results (higher score).

**When to use --hops:**
- `--hops 0` (default): Just vector search. Use when you want the closest
  semantic matches and nothing else.
- `--hops 1`: Adds direct neighbors (callers, referenced types). Good for
  understanding the context around your search results.
- `--hops 2`: Adds neighbors-of-neighbors. Useful for finding broader patterns
  but results can get noisy.

**How scoring works for expanded results:** Vector matches keep their raw
similarity score. Graph-expanded results get a blended score that is always
higher (worse) than any vector match, so you never lose the direct matches:

```text
graph_proximity = 1.0 / (1.0 + hop_distance)
blended_score   = 0.7 × vector_score + 0.3 × (1.0 − graph_proximity)
```

The 0.7/0.3 split means vector similarity matters more, but graph structure
still influences ranking. The `(1.0 − graph_proximity)` term ensures that
farther-away symbols get worse scores.

#### Command reference

| Flag | Default | Description |
|------|---------|-------------|
| `<query>` | (required) | Natural language search text. Describe what you're looking for. |
| `--db <PATH>` | (required) | Path to your Magellan database. |
| `--k <N>` | 10 | How many vector results to find. When `--hops > 0`, total results can exceed `k`. |
| `--hops <N>` | 0 | Graph expansion depth. 0 = off, 1 = direct neighbors, 2 = neighbors of neighbors. |
| `--output human\|json\|pretty` | human | Output format. `json` is useful for piping to `jq` or scripts. |
| `--tokens <N>` | 0 (unlimited) | Limit output to ~N tokens (chars/4 heuristic). Preserves symbol names, truncates context first. `0` or absent = no limit. JSON includes `tokens_estimated` and `truncated` metadata fields. |

#### JSON output

Use `--output json` for scripting or piping to other tools:

```bash
magellan hopgraph "error handling" --db code.db --k 5 --output json | jq '.[0].name'
```

JSON structure:

```json
[
  {
    "rank": 1,
    "entity_id": 165170,
    "score": 0.140667,
    "name": "parse_watch_args",
    "kind": "Symbol",
    "file_path": "/path/to/src/cli/parsers/watch.rs",
    "start_line": 42
  },
  {
    "rank": 7,
    "entity_id": 165200,
    "score": 0.265536,
    "name": "ref to extract_quoted_symbol",
    "kind": "Reference",
    "file_path": "/path/to/src/ask_cmd.rs",
    "start_line": 387,
    "hop_distance": 1
  }
]
```

Fields:
- `rank`: Position in sorted results (1-based).
- `entity_id`: Internal Magellan ID (stable across sessions).
- `score`: Match quality (lower = better).
- `name`: The symbol's name in source code.
- `kind`: `Symbol` for definitions, `Reference` for references.
- `file_path`: Absolute path to the source file.
- `start_line`: Line number where the symbol starts.
- `hop_distance`: Only present for graph-expanded results (> 0). Absent for
  direct vector matches (0).

#### Troubleshooting

| Problem | Cause | Fix |
|---------|-------|-----|
| "0 result(s)" every time | Embeddings not enabled | Set `enabled = true` in config and run `magellan embed --db code.db` |
| "0 result(s)" after enabling | Index not built yet | Run `magellan embed --db code.db` (requires indexed codebase first) |
| Results seem wrong / off-topic | Wrong or missing model | Check `ollama list` shows `nomic-embed-text`. Check config `model` matches. |
| Very slow queries | Large codebase, cold cache | First query loads the HNSW index into memory; subsequent queries are fast. |
| Stale results after renaming symbols | Index not rebuilt | Run `magellan embed --db code.db --force` to rebuild from scratch. |
| `--hops` returns no extra results | All neighbors already in vector results | Try smaller `--k` or use `--hops 2` for deeper expansion. |
| No `hop_distance` in JSON output | All results are vector matches (hop=0) | The field is intentionally omitted when 0. Try a smaller `--k` to see expanded results. |

#### Testing without an embedding model

If you don't have Ollama or an embedding server configured, Magellan includes a
built-in `HashEmbedder` that uses a 128-dimensional structural hash. It matches
on token overlap in symbol names and fully-qualified names — not true semantic
similarity, but enough for basic testing:

```bash
# This works without any embedding server but gives approximate results
magellan hopgraph "main function" --db code.db
```

For production use, configure a real embedding model as described in Step 1.

### Configuration

```bash
# Show current configuration
magellan config show
magellan config show --output json

# Initialize default config
magellan config init
```

Config is stored in `~/.config/magellan/config.toml`.

Example with cross-tool integrations (all opt-in):

```toml
[language-model]
provider = "ollama"
base_url = "http://localhost:11434"
model = "codellama"

[registry]
auto_scan = true
scan_roots = ["/home/feanor/Projects"]

[embeddings]
enabled = true
base_url = "http://localhost:11434"
model = "nomic-embed-text"

[integrations]
# Cross-tool integration is opt-in; magellan works standalone by default.
[integrations.atheneum]
enabled = false
db = "~/.local/share/atheneum/atheneum.db"
meta_db = "~/.local/share/atheneum/meta.db"

[integrations.envoy]
enabled = false
url = "http://localhost:9876"

auto_export_discoveries = false
```

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
# Standard export formats
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip --output graph.scip
magellan export --db code.db --format dot --output graph.dot
magellan export --db code.db --format lsif --output graph.lsif

# Impact export (requires --symbol parameter)
magellan export --db code.db --format impact --symbol "function_name" [--output impact.json]

# Import external indexes
magellan import-lsif --db code.db path/to/index.lsif
```

**Repo-root export convention:** When no `--output` is specified and magellan is run from within a git repository, exports automatically write to the `.magellan/` directory in the repository root:

| Format | Output file |
|--------|-------------|
| `json` | `.magellan/export.json` |
| `jsonl` | `.magellan/export.jsonl` |
| `impact` | `.magellan/impact.json` |

When not in a git repository, exports fall back to stdout.

Export filters:

```bash
--no-symbols
--no-references
--no-calls
--include-collisions
--collisions-field <fqn|display_fqn|canonical_fqn>
--minify
```

## Blast Score — Single-Symbol Impact Analysis

Compute blast radius for a single symbol using codeindex-style scoring (direct + 0.5×transitive connections).

```bash
magellan blast-score --db code.db --symbol "function_name" [--file path/to/file.rs] [--depth 10]
```

**Output format:** Single-line text classification:
```
Blast Score: 8.5 (2 direct · 7 transitive) [HIGH]
```

**Classification thresholds:**
- `LOW`: score < 5
- `MEDIUM`: score ≥ 5 and ≤ 10  
- `HIGH`: score > 10

**Options:**
| Flag | Description |
|------|-------------|
| `--symbol <name>` | Target symbol name (required) |
| `--file <path>` | Disambiguate symbols by file path (optional) |
| `--depth <N>` | BFS traversal depth (default: 10) |
| `--output <format>` | Output format: human, json, pretty |

## Pre-commit Hooks — Blast Score Checking

Install git pre-commit hooks that automatically check blast scores for changed symbols before commits.

```bash
# Install hook with default threshold (10.0)
magellan install-hook --db code.db --threshold 10.0

# Install with strict mode (block commits exceeding threshold)
magellan install-hook --db code.db --threshold 10.0 --strict
```

**Hook behavior:**
- Runs on every `git commit`
- Extracts symbols from staged files
- Computes blast scores for changed symbols
- Warns or blocks commits based on configuration
- Requires magellan to be installed and available in PATH

**Options:**
| Flag | Description |
|------|-------------|
| `--threshold <N>` | Blast score threshold (default: 10.0) |
| `--strict` | Block commits exceeding threshold (default: warn only) |

The hook script is installed at `.git/hooks/pre-commit` in the repository root.

## Context And Enrichment

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

C/C++ and Java CFG extraction uses installed external tools detected at runtime.
No special build flags or recompilation is required.

**C/C++ (clang):** When `clang` is on `PATH`, magellan emits LLVM IR during
indexing and extracts per-function CFG basic blocks and edges directly from the
compiler's intermediate representation. This gives more accurate results than
the tree-sitter fallback. When clang is absent, tree-sitter CFG approximations
are used transparently.

**Java (javac):** When `javac` is on `PATH`, magellan compiles `.java` source
files to bytecode and extracts CFG from the `.class` output. Falls back to
tree-sitter when javac is absent.

**compile_commands.json:** C/C++ projects that require project-specific compiler
flags (defines, include paths, `-std=` flags) can expose those flags via the
`CodeGraph::set_compile_commands` API. Flags from the JSON file are forwarded
to clang when emitting LLVM IR, enabling accurate indexing of large projects
such as the Linux kernel or LLVM itself.

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
