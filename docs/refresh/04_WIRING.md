# Refresh Command: Wiring in main.rs

**Date:** 2026-03-15
**Task:** Wire the refresh command handler into main.rs

## Overview

This document describes the wiring of the `refresh` command into the Magellan CLI main entry point.

## Changes Made

### 1. Module Declaration

Added `mod refresh_cmd;` to the module declarations at the top of `/home/feanor/Projects/magellan/src/main.rs`.

**Location:** Line 24 (alphabetically placed between `reachable_cmd` and `refs_cmd`)

```rust
mod ast_cmd;
mod cli;
mod collisions_cmd;
mod condense_cmd;
mod context_cmd;
mod cycles_cmd;
mod dead_code_cmd;
mod doctor_cmd;
mod enrich_cmd;
mod export_cmd;
mod files_cmd;
mod find_cmd;
mod get_cmd;
mod import_lsif_cmd;
mod label_cmd;
mod migrate_cmd;
mod path_enumeration_cmd;
mod query_cmd;
mod reachable_cmd;
mod refresh_cmd;  // Added here
mod refs_cmd;
mod slice_cmd;
mod status_cmd;
mod verify_cmd;
mod version;
mod watch_cmd;
```

### 2. Import Cleanup

Removed unused `std::path::PathBuf` import from main.rs (line 36 was removed).

### 3. Command Handler

Added the `Command::Refresh` handler in the match statement after the `Slice` command handler (around line 660).

**Structure:**

```rust
Ok(Command::Refresh {
    db_path,
    dry_run,
    include_untracked,
    staged,
    unstaged,
    force,
    output_format,
}) => {
    let args = refresh_cmd::RefreshArgs {
        db_path,
        dry_run,
        include_untracked,
        staged,
        unstaged,
        force,
        output_format,
    };
    match refresh_cmd::run_refresh(&args) {
        Ok(report) => {
            // Output report based on output_format
            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
                }
                OutputFormat::Human => {
                    println!("Refresh complete:");
                    println!("  Updated: {}", report.updated.len());
                    println!("  Deleted: {}", report.deleted.len());
                    println!("  Added: {}", report.added.len());
                    println!("  Unchanged: {}", report.unchanged);
                    if report.dry_run {
                        println!("  (dry run - no changes applied)");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
```

### 4. refresh_cmd.rs Module Created

Created `/home/feanor/Projects/magellan/src/refresh_cmd.rs` with:

- `RefreshArgs` struct - Arguments for the refresh command
- `RefreshReport` struct - Serializable report of refresh operations
- `run_refresh()` function - Main entry point for the refresh command
- Helper functions:
  - `find_git_root()` - Locates the git repository root
  - `get_git_status()` - Gets git status with appropriate options
  - `apply_changes()` - Applies detected changes to the database (placeholder)

## Compilation Issues Resolved

### Issue 1: Import Path
**Problem:** `use crate::output::OutputFormat;` was incorrect for a binary module.
**Fix:** Changed to `use magellan::output::OutputFormat;`

### Issue 2: Missing tracing crate
**Problem:** Used `tracing::debug!()` but tracing is not a dependency.
**Fix:** Changed to `eprintln!()` for logging pending operations.

### Issue 3: Lifetime specifier
**Problem:** `git2::Statuses` requires a lifetime parameter.
**Fix:** Added explicit lifetime: `fn get_git_status<'a>(repo: &'a Repository, args: &'a RefreshArgs) -> Result<git2::Statuses<'a>>`

### Issue 4: Type inference
**Problem:** `entry.path()` returns `Option<&str>` but `to_string_lossy()` couldn't be called directly.
**Fix:** Used explicit type annotation and `map()`/`unwrap_or_else()` pattern.

### Issue 5: Unused import in main.rs
**Problem:** `std::path::PathBuf` was imported but unused.
**Fix:** Removed the import.

### Issue 6: Dead code warning
**Problem:** `output_format` field in `RefreshArgs` was stored but not used within the module.
**Fix:** Added `#[allow(dead_code)]` attribute since the field is passed from CLI but output formatting is handled in main.rs.

## Verification

```bash
$ cargo check
    Checking magellan v3.1.1 (/home/feanor/Projects/magellan)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.22s
```

Compilation succeeds with no warnings.

## Files Modified

| File | Lines Changed | Description |
|------|---------------|-------------|
| `/home/feanor/Projects/magellan/src/main.rs` | +35, -2 | Added module declaration, removed unused import, added command handler |
| `/home/feanor/Projects/magellan/src/refresh_cmd.rs` | +190 | New module implementing refresh command logic |

## Next Steps

The refresh command is now wired and compiles. The actual database update logic in `apply_changes()` is currently a placeholder and should be implemented in a future phase to:

1. Remove deleted files from the database
2. Re-index modified files
3. Index new files
4. Handle the `--force` flag for full re-indexing
