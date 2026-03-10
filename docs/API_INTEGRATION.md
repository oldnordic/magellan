# Magellan API & Integration Guide

**Version:** 3.1.0
**Purpose:** Integration guide for downstream tools using Magellan as a library

---

## Overview

Magellan provides a Rust library API for code indexing and querying. Downstream
tools like llmgrep, Mirage, and Splice use this API to build code intelligence
features.

**Key Design Principles:**
- Backend-agnostic API where possible
- Explicit capability queries for backend-specific features
- Stable symbol IDs for cross-backend references
- Deterministic re-index behavior

---

## Backend Capability Model

### Capability Detection

The `capabilities` module provides runtime capability queries:

```rust
use magellan::capabilities::{
    BackendCapabilities, BackendType, all_capabilities,
    capabilities_for_path, validate_command,
};

// Get all backend capabilities (including disabled)
let all = all_capabilities();

// Get capabilities for a specific backend
let sqlite_caps = BackendCapabilities::for_backend(BackendType::SQLite);
let geo_caps = BackendCapabilities::for_backend(BackendType::Geometric);

// Detect backend from file path
let caps = capabilities_for_path(Path::new("code.geo"));

// Check if a command is supported
let ok = validate_command("paths", &caps);
```

### Capability Structure

```rust
pub struct BackendCapabilities {
    pub backend_type: BackendType,
    pub supports_symbol_queries: bool,
    pub supports_call_graph: bool,
    pub supports_cfg_analysis: bool,
    pub supports_chunks: bool,
    pub supports_cycles: bool,
    pub supports_paths: bool,           // Geometric only
    pub supports_slice: bool,
    pub supports_vacuum_maintenance: bool,
    pub supports_dead_code: bool,
    pub supports_reachability: bool,
    pub supports_export: bool,
    pub supports_ast: bool,             // SQLite, Native V3 only
    pub supports_labels: bool,           // SQLite, Native V3 only
    pub database_extension_hint: String,
    pub format_hint: String,
    pub build_enabled: bool,
    pub required_feature: Option<String>,
}
```

### Backend Type Detection

```rust
pub enum BackendType {
    SQLite,     // .db files (default)
    Geometric,  // .geo files (requires geometric-backend)
    NativeV3,   // .v3 files (requires native-v3)
}

impl BackendType {
    pub fn extension(&self) -> &'static str;
    pub fn display_name(&self) -> &'static str;
    pub fn from_extension(ext: Option<&str>) -> Option<Self>;
}
```

**Detection rules:**
- `.db` → SQLite
- `.geo` → Geometric (if built) or None
- `.v3` → Native V3
- Unknown/None → SQLite (default)

---

## Backend Detection and Open Rules

### Unified Backend Interface

The `Backend` trait provides a unified interface for all backends:

```rust
use magellan::graph::backend::Backend;

pub trait Backend {
    // Statistics
    fn get_stats(&self) -> Result<BackendStats>;

    // Export
    fn export_json(&self) -> Result<String>;
    fn export_jsonl(&self) -> Result<String>;
    fn export_csv(&self) -> Result<String>;

    // Symbol queries
    fn find_symbol_by_fqn(&self, fqn: &str) -> Option<SymbolInfo>;
    fn find_symbols_by_name(&self, name: &str) -> Vec<SymbolInfo>;
    fn search_symbols(&self, pattern: &str) -> Vec<SymbolInfo>;
    fn get_all_symbols(&self) -> Result<Vec<SymbolInfo>>;

    // Call graph
    fn get_callees(&self, symbol_id: u64) -> Vec<u64>;
    fn get_callers(&self, symbol_id: u64) -> Vec<u64>;
    fn get_references_bidirectional(&self, symbol_id: u64)
        -> Result<(Vec<u64>, Vec<u64>)>;

    // Analysis
    fn find_dead_code(&self, entry_id: u64) -> Result<Vec<u64>>;
    fn find_dead_code_multiple_entries(&self, entry_ids: &[u64])
        -> Result<Vec<u64>>;

    // Mutation
    fn insert_symbol(&self, symbol: &SymbolData) -> Result<u64>;
    fn insert_reference(&self, caller_id: u64, callee_id: u64) -> Result<()>;
}
```

### Opening Databases

**SQLite (CodeGraph):**

```rust
use magellan::CodeGraph;

// Open or create SQLite database
let graph = CodeGraph::open(Path::new("code.db"))?;

// Index a file
graph.index_file(path, &source)?;

// Query symbols
let symbols = graph.symbols_in_file(path)?;
```

**Geometric (MagellanBackend):**

```rust
use magellan::backend_router::MagellanBackend;

// Create or open geometric database
let backend = MagellanBackend::create(Path::new("code.geo"))?;
let backend = MagellanBackend::open(Path::new("code.geo"))?;

// Index a file
backend.reconcile_file_path(path)?;

// Query symbols
let symbols = backend.symbols_in_file(path)?;
```

**Auto-detection:**

```rust
use magellan::capabilities::BackendType;

let path = Path::new("code.db");
let ext = path.extension().and_then(|e| e.to_str());
let backend_type = BackendType::from_extension(ext)
    .unwrap_or(BackendType::SQLite);

match backend_type {
    BackendType::SQLite => {
        let graph = CodeGraph::open(path)?;
        // use graph
    }
    BackendType::Geometric => {
        let backend = MagellanBackend::open(path)?;
        // use backend
    }
    BackendType::NativeV3 => {
        // Similar to SQLite but uses native backend
    }
}
```

---

## Query Surface Expectations

### Symbol Queries

**Find by name:**

```rust
// SQLite
let symbols = graph.symbols_by_name("main")?;

// Geometric
let symbols = backend.find_symbols_by_name("main");
```

**Returns:** All symbols matching the name (no silent deduplication)

**Find by FQN:**

```rust
// SQLite
let symbol = graph.symbol_by_fqn("crate::main")?;

// Geometric
let symbol = backend.find_symbol_by_fqn("crate::main");
```

**Returns:** Single symbol or None

**Symbols in file:**

```rust
// SQLite
let symbols = graph.symbols_in_file("src/main.rs")?;

// Geometric
let symbols = backend.symbols_in_file("src/main.rs")?;
```

**Returns:** All symbols defined in the file

### Call Graph Queries

**Get callees (what this calls):**

```rust
// Both backends
let callees = backend.get_callees(symbol_id);
```

**Returns:** Vec<symbol_id>

**Get callers (what calls this):**

```rust
// Both backends
let callers = backend.get_callers(symbol_id);
```

**Returns:** Vec<symbol_id>

### CFG Queries

**Get CFG blocks:**

```rust
// SQLite
let blocks = graph.cfg_blocks_for_function(function_id)?;

// Geometric
let blocks = backend.cfg_blocks_for_function(function_id);
```

**Returns:** Vec<CfgBlock>

**Get CFG edges:**

```rust
// SQLite
let edges = graph.cfg_edges_for_function(function_id)?;

// Geometric
let edges = backend.cfg_edges_for_function(function_id);
```

**Returns:** Vec<CfgEdge>

---

## Backend-Specific Behavior

### Paths Command (Geometric Only)

```rust
// Only available on Geometric backend
if caps.supports_paths {
    let paths = backend.find_paths(
        function_id,
        start_block_id,
        goal_block_id
    )?;
}
```

**Always check capability first:**

```rust
if !caps.supports_paths {
    return Err(anyhow!("Paths not supported by this backend"));
}
```

### AST Queries (SQLite, Native V3 Only)

```rust
// Only available on SQLite and Native V3
if caps.supports_ast {
    let ast_nodes = graph.ast_nodes_in_file(path)?;
}
```

### Label Queries (SQLite, Native V3 Only)

```rust
// Only available on SQLite and Native V3
if caps.supports_labels {
    let labels = graph.labels_for_symbol(symbol_id)?;
}
```

---

## Stable Symbol IDs

### Symbol ID Format

Symbol IDs are SHA-256 hashes of `language:fqn:span_id`:

```rust
use magellan::graph::symbols::generate_symbol_id;

let symbol_id = generate_symbol_id(
    "rust",
    "crate::main",
    "1234-5678"
);
```

**Properties:**
- Deterministic: Same symbol → same ID
- Collision-resistant: Different symbols → different IDs (practically)
- Backend-independent: Same ID across SQLite/Geometric

### Using Symbol IDs

```rust
// Symbol ID is stable across re-index
let id = symbol.symbol_id.unwrap();

// Use for cross-backend references
let reference = CrossFileRef {
    from_symbol_id: caller_id.clone(),
    to_symbol_id: callee_id.clone(),
    // ...
};
```

---

## Re-Index Semantics

### Reconcile Operation

**SQLite:**

```rust
use magellan::graph::ops::ReconcileOutcome;

let outcome = graph.reconcile_file_path(path, path_key)?;

match outcome {
    ReconcileOutcome::Deleted => { /* file removed */ }
    ReconcileOutcome::Unchanged => { /* no changes */ }
    ReconcileOutcome::Reindexed { symbols, references, calls } => {
        println!("Reindexed {} symbols", symbols);
    }
}
```

**Geometric:**

```rust
use magellan::graph::geo_index::GeoReconcileOutcome;

let outcome = backend.reconcile_file_path(path)?;

match outcome {
    GeoReconcileOutcome::Deleted => { /* file removed */ }
    GeoReconcileOutcome::Unchanged => { /* no changes */ }
    GeoReconcileOutcome::Reindexed { symbols } => {
        println!("Reindexed {} symbols", symbols);
    }
}
```

### Idempotence

Both backends guarantee idempotence:

```rust
// Multiple reconciles on unchanged file
assert!(matches!(
    graph.reconcile_file_path(path, path_key)?,
    ReconcileOutcome::Unchanged
));
assert!(matches!(
    graph.reconcile_file_path(path, path_key)?,
    ReconcileOutcome::Unchanged
));
```

---

## Error Handling

### Backend Not Built

```rust
use magellan::capabilities::CommandValidationError;

match validate_command("paths", &caps) {
    Err(CommandValidationError::BackendNotBuilt { backend, feature }) => {
        eprintln!("{} backend not built. Rebuild with --features {}", backend, feature);
    }
    Err(e) => return Err(e.into()),
    Ok(()) => { /* command supported */ }
}
```

### Command Not Supported

```rust
match validate_command("ast", &geo_caps) {
    Err(CommandValidationError::UnsupportedBackend { command, backend, .. }) => {
        eprintln!("Command '{}' not supported by {} backend", command, backend);
    }
    Err(e) => return Err(e.into()),
    Ok(()) => { /* command supported */ }
}
```

---

## Integration Patterns

### Pattern 1: Universal Symbol Query

```rust
use magellan::capabilities::capabilities_for_path;

fn query_symbols(db_path: &Path, name: &str) -> Result<Vec<SymbolInfo>> {
    let caps = capabilities_for_path(db_path);

    if !caps.supports_symbol_queries {
        return Err(anyhow!("Symbol queries not supported"));
    }

    let backend = open_backend(db_path)?;
    Ok(backend.find_symbols_by_name(name))
}
```

### Pattern 2: Capability-Gated Features

```rust
fn find_paths(db_path: &Path, function_id: u64) -> Result<Vec<Path>> {
    let caps = capabilities_for_path(db_path);

    if !caps.supports_paths {
        return Err(anyhow!("Path enumeration requires Geometric backend"));
    }

    let backend = MagellanBackend::open(db_path)?;
    Ok(backend.find_paths(function_id)?)
}
```

### Pattern 3: Fallback Behavior

```rust
fn get_ast_nodes(db_path: &Path, file: &str) -> Result<Vec<AstNode>> {
    let caps = capabilities_for_path(db_path);

    if caps.supports_ast {
        let graph = CodeGraph::open(db_path)?;
        Ok(graph.ast_nodes_in_file(file)?)
    } else {
        // Fallback: Parse with tree-sitter directly
        parse_ast_fallback(file)
    }
}
```

---

## Testing Integration

### Capability Testing

```rust
#[test]
fn test_backend_capabilities() {
    let sqlite_caps = BackendCapabilities::for_backend(BackendType::SQLite);
    assert!(sqlite_caps.supports_symbol_queries);
    assert!(sqlite_caps.supports_ast);

    let geo_caps = BackendCapabilities::for_backend(BackendType::Geometric);
    assert!(geo_caps.supports_paths);
    assert!(!geo_caps.supports_ast); // Not implemented
}
```

### Backend-Agnostic Testing

```rust
#[test]
fn test_symbol_query() {
    for backend_type in BackendCapabilities::enabled_backends() {
        let caps = BackendCapabilities::for_backend(backend_type);
        if !caps.supports_symbol_queries {
            continue;
        }

        let db = create_test_db(backend_type)?;
        let symbols = db.find_symbols_by_name("test")?;
        assert!(!symbols.is_empty());
    }
}
```

---

## See Also

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture
- [SCHEMA_SQLITE.md](SCHEMA_SQLITE.md) - SQLite schema
- [SCHEMA_GEOMETRIC.md](SCHEMA_GEOMETRIC.md) - Geometric schema
- [INVARIANTS.md](INVARIANTS.md) - Database invariants
