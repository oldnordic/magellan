# Magellan Scripts Collection

This directory contains project-agnostic scripts for code analysis using Magellan.

## Quick Start

1. **Copy to your project** (or use directly from Magellan):
   ```bash
   cp -r /path/to/magellan/scripts /your/project/scripts
   ```

2. **Configure** (optional, via environment variables):
   ```bash
   export PROJECT_NAME=myproject
   export DB_DIR=.codemcp
   export SRC_DIR=src
   ```

3. **Start the watcher**:
   ```bash
   ./scripts/magellan-workflow.sh start
   ```

## Available Scripts

| Script | Purpose | Example |
|--------|---------|---------|
| `magellan-workflow.sh` | Main workflow: start/stop watcher, search, get, refs | `./scripts/magellan-workflow.sh search "MyFunction"` |
| `blast-zone.sh` | Impact analysis: what gets affected by a change | `./scripts/blast-zone.sh "process_file" --max-depth 2` |
| `call-chain.sh` | Show call chains forward/backward | `./scripts/call-chain.sh "handle_request" --direction backward` |
| `unreachable.sh` | Find unreachable public functions | `./scripts/unreachable.sh` |
| `module-deps.sh` | Module dependency analysis | `./scripts/module-deps.sh --format dot` |

## Configuration

All scripts respect these environment variables:

- `PROJECT_NAME` - Database filename (default: magellan)
- `DB_DIR` - Database directory (default: .codemcp)
- `SRC_DIR` - Source directory to index (default: src)

## Requirements

- `magellan` - Code graph CLI tool (install via `cargo install --path .`)
- `llmgrep` - Symbol search CLI (installed with magellan)
- `jq` - JSON processor (for formatted output)
- `sqlite3` - For metrics queries (optional)

## Installing Magellan

From the Magellan project directory:
```bash
cargo install --path .
```

This installs both `magellan` and `llmgrep` to your PATH.

## Documentation

See `CODE_ANALYSIS_CAPABILITIES.md` for detailed documentation of what analysis is possible with Magellan.
