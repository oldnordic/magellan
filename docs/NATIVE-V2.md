# Native V2 Backend Documentation

**Version:** 2.1.0
**Last Updated:** 2026-02-08

---

## Overview

The Native V2 backend is a high-performance storage alternative to SQLite that uses embedded KV storage for metadata. It provides O(1) lookups for common operations and stores all data in a single file.

### Key Differences from SQLite

| Aspect | SQLite Backend | Native V2 Backend |
|--------|---------------|-------------------|
| Graph storage | SQL tables | Native graph database |
| Metadata storage | SQL tables | KV store (O(1) lookups) |
| Symbol lookup | SQL query | Direct KV key lookup |
| File format | SQLite format 3 | Custom "MAG2" format |
| Algorithm commands | Full support | Planned for future |

### When to Use Native V2

- **Large codebases:** O(1) lookups scale better than SQL queries
- **Frequent symbol queries:** Direct KV access avoids query planning overhead
- **Embedded deployment:** Single file without external SQLite dependency
- **Indexing performance:** KV writes are optimized for write-heavy workloads

---

## KV Storage Architecture

### Key Pattern Namespace

All KV keys use colon-separated namespace prefixes to prevent collisions:

```
{namespace}:{identifier}:{value}
```

### Complete Key Reference

| Key Pattern | Value Type | Purpose |
|-------------|------------|---------|
| `sym:fqn:{fqn}` | `SymbolId` (i64) | O(1) lookup of SymbolId by fully-qualified name |
| `sym:id:{id}` | Symbol metadata | Symbol metadata by ID |
| `sym:rev:{id}` | `Vec<SymbolId>` | Reverse index: symbols referencing this symbol |
| `sym:fqn_of:{id}` | `String` (FQN) | FQN lookup by SymbolId for cache invalidation |
| `file:path:{path}` | `FileId` (u64) | FileId lookup by path |
| `file:sym:{id}` | `Vec<SymbolId>` | All symbols in a file |
| `chunk:{path}:{start}:{end}` | `CodeChunk` JSON | Source code fragment by byte span |
| `ast:file:{id}` | `Vec<AstNode>` | Abstract syntax tree nodes for a file |
| `execlog:{id}` | `ExecutionLog` JSON | Command execution history |
| `metrics:file:{path}` | `FileMetrics` JSON | Complexity, LOC, fan-in/out per file |
| `metrics:symbol:{id}` | `SymbolMetrics` JSON | Complexity per symbol |
| `cfg:func:{id}` | `Vec<CfgBlock>` | Control flow graph blocks |
| `label:{name}` | `KvValue` | Canonical FQN mappings, categories |
| `calls:{caller}:{callee}` | Call metadata | Individual call relationship |
| `calls:from:{caller}:` | Prefix scan | All calls from a symbol |
| `calls:to:{callee}:` | Prefix scan | All calls to a symbol |

### Key Design Principles

1. **Namespace prefixes** prevent key collisions between data types
2. **Colon separation** enables efficient prefix scans
3. **ID-based lookups** provide O(1) access without joins
4. **Prefix patterns** support range queries (e.g., all symbols in a file)

---

## Indexing Behavior

### What Gets Stored Where

When indexing a source file with Native V2 backend:

| Data Type | Storage Location | Key Pattern |
|-----------|------------------|-------------|
| File node | Graph database | N/A (entity) |
| Symbol node | Graph database | N/A (entity) |
| Reference node | Graph database | N/A (entity) |
| Call node | Graph database | N/A (entity) |
| Symbol index | KV store | `sym:fqn:{fqn}` |
| File-to-symbol mapping | KV store | `file:sym:{file_id}` |
| Code chunks | KV store | `chunk:{path}:{start}:{end}` |
| AST nodes | KV store | `ast:file:{file_id}` |
| File metrics | KV store | `metrics:file:{path}` |
| Symbol metrics | KV store | `metrics:symbol:{id}` |
| Call edges | KV store | `calls:*` |

### Indexing Flow

```
1. Parse source file with tree-sitter
2. Extract symbols, references, calls
3. Insert nodes/edges into graph database
4. Populate KV indexes:
   - sym:fqn:* for O(1) symbol lookup
   - file:sym:* for file-level queries
   - chunk:* for code retrieval
   - ast:file:* for AST queries
   - metrics:* for complexity analysis
   - calls:* for call graph
5. Commit transaction (WAL)
```

### Performance Characteristics

| Operation | SQLite Backend | Native V2 Backend |
|-----------|---------------|-------------------|
| Symbol lookup by FQN | SQL query (~1-5ms) | KV get (~0.01-0.1ms) |
| File symbol listing | SQL query (~1-5ms) | KV decode (~0.1-0.5ms) |
| Code chunk retrieval | SQL query (~1-5ms) | KV get (~0.01-0.1ms) |
| AST node query | SQL query (~5-20ms) | KV decode (~0.5-2ms) |
| Batch indexing | Transactional writes | KV writes + WAL |

---

## Query Behavior

All Magellan CLI commands automatically use the appropriate storage backend:

| Command | SQLite Backend | Native V2 Backend |
|---------|---------------|-------------------|
| `find` | SQL query | KV lookup (`sym:fqn:{fqn}`) |
| `query` | SQL query | Graph + KV lookup |
| `files` | SQL query | Graph query |
| `refs` | SQL query | Graph query |
| `chunks` | SQL query | KV prefix scan |
| `chunk-by-span` | SQL query | KV lookup (`chunk:*`) |
| `chunk-by-symbol` | SQL query | KV lookup |
| `get` | SQL query | KV lookup |
| `get-file` | SQL query | KV prefix scan |
| `ast` | SQL query | KV lookup (`ast:file:*`) |
| `find-ast` | SQL query | KV scan |
| `label` | SQL query | KV lookup (`label:*`) |
| `collisions` | SQL query | Graph query |
| `cycles` | SQL query | Not yet supported |
| `dead-code` | SQL query | Not yet supported |
| `reachable` | SQL query | Not yet supported |
| `export` | SQL query | Graph + KV scan |

### Backend Detection

Magellan automatically detects the backend format from the database file header:

```bash
# Check first 4 bytes of database file
hexdump -C -n 4 codegraph.db

# Output:
# 4d 41 47 32  = Native V2 ("MAG2")
# 53 51 4c 69  = SQLite format 3
```

No manual configuration required - all commands work with both backends.

---

## Migration Guide

### Migrating from SQLite to Native V2

```bash
# 1. Export from SQLite database
magellan export --db ./magellan.db > export.json

# 2. Create new Native V2 database
magellan watch --root . --db ./magellan-v2.db --scan-initial

# 3. Verify data migration
magellan status --db ./magellan-v2.db

# 4. Optional: Remove old database
rm ./magellan.db
```

### Data Preservation

All data is preserved during migration:

- Graph entities (File, Symbol, Reference, Call nodes)
- Code chunks (stored as `chunk:*` keys)
- AST nodes (stored as `ast:file:*` keys)
- Metrics (stored as `metrics:*` keys)
- Symbol index (stored as `sym:*` keys)
- Call edges (stored as `calls:*` keys)

### Rollback

To rollback to SQLite:

```bash
# 1. Export from Native V2
magellan export --db ./magellan-v2.db > export.json

# 2. Rebuild with SQLite backend
cargo build --release --no-default-features

# 3. Create new SQLite database
magellan watch --root . --db ./magellan.db --scan-initial
```

---

## Performance Characteristics

### Read Performance

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Symbol lookup | O(1) | Direct KV key lookup |
| File symbols | O(1) | Single KV read + decode |
| Code chunk | O(1) | Direct KV key lookup |
| AST nodes | O(1) | Single KV read + decode |
| Prefix scan | O(n) | n = number of matching keys |

### Write Performance

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Store symbol | O(1) | Graph insert + KV index |
| Store chunk | O(1) | Single KV write |
| Store AST nodes | O(1) | Batch KV write |
| Invalidate file | O(k) | k = symbols in file |

### Scaling

Native V2 backend is designed for:

- **Large codebases:** 100K+ files, 1M+ symbols
- **Frequent queries:** Symbol lookups don't degrade with database size
- **Concurrent access:** WAL-based multi-reader support

---

## Troubleshooting

### Common Issues

#### "Algorithm command not supported"

**Problem:** Commands like `cycles`, `dead-code`, `reachable` return errors.

**Solution:** These commands require SQLite backend. Use SQLite backend for algorithm commands or wait for Native V2 algorithm support in a future release.

#### "Database format not recognized"

**Problem:** Magellan doesn't recognize the database file.

**Solution:** Check the file header with `hexdump -C -n 4 codegraph.db`. If it's not "MAG2" or "SQLite format 3", the file may be corrupted.

#### Slow prefix scans

**Problem:** Queries like `chunks --file pattern` are slow.

**Solution:** Use more specific file paths. Prefix scans are O(n) where n is the number of matching keys.

### Debug Commands

```bash
# Check database format
hexdump -C -n 4 codegraph.db

# Check what's stored in KV (requires inspection tools)
# No direct KV inspection CLI yet - use export commands

# Verify data integrity
magellan verify --root . --db ./codegraph.db
```

---

## Known Limitations

1. **Graph algorithm commands** (`cycles`, `dead-code`, `reachable`, `condense`, `paths`, `slice`) require SQLite backend
2. **No direct SQL access** to KV data - use Magellan CLI commands
3. **No KV inspection CLI** - data access through export commands

---

## Future Enhancements

Planned for future releases:

- Graph algorithm support (cycles, dead-code, reachable, etc.)
- KV inspection CLI command
- Backup/restore utilities
- Compression for large KV values

---

## References

- [MANUAL.md - Backend Compatibility](../MANUAL.md#6-backend-compatibility)
- [README.md - Native V2 Backend](../README.md#native-v2-backend)
- [Phase 55: KV Data Storage Migration](../.planning/phases/55-kv-data-storage-migration/55-CONTEXT.md)
