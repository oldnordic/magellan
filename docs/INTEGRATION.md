# Integration Guide

**Last Updated:** 2026-02-10
**Version:** v2.2.1

How to use Magellan with other tools in the code intelligence ecosystem.

---

## Table of Contents

1. [Ecosystem Overview](#ecosystem-overview)
2. [LLM Workflows](#llm-workflows)
3. [llmgrep Integration](#llmgrep-integration)
4. [splice Integration](#splice-integration)
5. [mirage Integration](#mirage-integration)
6. [Traditional Tools](#traditional-tools)
7. [CI/CD Integration](#cicd-integration)
8. [Editor Plugins](#editor-plugins)

---

## Ecosystem Overview

Magellan is part of a larger code intelligence ecosystem:

```
┌─────────────────────────────────────────────────────────────┐
│                    Code Intelligence Ecosystem               │
│                                                              │
│  ┌──────────┐                                               │
│  │ Magellan │ ──────┐                                       │
│  │ (index)  │       │                                       │
│  └─────┬────┘       │                                       │
│        │            │                                       │
│        ▼            │                                       │
│  ┌──────────────────┴─────┐       ┌──────────┐             │
│  │    codegraph.db        │◄──────│ splice   │             │
│  │  (sqlitegraph db)      │       │ (refactor)│             │
│  └────────────┬───────────┘       └────┬─────┘             │
│               │                        │                    │
│               │                        │                    │
│      ┌────────┴────────┐               │                    │
│      ▼                 ▼               ▼                    │
│ ┌─────────┐      ┌─────────┐   ┌──────────┐               │
│ │ llmgrep │      │ mirage  │   │ OdinCode │               │
│ │ (search)│      │   CFG   │   │  (LLM)   │               │
│ └─────────┘      └─────────┘   └──────────┘               │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Key Concept:** All tools share the same `codegraph.db` database created by Magellan.

---

## LLM Workflows

Magellan is designed to work seamlessly with AI assistants like Claude, ChatGPT, and OdinCode.

### Typical LLM Workflow

```bash
# 1. Index the codebase once
magellan watch --root ./src --db .codemcp/codegraph.db --scan-initial &
WATCH_PID=$!

# 2. LLM queries the database as needed
llmgrep --db .codemcp/codegraph.db search --query "auth" --output json

# 3. LLM performs refactoring using precise spans
splice patch --file src/auth.rs --symbol login --with new_login.rs

# 4. Watcher automatically re-indexes changed files
# (No manual intervention needed)

# 5. Cleanup
kill $WATCH_PID
```

### LLM-Friendly Features

| Feature | Why LLMs Love It |
|---------|------------------|
| Stable SymbolId | Same ID across sessions = reliable references |
| JSON output | Parseable, structured data |
| Byte spans | Precise location without reading full files |
| `--explain` flag | Self-documenting commands |
| CLI interface | Generate shell commands, not in-process code |

### Example: Claude Code Workflow

```markdown
# User asks: "Find all functions related to authentication"

# Claude generates:
1. llmgrep --db .codemcp/codegraph.db search --query "auth" --output json
2. magellan label --db .codemcp/codegraph.db --label fn --show-code

# User: "Rename login_handler to authenticate_user"

# Claude generates:
1. magellan find --db .codemcp/codegraph.db --name login_handler --output json
2. splice rename --symbol <ID> --file src/auth.rs --to authenticate_user
```

---

## llmgrep Integration

**llmgrep** is a semantic code search tool that reads Magellan's database.

### What llmgrep Adds

- Semantic search (finds symbols by purpose, not just name)
- Fuzzy matching (handles typos, variations)
- Relevance ranking (best results first)

### Workflow

```bash
# 1. Magellan indexes the codebase
magellan watch --root . --db .codemcp/codegraph.db --scan-initial

# 2. Use llmgrep for semantic queries
llmgrep --db .codemcp/codegraph.db search --query "user authentication" --output json

# 3. Get detailed info from Magellan
magellan find --db .codemcp/codegraph.db --name authenticate_user

# 4. Use splice for refactoring
splice rename --symbol <ID> --file src/auth.rs --to login_user
```

### When to Use Each

| Task | Use | Because |
|------|-----|---------|
| Exact symbol lookup | `magellan find` | Precise, fast |
| Semantic discovery | `llmgrep search` | Fuzzy, ranked |
| File structure | `magellan query` | Complete listing |
| Symbol relationships | `magellan refs` | Call graph |

---

## splice Integration

**splice** is a precision code editing tool that uses Magellan's symbol database.

### What splice Adds

- Byte-accurate code editing
- Cross-file refactoring
- Safe symbol renaming
- AST-aware validation

### Workflow

```bash
# 1. Find the symbol to refactor
magellan find --db .codemcp/codegraph.db --name old_function --output json

# Output includes:
# - symbol_id (stable identifier)
# - canonical_fqn (unambiguous name)
# - byte_start, byte_end (precise location)

# 2. Preview the change
splice rename --symbol <SYMBOL_ID> --file src/lib.rs --to new_function --preview

# 3. Apply the change
splice rename --symbol <SYMBOL_ID> --file src/lib.rs --to new_function

# 4. Magellan watcher automatically re-indexes
# (No manual re-index needed)
```

### Advanced: Multi-File Refactoring

```bash
# 1. Find all symbols matching a pattern
magellan find --db .codemcp/codegraph.db --list-glob "handle_*" --output json > matches.json

# 2. Generate rename commands
jq -r '.data.matches[] | "splice rename --symbol \(.symbol_id) --file \(.file_path) --to \(.name | sub("^handle_"; "process_"))"' matches.json > renames.sh

# 3. Review and execute
cat renames.sh  # Review
bash renames.sh  # Execute
```

---

## mirage Integration

**mirage** is a control flow graph (CFG) and path analysis tool for Rust code.

### What mirage Adds

- CFG visualization (function-level control flow)
- Path enumeration (all execution paths)
- Loop detection
- Dominance analysis

### Workflow

```bash
# 1. Magellan indexes the codebase
magellan watch --root . --db .codemcp/codegraph.db --scan-initial

# 2. Use mirage for CFG analysis
mirage --db .codemcp/codegraph.db cfg --function "process_request"

# 3. Find all execution paths
mirage --db .codemcp/codegraph.db paths --function "process_request"

# 4. Detect loops
mirage --db .codemcp/codegraph.db loops --function "process_request"
```

### When to Use mirage

| Task | Use | Because |
|------|-----|---------|
| Call graph analysis | `magellan cycles` | Cross-function relationships |
| Control flow within function | `mirage cfg` | Internal function structure |
| Execution paths | `mirage paths` | All possible paths |
| Dead code detection | `magellan dead-code` | Unreachable from entry point |

---

## Traditional Tools

Magellan complements, not replaces, traditional Unix tools.

### ripgrep (rg)

```bash
# Use ripgrep for:               Use Magellan for:
# - Substring search            - Symbol definitions
# - Regex search                - Call relationships
# - Literal text matching       - Cross-file references
rg "TODO" ./src                 magellan refs --db ./db --name main
```

### ctags

```bash
# ctags advantages:             Magellan advantages:
# - Vim/Emacs integration       - Graph algorithms
# - Instant symbol jump         - JSON output
# - No indexing step needed     - Multi-language consistency
ctags -R .                      magellan watch --root . --db ./db --scan-initial
```

### GNU Global

```bash
# Global advantages:            Magellan advantages:
# - Web-based browsing          - LLM-friendly output
# - Incremental updates         - Modern languages (Rust, TS, etc.)
gtags                           magellan watch --root . --db ./db --scan-initial
```

---

## CI/CD Integration

Magellan fits naturally into CI/CD pipelines for code quality checks.

### GitHub Actions Example

```yaml
name: Code Quality

on: [push, pull_request]

jobs:
  graph-analysis:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Magellan
        run: cargo install magellan

      - name: Index codebase
        run: magellan watch --root . --db ./codegraph.db --scan-initial &
        shell: bash

      - name: Wait for indexing
        run: sleep 10

      - name: Check for dead code
        run: |
          DEAD=$(magellan dead-code --db ./codegraph.db --entry main --output json | jq '.data.dead_symbols | length')
          if [ "$DEAD" -gt 10 ]; then
            echo "Too much dead code: $DEAD functions"
            exit 1
          fi

      - name: Check for cycles
        run: |
          CYCLES=$(magellan cycles --db ./codegraph.db --output json | jq '.data.cycles | length')
          if [ "$CYCLES" -gt 5 ]; then
            echo "Too many cycles: $CYCLES"
            magellan cycles --db ./codegraph.db
            exit 1
          fi

      - name: Export graph for artifacts
        run: magellan export --db ./codegraph.db --output codegraph.json

      - uses: actions/upload-artifact@v3
        with:
          name: codegraph
          path: codegraph.json
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Index current state
magellan watch --root . --db .git/magellan.db --scan-initial &
MAGELLAN_PID=$!
sleep 5

# Run checks
DEAD=$(magellan dead-code --db .git/magellan.db --entry main --output json | jq '.data.dead_symbols | length')

if [ "$DEAD" -gt 0 ]; then
    echo "Warning: $DEAD dead functions detected"
    magellan dead-code --db .git/magellan.db --entry main
fi

kill $MAGELLAN_PID
```

---

## Editor Plugins

### Neovim/Vim

```vim
" FZF integration for symbol search
command! -nargs=1 MagellanFind call fzf#run({
  \ 'source': 'magellan find --db .codemcp/codegraph.db --list-glob "*' . <q-args> . '*" --output json | jq -r ''.data.matches[] | "\(.name)\t\(.file_path):\(.start_line)"''',
  \ 'sink': function(line)
    \ let parts = split(line, '\t')
    \ execute 'e ' . parts[1]
  \ endfunction
})

" LSP-like goto definition
nnoremap <silent> gd :MagellanFind<CR>
```

### VS Code Extension

```typescript
// vscode-magellan/src/extension.ts
import * as vscode from 'vscode';
import { exec } from 'child_process';

export function activate(context: vscode.ExtensionContext) {
  const goToDefinition = vscode.commands.registerCommand(
    'magellan.goToDefinition',
    async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) return;

      const word = editor.document.getText(editor.selection);
      const result = exec(
        `magellan find --db .codemcp/codegraph.db --name ${word} --output json`
      );

      const data = JSON.parse(result.stdout);
      if (data.data.match) {
        const doc = await vscode.workspace.openTextDocument(
          data.data.match.file_path
        );
        const position = new vscode.Position(
          data.data.match.start_line - 1,
          data.data.match.start_col
        );
        vscode.window.showTextDocument(doc, {
          selection: new vscode.Range(position, position)
        });
      }
    }
  );

  context.subscriptions.push(goToDefinition);
}
```

---

## Language Server Protocol (LSP)

Magellan can complement LSP servers:

| What LSP Provides | What Magellan Provides |
|-------------------|------------------------|
| Real-time diagnostics | Batch analysis |
| In-editor navigation | Database persistence |
| Type information | Call graph algorithms |
| Refactoring (single file) | Cross-file refactoring |

### Using Magellan alongside rust-analyzer

```bash
# Terminal 1: rust-analyzer (LSP)
rust-analyzer

# Terminal 2: Magellan watcher
magellan watch --root . --db .codemcp/codegraph.db --scan-initial

# Now you have:
# - LSP for: autocomplete, diagnostics, inline errors
# - Magellan for: dead code detection, cycles, export
```

---

## Database Sharing

The key to integration is database sharing. All tools read from the same database:

```bash
# Set environment variable for convenience
export MAGELLAN_DB="${XDG_CACHE_HOME:-$HOME/.cache}/magellan/project.db"

# All tools use the same database
magellan watch --root . --db "$MAGELLAN_DB" --scan-initial &
llmgrep --db "$MAGELLAN_DB" search --query "auth"
mirage --db "$MAGELLAN_DB" cfg --function "main"
splice rename --db "$MAGELLAN_DB" --symbol <ID> --to new_name
```

### Database Locking

**Important:** Only one writer at a time, but multiple readers are fine:

```bash
# OK: Multiple readers
magellan status --db ./codegraph.db &
llmgrep --db ./codegraph.db search --query "test" &
mirage --db ./codegraph.db cfg --function "main" &

# NOT OK: Multiple writers
magellan watch --root . --db ./codegraph.db --scan-initial &  # Writer 1
magellan watch --root . --db ./codegraph.db --scan-initial &  # Writer 2 - FAIL!
```

---

## Quick Reference

| Tool | Command Example | Purpose |
|------|----------------|---------|
| **Magellan** | `magellan watch --root . --db ./db --scan-initial` | Index codebase |
| **llmgrep** | `llmgrep --db ./db search --query "auth" --output json` | Semantic search |
| **splice** | `splice rename --symbol <ID> --file src/lib.rs --to new_name` | Refactor code |
| **mirage** | `mirage --db ./db cfg --function "main"` | CFG analysis |
| **ripgrep** | `rg "TODO" ./src` | Text search |
| **ctags** | `ctags -R .` | Editor tags |

---

## Further Reading

- [README.md](../README.md) - Quick start guide
- [MANUAL.md](../MANUAL.md) - Comprehensive command reference
- [PHILOSOPHY.md](PHILOSOPHY.md) - Design principles
- [PERFORMANCE.md](PERFORMANCE.md) - Benchmarks and optimization
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues and solutions
