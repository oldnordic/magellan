# Bug: Database Initialization Fails - Missing execution_log Table

## Summary

**Version:** 2.1.0 (d47a0ce 2026-02-07)

Magellan fails to initialize a new `codegraph.db` database with error:
```
Error: Failed to insert execution log: no such table: execution_log
```

This creates an incomplete 88-byte database file that cannot be used, preventing
any codegraph operations from working.

## Current Behavior

### Reproduction Steps
```bash
# Remove any existing database
rm -f /tmp/test_codegraph.db

# Try to create new database
magellan status --db /tmp/test_codegraph.db

# Result:
# [CLUSTER_DEBUG] initialize_v2_header() called - fixing cluster offsets...
# Error: Failed to insert execution log: no such table: execution_log
```

### Resulting Database File
```bash
ls -la /tmp/test_codegraph.db
# -rw-r--r-- 1 feanor feanor 88 ... /tmp/test_codegraph.db

xxd -l 16 /tmp/test_codegraph.db
# 00000000: 5351 4c54 4746 0000...  (SQLTGF magic number)
# But file is only 88 bytes - no tables created
```

### Trying to Use the Incomplete Database
```bash
magellan status --db /tmp/test_codegraph.db
# Error: connection error: Invalid magic number: expected 0x53514c5447460000, got 0x53514c6974652066
```

## Expected Behavior

When `magellan` commands run with a non-existent database:
1. Create valid SQLiteGraph database with SQLTGF magic number
2. Initialize all required tables including `execution_log`
3. Complete database initialization successfully
4. Allow subsequent commands to query/use the database

## Root Cause

**Chicken-and-egg problem:** During database initialization, magellan tries to
insert into the `execution_log` table BEFORE the table schema is created.

The initialization sequence appears to be:
1. `initialize_v2_header()` - Creates SQLTGF header ✅
2. Fix cluster offsets ✅
3. **Try to insert execution log** ❌ (table doesn't exist yet)
4. Create table schemas (never reached)

## Impact

### Severity: CRITICAL

- **Cannot create new codegraph databases** - All magellan operations fail
- **Downstream tools broken** - odincode, codemcp, and any tool using magellan fails
- **Existing databases rejected** - Pre-2.1.0 databases with "SQLite format 3" magic
  number are rejected with "Invalid magic number" error

### Affected Commands
- `magellan status`
- `magellan watch`
- `magellan verify`
- All commands that create/open databases

### Error Output to Users
```
[CLUSTER_DEBUG] initialize_v2_header() called - fixing cluster offsets to prevent node slot corruption
[CLUSTER_DEBUG] Layout invariants:
  node_data_offset = 512
  node_count = 0
  node_region_end = 512
  base_cluster_start = 512
  cluster_floor = 1536
  current outgoing_cluster_offset = 0
  current incoming_cluster_offset = 0
CRITICAL FIX: Moving outgoing_cluster_offset from 0 to 1536 to prevent node slot corruption
CRITICAL FIX: Moving incoming_cluster_offset from 0 to 1536 to prevent node slot corruption
  final outgoing_cluster_offset = 1536
  final incoming_cluster_offset = 1536
Error: Failed to insert execution log: no such table: execution_log
```

## Files to Fix

Likely in `magellan/src/`:
- Database initialization code (where `initialize_v2_header()` is called)
- Execution logging setup (where execution log insert happens)
- Table schema creation (needs to run before logging attempts)

The fix should ensure table schemas are created BEFORE any execution logging attempts.

## Temporary Workarounds

### Option 1: Use Pre-2.1.0 Database
If you have a `codegraph.db` from magellan < 2.1.0, it may still work
(but "Invalid magic number" errors suggest this is inconsistent).

### Option 2: Copy Existing Working Database
```bash
# From odincode project
cp /home/feanor/Projects/odincode/.codemcp/codegraph.db /path/to/new/location
```

### Option 3: Wait for Fix
This is a critical bug that blocks all new database creation.

## Verification

After fix, the following should work:
```bash
# Test 1: Create new database
rm -f /tmp/test_codegraph.db
magellan status --db /tmp/test_codegraph.db
# Expected: Success (not an error)

# Test 2: Verify database is valid
file /tmp/test_codegraph.db
# Expected: SQLite 3.x database (with SQLTGF magic)
# Size: > 1000 bytes (not 88 bytes)

# Test 3: Check tables exist
sqlite3 /tmp/test_codegraph.db ".tables"
# Expected: ast_nodes, graph_edges, graph_entities, execution_log, etc.

# Test 4: Run watch command
magellan watch --root ./src --db /tmp/test_codegraph.db --debounce-ms 500 &
# Expected: Watcher starts successfully
```

## Additional Context

### Database Format Inconsistency
- **New format (2.1.0):** SQLTGF magic number (`0x53514c5447460000`)
- **Old format (< 2.1.0):** SQLite format 3 (`0x53514c6974652066`)
- **Incompatibility:** Old databases rejected with "Invalid magic number"

### Related Issues
- odincode reports file_glob operations hanging (likely due to failed codegraph access)
- codemcp may also be affected

### Report Date
2026-02-08

### Reporter
claude@anthropic.com (via odincode project investigation)
