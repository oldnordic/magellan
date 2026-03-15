# Refresh Command CLI Implementation

## Overview

This document describes the implementation of the `refresh` command in the Magellan CLI parser.

## Command Enum Addition

The `Refresh` variant was added to the `Command` enum in `/home/feanor/Projects/magellan/src/cli.rs`, positioned logically after the `Verify` command (around line 515).

### Location in Command Enum

```rust
Verify {
    root_path: PathBuf,
    db_path: PathBuf,
},
/// Refresh index based on git changes
Refresh {
    db_path: PathBuf,
    dry_run: bool,
    include_untracked: bool,
    staged: bool,
    unstaged: bool,
    force: bool,
    output_format: OutputFormat,
},
/// Query symbols by label (Phase 2: Label integration)
Label {
    ...
}
```

The `Refresh` variant follows the same pattern as other database-related commands like `Doctor`, `Migrate`, and `Verify`.

## Parsing Logic

The parsing logic was added in a new function `parse_refresh_args()` located after `parse_verify_args()` (around line 1870).

### Command Registration

In the `parse_args_impl()` function's match statement, the refresh command was registered:

```rust
match command.as_str() {
    ...
    "verify" => parse_verify_args(&args[2..]),
    "refresh" => parse_refresh_args(&args[2..]),  // Added here
    "label" => parse_label_args(&args[2..]),
    ...
}
```

### Parse Function Implementation

The `parse_refresh_args()` function follows the established patterns from `parse_migrate_args()` and `parse_doctor_args()`:

```rust
/// Parse the `refresh` command arguments
fn parse_refresh_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut include_untracked = false;
    let mut staged = false;
    let mut unstaged = false;
    let mut force = false;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--include-untracked" => {
                include_untracked = true;
                i += 1;
            }
            "--staged" => {
                staged = true;
                i += 1;
            }
            "--unstaged" => {
                unstaged = true;
                i += 1;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            "--output" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--output requires an argument"));
                }
                output_format = match args[i + 1].as_str() {
                    "human" => OutputFormat::Human,
                    "json" => OutputFormat::Json,
                    "pretty" => OutputFormat::Pretty,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid output format: {}. Must be human, json, or pretty",
                            args[i + 1]
                        ))
                    }
                };
                i += 2;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    Ok(Command::Refresh {
        db_path,
        dry_run,
        include_untracked,
        staged,
        unstaged,
        force,
        output_format,
    })
}
```

## Supported Flags

| Flag | Type | Description |
|------|------|-------------|
| `--db <path>` | Required | Path to the sqlitegraph database |
| `--dry-run` | Boolean | Preview changes without applying them |
| `--include-untracked` | Boolean | Include untracked files in the refresh |
| `--staged` | Boolean | Only process staged changes |
| `--unstaged` | Boolean | Only process unstaged changes |
| `--force` | Boolean | Force re-index all tracked files |
| `--output <format>` | Optional | Output format: `human` (default), `json`, or `pretty` |

## Patterns Followed

The implementation follows these established patterns from the codebase:

1. **Argument Parsing Style**: Uses the same `while` loop with `match` pattern seen in `parse_migrate_args()`, `parse_doctor_args()`, and other command parsers.

2. **Error Handling**: Uses `anyhow::anyhow!()` for error messages, consistent with other parsers.

3. **Required Arguments**: Uses `ok_or_else()` to convert `Option` to `Result` with a descriptive error message for missing required arguments (like `--db`).

4. **Boolean Flags**: Simple boolean flags (like `--dry-run`, `--force`) increment by 1 and set the flag to `true`.

5. **Output Format**: The `--output` flag uses the same three-way match pattern (`human`, `json`, `pretty`) used throughout the codebase.

6. **Documentation Comment**: Includes a `///` doc comment following Rust conventions, similar to other parse functions.

## Usage Example

```bash
# Basic refresh
magellan refresh --db code.db

# Dry run to preview changes
magellan refresh --db code.db --dry-run

# Include untracked files
magellan refresh --db code.db --include-untracked

# Only staged changes with JSON output
magellan refresh --db code.db --staged --output json

# Force re-index all tracked files
magellan refresh --db code.db --force
```
