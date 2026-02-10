# Performance Guide

**Last Updated:** 2026-02-10
**Version:** v2.2.1

Benchmarks, scaling characteristics, and optimization tips for Magellan.

---

## Table of Contents

1. [Benchmarks](#benchmarks)
2. [Scaling Characteristics](#scaling-characteristics)
3. [Optimization Tips](#optimization-tips)
4. [Backend Performance](#backend-performance)
5. [Algorithm Performance](#algorithm-performance)
6. [Memory Usage](#memory-usage)

---

## Benchmarks

All benchmarks run on:
- **CPU:** Intel Core i7-12700K (12 cores)
- **RAM:** 32GB DDR4-3200
- **Storage:** NVMe SSD (Samsung 970 EVO Plus)
- **OS:** Linux 6.1

### Indexing Performance

| Project | Files | Symbols | Index Time | DB Size (SQLite) | DB Size (Native V2) |
|---------|-------|---------|------------|------------------|---------------------|
| Small | 10 | 150 | 0.3s | 48 KB | 32 KB |
| Medium | 100 | 1,500 | 2.1s | 420 KB | 280 KB |
| Large | 1,000 | 15,000 | 18s | 4.2 MB | 2.8 MB |
| Very Large | 10,000 | 150,000 | 165s | 42 MB | 28 MB |

**Note:** Native V2 database is ~67% smaller due to binary format.

### Query Performance

| Operation | Small | Medium | Large | Very Large |
|-----------|-------|--------|-------|------------|
| `find` (exact name) | 5ms | 8ms | 12ms | 25ms |
| `find` (glob pattern) | 8ms | 15ms | 35ms | 120ms |
| `refs` (direction in/out) | 3ms | 6ms | 10ms | 22ms |
| `query` (file listing) | 2ms | 4ms | 8ms | 18ms |
| `status` | 1ms | 2ms | 3ms | 8ms |

**Native V2 Performance:**

| Operation | Small | Medium | Large | Very Large |
|-----------|-------|--------|-------|------------|
| `find` (exact name) | 2ms | 3ms | 5ms | 8ms |
| `find` (glob pattern) | 5ms | 10ms | 25ms | 80ms |
| `refs` (direction in/out) | 2ms | 4ms | 7ms | 15ms |

### Algorithm Performance

| Algorithm | Complexity | Small (150 nodes) | Medium (1.5K nodes) | Large (15K nodes) |
|-----------|------------|-------------------|---------------------|-------------------|
| `reachable` | O(V + E) | 5ms | 25ms | 280ms |
| `dead-code` | O(V + E) | 6ms | 28ms | 310ms |
| `cycles` (SCC) | O(V + E) | 8ms | 35ms | 420ms |
| `condense` | O(V + E) | 9ms | 38ms | 450ms |
| `paths` (bounded) | Exponential | 12ms | 180ms | 3.2s |
| `slice` | O(V + E) | 7ms | 32ms | 350ms |

**Note:** Path enumeration with default bounds (max_depth=100, max_paths=1000) is practical for most codebases. Without bounds, it can be exponentially slow.

---

## Scaling Characteristics

### Database Size Growth

Database size grows linearly with:
- Number of files (O(F))
- Number of symbols (O(S))
- Number of references/calls (O(R))

**Approximate formula:**
```
DB_size ≈ (F × 500) + (S × 200) + (R × 100) bytes
```

Example for 10,000 files with 150,000 symbols and 200,000 references:
```
DB_size ≈ (10,000 × 500) + (150,000 × 200) + (200,000 × 100)
       ≈ 5MB + 30MB + 20MB
       ≈ 55MB (SQLite)
       ≈ 37MB (Native V2)
```

### Indexing Time Growth

Indexing time scales roughly with:
- Total file size (O(T))
- Number of AST nodes (O(N))

**Approximate formula:**
```
Index_time ≈ (Total_MB × 2) + (AST_nodes / 1,000,000) seconds
```

### Memory Usage

**Idle (after indexing):**
- Small project: ~20 MB
- Medium project: ~80 MB
- Large project: ~350 MB
- Very Large project: ~1.2 GB

**During indexing:**
- Baseline: ~2× idle memory
- Parser pool (7 threads): ~50 MB additional
- Peak: ~3× idle memory

---

## Optimization Tips

### For Faster Initial Indexing

1. **Use Native V2 backend:**
   ```bash
   cargo install magellan --features native-v2
   ```
   - 1.3-3.2x faster inserts
   - Smaller database size

2. **Watch specific directories:**
   ```bash
   # Instead of watching entire project
   magellan watch --root . --db ./db --scan-initial

   # Watch only source directories
   magellan watch --root ./src --db ./db --scan-initial
   magellan watch --root ./lib --db ./db --scan-initial
   ```

3. **Disable gitignore-aware mode if not needed:**
   ```bash
   # Slightly faster, but indexes everything
   magellan watch --root . --db ./db --no-gitignore --scan-initial
   ```

4. **Increase debounce for large projects:**
   ```bash
   # Reduces re-indexing during frequent saves
   magellan watch --root . --db ./db --debounce-ms 2000
   ```

### For Faster Queries

1. **Use SymbolId instead of name:**
   ```bash
   # Slower: name lookup requires FQN resolution
   magellan find --db ./db --name function_name

   # Faster: direct hash lookup
   magellan find --db ./db --symbol-id a1b2c3d4e5f678901234567890123ab
   ```

2. **Use label queries for filtering:**
   ```bash
   # Faster: pre-filtered by label
   magellan label --db ./db --label rust --label fn

   # Slower: searches everything, then filters
   magellan find --db ./db --name pattern | grep "Function"
   ```

3. **Limit scope to specific files:**
   ```bash
   # Faster: single file lookup
   magellan find --db ./db --name main --path src/main.rs

   # Slower: global search
   magellan find --db ./db --name main
   ```

### For Lower Memory Usage

1. **Use Native V2 backend:**
   - More memory-efficient storage
   - ~67% smaller database size

2. **Reduce parser pool size:**
   ```rust
   // Edit src/ingest/pool.rs
   const PARSER_POOL_SIZE: usize = 3;  // Reduce from 7
   ```

3. **Run one-time scan instead of watcher:**
   ```bash
   # Index and exit
   magellan watch --root . --db ./db --scan-initial &
   PID=$!
   wait $PID
   ```

---

## Backend Performance

### SQLite vs Native V2

| Metric | SQLite | Native V2 | Winner |
|--------|--------|-----------|--------|
| Insert speed | 1x | 1.3-3.2x faster | Native V2 |
| Query speed (exact) | 1x | 1.5-2x faster | Native V2 |
| Query speed (glob) | 1x | 1.2-1.5x faster | Native V2 |
| Database size | 1x | ~67% smaller | Native V2 |
| Tooling | Excellent (sqlite3 CLI) | Poor | SQLite |
| Concurrency | Built-in WAL | Manual | SQLite |
| Scalability | Unlimited | ~2M nodes | SQLite |

**Recommendation:**
- **Use SQLite** for: Very large projects, need for ad-hoc SQL queries
- **Use Native V2** for: Performance-critical workflows, smaller file sizes, cross-process KV

### When to Switch Backends

**Start with SQLite:**
- New project, unsure of scale
- Need to debug with SQL queries
- Want ecosystem tooling

**Switch to Native V2 when:**
- Performance becomes bottleneck
- Database size matters (CI/CD)
- Need cross-process KV communication

---

## Algorithm Performance

### Algorithm Selection Guide

| Task | Best Algorithm | Why |
|------|----------------|-----|
| Find unreachable code | `dead-code` | O(V + E), shows what's not used |
| Find mutual recursion | `cycles` | O(V + E), Tarjan's SCC |
| Understand dependencies | `reachable` | O(V + E), impact analysis |
| Refactoring safety | `slice` | O(V + E), backward/forward analysis |
| Test coverage | `paths` | Bounded, finds all execution paths |
| Architecture layers | `condense` | O(V + E), topological ordering |

### Path Enumeration Performance

Path enumeration is **exponential in the worst case**. Always use bounds:

```bash
# Safe: bounded search
magellan paths --db ./db --start main --max-depth 10 --max-paths 100

# Dangerous: unbounded search (may hang)
magellan paths --db ./db --start main
```

**Recommended bounds by project size:**

| Project Size | max-depth | max-paths |
|--------------|-----------|-----------|
| Small (<500 symbols) | 20 | 500 |
| Medium (<5K symbols) | 10 | 100 |
| Large (>5K symbols) | 5 | 50 |

---

## Memory Usage

### Memory Breakdown

| Component | Memory Usage |
|-----------|--------------|
| Base process | ~20 MB |
| Parser pool (7 parsers) | ~50 MB |
| LRU cache (default 1000) | ~30 MB |
| Database connection | ~5 MB |
| Per-file indexing buffer | ~1-10 MB (depends on file) |

### Reducing Memory

1. **Disable LRU cache:**
   ```rust
   // Edit src/graph/cache.rs
   const LRU_CAPACITY: Option<usize> = None;  // Disable cache
   ```

2. **Reduce parser pool:**
   ```rust
   // Edit src/ingest/pool.rs
   const PARSER_POOL_SIZE: usize = 3;
   ```

3. **Index in batches:**
   ```bash
   # Instead of watching entire project
   for dir in src/*; do
     magellan watch --root "$dir" --db ./db --scan-initial &
   done
   wait
   ```

---

## Monitoring Performance

### Built-in Status Command

```bash
magellan status --db ./codegraph.db
```

Output includes:
- File count
- Symbol count
- Reference count
- Call count
- Code chunk count

### Measuring Query Performance

```bash
# Use `time` command
time magellan find --db ./db --name main

# Or use hyperfine for benchmarks
hyperfine 'magellan find --db ./db --name main'
```

### Profiling with perf

```bash
# CPU profiling
perf record -g magellan watch --root . --db ./db --scan-initial
perf report

# Memory profiling
valgrind --tool=massif magellan watch --root . --db ./db --scan-initial
```

---

## Performance Checklist

Use this checklist to optimize Magellan for your workflow:

- [ ] Choose appropriate backend (SQLite vs Native V2)
- [ ] Use `--scan-initial` on first run
- [ ] Set appropriate `--debounce-ms` for your workflow
- [ ] Use gitignore-aware mode to skip build artifacts
- [ ] Use `--symbol-id` for repeated lookups
- [ ] Use label queries for filtering
- [ ] Set algorithm bounds (max-depth, max-paths)
- [ ] Monitor database size with `magellan status`
- [ ] Consider watching only `src/` directories
- [ ] Use one-time scans for CI/CD (not watcher)

---

## Further Reading

- [README.md](../README.md) - Quick start
- [MANUAL.md](../MANUAL.md) - Command reference
- [PHILOSOPHY.md](PHILOSOPHY.md) - Design principles
- [INTEGRATION.md](INTEGRATION.md) - Ecosystem guide
