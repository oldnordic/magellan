# Troubleshooting Guide

**Last Updated:** 2026-02-10
**Version:** v2.2.1

Common issues and solutions when using Magellan.

---

## Table of Contents

1. [Installation & Build Issues](#installation--build-issues)
2. [Runtime Issues](#runtime-issues)
3. [Performance Issues](#performance-issues)
4. [Indexing Issues](#indexing-issues)
5. [Query Issues](#query-issues)
6. [Backend-Specific Issues](#backend-specific-issues)
7. [Integration Issues](#integration-issues)
8. [Getting Help](#getting-help)

---

## Installation & Build Issues

### "cargo install fails with linking error"

**Symptom:** Build fails with `error: linking with cc failed`

**Cause:** Missing system dependencies for tree-sitter or SQLite.

**Solution:**

```bash
# Debian/Ubuntu
sudo apt-get install build-essential libsqlite3-dev

# macOS (typically pre-installed)
xcode-select --install

# Arch Linux
sudo pacman -S base-devel sqlite

# Fedora/RHEL
sudo dnf install gcc sqlite-devel
```

### "Feature native-v2 not found"

**Symptom:** Build fails with `error: unused manifest key`

**Cause:** Native V2 is a feature flag, not a separate crate.

**Solution:**

```bash
# Correct installation
cargo install magellan --features native-v2

# Or build from source
cd magellan
cargo build --release --features native-v2
```

### "thread 'main' panicked: 'called `Result::unwrap()` on an `Err` value: ParseIntError'"

**Symptom:** Panic during build or runtime.

**Cause:** Usually incompatible Rust version or corrupted build cache.

**Solution:**

```bash
# Check Rust version
rustc --version  # Should be 1.70 or higher

# Clean build
cargo clean
cargo build --release

# If that fails, update Rust
rustup update
```

---

## Runtime Issues

### "Database is locked"

**Symptom:** Operations fail with database locked error.

**Cause:** Another process is writing to the database.

**Solution:**

```bash
# Check for other Magellan processes
ps aux | grep magellan

# Kill existing watcher
pkill -f "magellan watch"

# Or wait for current operation to finish
```

**Prevention:**

```bash
# Use a single watcher per database
magellan watch --root . --db ./codegraph.db --scan-initial &

# All other operations should use read-only commands
magellan status --db ./codegraph.db
magellan find --db ./codegraph.db --name main
```

### "No such file or directory (os error 2)"

**Symptom:** Database file not found after running commands.

**Cause:** Database path doesn't exist or wasn't created.

**Solution:**

```bash
# Always use --scan-initial for first use
magellan watch --root . --db ./codegraph.db --scan-initial

# Or create database first
magellan watch --root . --db ./codegraph.db --scan-initial &
sleep 5
magellan status --db ./codegraph.db
```

### "Signal received, shutting down"

**Symptom:** Magellan exits when pressing Ctrl+C.

**Cause:** This is expected behavior - graceful shutdown.

**Solution:**

```bash
# Run in background for persistent watching
magellan watch --root . --db ./codegraph.db --scan-initial &

# Check if it's running
ps aux | grep "magellan watch"

# Stop it later
pkill -f "magellan watch"
```

---

## Performance Issues

### "Indexing is too slow"

**Symptom:** Initial scan takes a long time on large codebases.

**Diagnosis:**

```bash
# Check what's being indexed
magellan files --db ./codegraph.db | wc -l

# Check database size
ls -lh ./codegraph.db
```

**Solutions:**

1. **Use gitignore-aware mode (default in v2.1.0):**
   ```bash
   # This is now the default - skips build artifacts
   magellan watch --root . --db ./codegraph.db
   ```

2. **Exclude large directories manually:**
   ```bash
   # Watch specific directories instead of entire project
   magellan watch --root ./src --db ./codegraph.db --scan-initial
   magellan watch --root ./lib --db ./codegraph.db --scan-initial
   ```

3. **Disable watcher for one-time scans:**
   ```bash
   # One-time scan without watcher overhead
   magellan watch --root . --db ./codegraph.db --scan-initial &
   PID=$!
   sleep 30  # Wait for indexing
   kill $PID
   ```

### "Queries are slow"

**Symptom:** `find`, `refs`, or `query` commands take too long.

**Diagnosis:**

```bash
# Check database statistics
magellan status --db ./codegraph.db
```

**Solutions:**

1. **Use Native V2 backend for O(1) lookups:**
   ```bash
   cargo install magellan --features native-v2
   ```

2. **Use more specific queries:**
   ```bash
   # Slower: searches all symbols
   magellan find --db ./codegraph.db --name get

   # Faster: limits to specific file
   magellan find --db ./codegraph.db --name get --path src/handler.rs
   ```

3. **Use label queries for filtering:**
   ```bash
   # Fast: only searches functions
   magellan label --db ./codegraph.db --label fn
   ```

### "High memory usage"

**Symptom:** Magellan process uses lots of RAM.

**Diagnosis:**

```bash
# Check memory usage
ps aux | grep magellan
```

**Solutions:**

1. **Reduce parser pool size** (edit source code):
   ```rust
   // src/ingest/pool.rs
   const PARSER_POOL_SIZE: usize = 3;  // Reduce from 7
   ```

2. **Use Native V2 backend** (more memory-efficient):
   ```bash
   cargo build --release --features native-v2
   ```

---

## Indexing Issues

### "Files not being indexed"

**Symptom:** Watcher runs but `status` shows 0 files.

**Diagnosis:**

```bash
# Check watcher is running
ps aux | grep "magellan watch"

# Check file extensions
find ./watched/dir -name "*.rs"  # Should return files
```

**Solutions:**

1. **Check file extensions are supported:**
   ```bash
   # Supported: .rs, .py, .js, .mjs, .ts, .tsx, .c, .cpp, .cc, .cxx, .hpp, .h, .java
   ```

2. **Use --scan-initial flag:**
   ```bash
   magellan watch --root . --db ./codegraph.db --scan-initial
   ```

3. **Check gitignore if using gitignore-aware mode:**
   ```bash
   # Your files might be ignored
   cat .gitignore

   # Disable gitignore to test
   magellan watch --root . --db ./codegraph.db --no-gitignore --scan-initial
   ```

### "Stale data after file changes"

**Symptom:** Query results don't reflect recent changes.

**Diagnosis:**

```bash
# Verify database state vs filesystem
magellan verify --root . --db ./codegraph.db
```

**Solutions:**

1. **Make sure watcher is running:**
   ```bash
   ps aux | grep "magellan watch"
   ```

2. **Check debounce time:**
   ```bash
   # Default is 500ms - might be too short for large saves
   magellan watch --root . --db ./codegraph.db --debounce-ms 2000
   ```

3. **Trigger manual re-scan:**
   ```bash
   # Stop watcher, re-scan, restart
   pkill -f "magellan watch"
   magellan watch --root . --db ./codegraph.db --scan-initial &
   ```

---

## Query Issues

### "Symbol not found"

**Symptom:** `find` command returns "Symbol not found".

**Diagnosis:**

```bash
# Check if symbol exists
magellan collisions --db ./codegraph.db | grep "symbol_name"
```

**Solutions:**

1. **Use --ambiguous flag for name collisions:**
   ```bash
   magellan find --db ./codegraph.db --ambiguous "my_crate::symbol_name"
   ```

2. **Check file path is correct:**
   ```bash
   magellan files --db ./codegraph.db | grep "file.rs"
   ```

3. **Re-index if needed:**
   ```bash
   magellan verify --root . --db ./codegraph.db
   ```

### "refs returns empty results"

**Symptom:** `refs` command shows no callers or callees.

**Diagnosis:**

```bash
# Check if file is indexed
magellan files --db ./codegraph.db

# Check if calls exist
magellan status --db ./codegraph.db
```

**Solutions:**

1. **Verify call graph was built:**
   ```bash
   # Status should show calls > 0
   magellan status --db ./codegraph.db
   ```

2. **Check symbol has calls:**
   ```bash
   # Use direction out to see what symbol calls
   magellan refs --db ./codegraph.db --name main --path src/main.rs --direction out
   ```

3. **Cross-file calls require both files indexed:**
   ```bash
   # Verify both files are in database
   magellan files --db ./codegraph.db | grep -E "(file_a.rs|file_b.rs)"
   ```

---

## Backend-Specific Issues

### SQLite Backend Issues

#### "SQLITE_CORRUPT: database disk image is malformed"

**Symptom:** SQLite reports corrupted database.

**Cause:** File system corruption, crash during write, or concurrent writes.

**Solutions:**

1. **Try to recover:**
   ```bash
   sqlite3 codegraph.db "PRAGMA integrity_check;"
   ```

2. **Export and re-import:**
   ```bash
   # Export what you can
   magellan export --db codegraph.db > backup.json

   # Create new database
   rm codegraph.db
   magellan watch --root . --db codegraph.db --scan-initial
   ```

#### "SQLITE_BUSY: database is locked"

**Symptom:** Operations fail with database busy error.

**Cause:** Multiple writers trying to access database simultaneously.

**Solution:**

```bash
# Only one writer at a time
# Use multiple readers, single writer
magellan watch --root . --db ./codegraph.db --scan-initial &  # Writer
magellan status --db ./codegraph.db  # Reader - OK
magellan find --db ./codegraph.db --name main  # Reader - OK
```

### Native V2 Backend Issues

#### "WAL file corrupted"

**Symptom:** Database won't open after crash.

**Solution:**

```bash
# Delete WAL file (main file is safe)
rm codegraph.db.wal

# Reopen database (will create new WAL)
magellan status --db ./codegraph.db
```

#### "Algorithm command not working"

**Symptom:** Graph algorithm commands fail on Native V2.

**Solution:**

```bash
# Verify you have v2.2.1 or later (full algorithm parity)
magellan --version

# Rebuild with latest version
cargo install magellan --features native-v2 --force
```

---

## Integration Issues

### "llmgrep can't find symbols"

**Symptom:** llmgrep returns empty results.

**Diagnosis:**

```bash
# Check Magellan database exists
magellan status --db .codemcp/codegraph.db

# Check llmgrep is using same database
llmgrep --db .codemcp/codegraph.db status
```

**Solution:**

```bash
# Ensure same database path
export MAGELLAN_DB=".codemcp/codegraph.db"

# Re-index if needed
magellan watch --root . --db "$MAGELLAN_DB" --scan-initial &
```

### "splice can't find symbol"

**Symptom:** splice returns "Symbol not found" error.

**Diagnosis:**

```bash
# Get correct SymbolId from Magellan
magellan find --db .codemcp/codegraph.db --name symbol_name --output json
```

**Solution:**

```bash
# Use exact SymbolId from Magellan output
SYMBOL_ID=$(magellan find --db .codemcp/codegraph.db --name symbol_name --output json | jq -r '.data.match.symbol_id')

splice rename --symbol "$SYMBOL_ID" --file src/lib.rs --to new_name
```

---

## Getting Help

### Before Asking for Help

1. **Check the version:**
   ```bash
   magellan --version
   ```

2. **Gather diagnostic info:**
   ```bash
   magellan status --db ./codegraph.db
   magellan verify --root . --db ./codegraph.db
   ```

3. **Create a minimal reproduction:**
   ```bash
   # Test on a small project
   mkdir /tmp/test-magellan
   cd /tmp/test-magellan
   echo "fn main() {}" > main.rs
   magellan watch --root . --db ./test.db --scan-initial
   magellan status --db ./test.db
   ```

### Where to Ask

| Channel | Purpose | Response Time |
|---------|---------|---------------|
| [GitHub Issues](https://github.com/oldnordic/magellan/issues) | Bug reports, feature requests | Days to weeks |
| [Documentation](https://github.com/oldnordic/magellan) | Common issues | Immediate |

### Bug Report Template

```markdown
## Bug Description

Brief description of the bug.

## Reproduction Steps

1. Run `magellan watch --root . --db ./db --scan-initial`
2. Call `magellan find --db ./db --name X`
3. Observe error

## Expected Behavior

What should happen.

## Actual Behavior

What actually happens.

## Environment

- Magellan version: x.y.z
- Rust version: ...
- OS: ...
- Backend: SQLite / Native V2

## Error Output

```
Paste full error here
```
```

---

## Further Reading

- [README.md](../README.md) - Quick start
- [MANUAL.md](../MANUAL.md) - Command reference
- [PHILOSOPHY.md](PHILOSOPHY.md) - Design principles
- [INTEGRATION.md](INTEGRATION.md) - Ecosystem guide
