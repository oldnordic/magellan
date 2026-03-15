# Refresh Command Implementation

## Overview

This document describes the implementation of the `refresh` command for Magellan, which synchronizes the graph database with the current git working tree state.

## Functions Implemented

### Core Structures

#### `RefreshArgs`
Command-line arguments structure matching the CLI fields:
- `db_path: PathBuf` - Path to the database file
- `dry_run: bool` - Preview changes without applying
- `include_untracked: bool` - Include untracked files
- `staged: bool` - Only process staged changes
- `unstaged: bool` - Only process unstaged changes
- `force: bool` - Force refresh even if no changes
- `output_format: OutputFormat` - Output format (Human/Json/Pretty)

#### `RefreshReport`
Report structure for operation results:
- `updated: Vec<String>` - Files that were re-indexed
- `deleted: Vec<String>` - Files removed from database
- `added: Vec<String>` - New files indexed
- `unchanged: usize` - Count of unchanged files
- `duration_ms: u64` - Operation duration

### Main Function

#### `run_refresh(args: &RefreshArgs) -> Result<RefreshReport>`
The main entry point that:
1. Opens the git repository using `git2::Repository::open(".")`
2. Opens the magellan database using `CodeGraph::open()`
3. Gets git status (modified, deleted, untracked files)
4. Gets all files from the database
5. Computes delta between git and database
6. Applies changes (unless `--dry-run`)
7. Outputs results in human or JSON format

### Helper Functions

#### `get_git_status(repo: &Repository, args: &RefreshArgs) -> Result<GitStatus>`
Uses `git2::StatusOptions` to retrieve repository status:
- Configures options based on flags (`--include-untracked`, `--staged`, `--unstaged`)
- Classifies files into modified, deleted, untracked, staged, unstaged categories
- Handles rename detection

#### `compute_delta(git_status: &GitStatus, db_files: &HashSet<String>, args: &RefreshArgs) -> Result<FileDelta>`
Computes the synchronization delta:
- **Files to update**: Modified in git AND exist in database
- **Files to delete**: In database but deleted in git (or missing from filesystem)
- **Files to add**: New in git (if `--include-untracked`) and not in database
- **Unchanged files**: All other files in database

#### `apply_changes(graph: &mut CodeGraph, delta: &FileDelta) -> Result<()>`
Applies the computed changes:
- Uses `graph.reconcile_file_path(path, path_key)` for updates and adds
- Uses `graph.delete_file_facts(path)` for deletions
- Logs progress to stderr for visibility

#### `print_human_output(report: &RefreshReport, dry_run: bool)`
Formats and prints human-readable output showing:
- Summary of changes (updated/deleted/added counts)
- List of affected files
- Duration

## Key Design Decisions

### 1. Git Integration with git2

The implementation uses the `git2` crate (already a dependency) for robust git operations:

```rust
let repo = Repository::open(".")
    .context("Failed to open git repository. Are you in a git repository?")?;
```

**Rationale**:
- `git2` is a mature, well-tested Rust binding to libgit2
- Already used elsewhere in the codebase
- Provides comprehensive status information including renames
- No external process spawning needed

### 2. Delta Computation Strategy

The refresh command computes a three-way delta:

```
Git Status          Database          Action
------------        --------          ------
Modified     +      Exists      ->    Update (reconcile)
Deleted      +      Exists      ->    Delete
New/Untracked +     Missing     ->    Add (if --include-untracked)
Missing (FS) +      Exists      ->    Delete (stale detection)
```

**Rationale**:
- Explicit delta computation allows dry-run preview
- Stale file detection handles cases where files are deleted outside git
- Separating update from add allows different handling strategies

### 3. Reuse of Existing Primitives

The implementation leverages existing Magellan primitives:

- **`reconcile_file_path()`**: Used for both updates and adds because it handles:
  - Hash comparison (skips unchanged files)
  - Deletion of old facts
  - Re-indexing with proper symbol/reference extraction

- **`delete_file_facts()`**: Used for deletions because it:
  - Removes all file-derived facts (symbols, references, calls, chunks)
  - Maintains graph consistency
  - Returns detailed deletion statistics

**Rationale**:
- Avoids code duplication
- Ensures consistency with watcher and indexer behavior
- Benefits from existing transaction safety

### 4. Execution Tracking

The command integrates with Magellan's execution log:

```rust
graph.execution_log().start_execution(&exec_id, ...)?;
// ... do work ...
graph.execution_log().finish_execution(&exec_id, "success", ...)?;
```

**Rationale**:
- Consistent with other commands
- Enables audit trails and debugging
- Tracks performance metrics

### 5. Error Handling Strategy

Errors during individual file operations are logged but don't fail the entire command:

```rust
match graph.reconcile_file_path(path, path_str) {
    Ok(outcome) => { /* log success */ }
    Err(e) => { eprintln!("  Error updating {}: {}", path_str, e); }
}
```

**Rationale**:
- Partial success is better than total failure
- One corrupt file shouldn't block refresh of others
- Errors are visible in stderr for investigation

## How git2 is Used

### Repository Discovery

```rust
let repo = Repository::open(".")?;
```

Opens the git repository from the current directory, automatically discovering the `.git` directory.

### Status Retrieval

```rust
let mut status_opts = StatusOptions::new();
status_opts
    .include_untracked(args.include_untracked)
    .renames_head_to_index(true)
    .renames_index_to_workdir(true);

let statuses = repo.statuses(Some(&mut status_opts))?;
```

Configures status options based on command flags and retrieves all status entries.

### Status Classification

```rust
let is_staged = status.is_index_new()
    || status.is_index_modified()
    || status.is_index_deleted()
    || status.is_index_renamed()
    || status.is_index_typechange();

let is_unstaged = status.is_wt_new()
    || status.is_wt_modified()
    || status.is_wt_deleted()
    || status.is_wt_renamed()
    || status.is_wt_typechange();
```

Classifies each file's status into staged/unstaged categories for filtering.

## Challenges Encountered

### 1. Staged vs Unstaged Filtering

**Challenge**: git2's `StatusOptions` doesn't have a direct way to exclude staged changes when looking at unstaged changes.

**Solution**: Retrieve all statuses and filter in Rust code based on the `is_staged` and `is_unstaged` flags:

```rust
let modified_files: HashSet<String> = if args.staged {
    git_status.staged.iter().cloned().collect()
} else if args.unstaged {
    git_status.unstaged.iter().cloned().collect()
} else {
    git_status.modified.iter().cloned().collect()
};
```

### 2. Untracked File Handling

**Challenge**: Untracked files need special handling - they should only be added if:
1. `--include-untracked` flag is set
2. The file actually exists on the filesystem
3. The file is not already in the database

**Solution**: Explicit checks at each stage:

```rust
if args.include_untracked {
    for path in &untracked_files {
        if !db_files.contains(path) && Path::new(path).exists() {
            to_add.push(path.clone());
        }
    }
}
```

### 3. Stale File Detection

**Challenge**: Files might be deleted outside of git (e.g., `rm` instead of `git rm`), leaving stale entries in the database.

**Solution**: Check filesystem existence for all database files:

```rust
for path in db_files {
    if deleted_files.contains(path) {
        to_delete.push(path.clone());
    } else if !Path::new(path).exists() {
        // File doesn't exist on filesystem (stale in DB)
        to_delete.push(path.clone());
    }
}
```

### 4. Testability

**Challenge**: Testing git-dependent code is difficult in unit tests.

**Solution**:
- Extract pure logic functions (`compute_delta`) that don't depend on git
- Use dependency injection pattern for testability
- Focus tests on delta computation logic rather than git operations

```rust
#[test]
fn test_compute_delta_basic() {
    let git_status = GitStatus { /* ... */ };
    let mut db_files = HashSet::new();
    // ... populate db_files ...
    let delta = compute_delta(&git_status, &db_files, &args).unwrap();
    // assertions ...
}
```

## Testing

The module includes unit tests for:

1. **Report structure**: `test_refresh_report_new`, `test_refresh_report_total_changes`
2. **Default arguments**: `test_refresh_args_default`
3. **Delta computation**: `test_compute_delta_basic`, `test_compute_delta_with_untracked`
4. **JSON response**: `test_refresh_response_from_report`

Run tests with:
```bash
cargo test refresh_cmd::tests
```

## Future Enhancements

Potential improvements for future phases:

1. **Parallel processing**: Process multiple files concurrently using rayon
2. **Progress bar**: Add indicatif progress bar for large repositories
3. **Force mode**: Implement `--force` to re-index all tracked files
4. **Submodule support**: Handle git submodules correctly
5. **Ignore patterns**: Respect `.gitignore` patterns for untracked files
6. **Incremental stats**: Track symbol/reference counts in report
