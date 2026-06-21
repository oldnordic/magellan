# Codeindex Features Implementation Plan

**Goal:** Add codeindex-like features to grounded coding stack (magellan, llmgrep, mirage)

**Current State Analysis (from graph):**

- `magellan export` exports full graph (7.2MB JSON) to absolute paths
- `magellan context impact --depth N` returns `Vec<SymbolRelation>` with depth per symbol
- `SymbolRelation` has: `name`, `file`, `line`, `depth: Option<usize>`
- No single blast score metric
- No simple symbol map export (O(1) lookup)
- No pre-commit hook infrastructure
- No repo-root file convention

**Gap Analysis:**

| Feature | codeindex | Current State | Gap |
|---------|-----------|---------------|-----|
| Single blast score | "8.5 (2 direct · 7 transitive) [HIGH]" | Depth-bounded list only | Need score computation |
| Symbol map export | O(1) JSON lookup | Full graph dump only | Need simple export |
| Impact export | impact.json with scores | context impact returns list | Need export format |
| Pre-commit hook | git hook enforcement | None | Need hook infra |
| Repo-root discovery | Files in repo root | Absolute paths only | Need convention |

---

## Phase 1: Blast Score Computation (magellan)

**Objective:** Add `magellan blast-score --symbol <name>` returning single score

**Implementation:**

### 1.1 Add score computation logic
**File:** `src/context/query.rs` (new function)

```rust
pub fn compute_blast_score(
    graph: &mut CodeGraph,
    symbol_name: &str,
    file_path: Option<&str>,
    max_depth: usize,
) -> Result<BlastScore> {
    let impacted = impact_analysis(graph, symbol_name, file_path, max_depth)?;

    let direct_count = impacted.iter()
        .filter(|r| r.depth == Some(1))
        .count();

    let transitive_count = impacted.iter()
        .filter(|r| r.depth.map(|d| d > 1).unwrap_or(false))
        .count();

    // codeindex formula: direct + 0.5 * transitive
    let score = (direct_count as f64) + (0.5 * transitive_count as f64);

    let total_files = graph.files()?.len() as f64;
    let risk_percent = (impacted.len() as f64 / total_files) * 100.0;

    let risk_level = match risk_percent {
        p if p >= 20.0 => "HIGH",
        p if p >= 10.0 => "MEDIUM",
        _ => "LOW",
    };

    Ok(BlastScore {
        score,
        direct_count,
        transitive_count,
        risk_level: risk_level.to_string(),
        risk_percent,
        total_impacted: impacted.len(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastScore {
    pub score: f64,
    pub direct_count: usize,
    pub transitive_count: usize,
    pub risk_level: String,
    pub risk_percent: f64,
    pub total_impacted: usize,
}
```

### 1.2 Add CLI command
**File:** `src/cli.rs`

Add to `Command` enum:
```rust
BlastScore {
    #[clap(long = "db")]
    db: PathBuf,

    #[clap(long = "symbol")]
    symbol: String,

    #[clap(long = "file")]
    file: Option<String>,

    #[clap(long = "depth", default_value = "3")]
    depth: usize,

    #[clap(long = "output", default_value = "human")]
    output: OutputFormat,
}
```

### 1.3 Add handler
**File:** `src/blast_score_cmd.rs` (new file)

```rust
pub fn run_blast_score(
    db_path: PathBuf,
    symbol: String,
    file: Option<String>,
    depth: usize,
    output: OutputFormat,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let blast_score = compute_blast_score(&mut graph, &symbol, file.as_deref(), depth)?;

    match output {
        OutputFormat::Human => {
            println!("Blast Score: {:.1} ({} direct · {} transitive) [{}]",
                blast_score.score,
                blast_score.direct_count,
                blast_score.transitive_count,
                blast_score.risk_level
            );
        }
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = serde_json::json!({
                "command": "blast-score",
                "data": blast_score,
            });
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    }

    Ok(())
}
```

### 1.4 Wire command routing
**File:** `src/main.rs`

Add to match statement:
```rust
Command::BlastScore { db, symbol, file, depth, output } => {
    blast_score_cmd::run_blast_score(db, symbol, file, depth, output)?;
}
```

**Verification:**
```bash
cargo test
magellan blast-score --db ~/.magellan/magellan/magellan.db --symbol CodeGraph
# Expected: "Blast Score: X.X (N direct · M transitive) [HIGH/MEDIUM/LOW]"
```

---

## Phase 2: Symbol Map Export (llmgrep)

**Objective:** Add `llmgrep export-symbols --output symbolindex.json`

**Implementation:**

### 2.1 Add export command to llmgrep
**File:** `llmgrep/src/commands/mod.rs` (add new module)

```rust
pub mod export_symbols;
```

### 2.2 Implement export
**File:** `llmgrep/src/commands/export_symbols.rs` (new file)

```rust
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::collections::HashMap;

#[derive(clap::Parser)]
pub struct ExportSymbolsCommand {
    #[clap(long = "db")]
    pub db: PathBuf,

    #[clap(long = "output", default_value = "symbolindex.json")]
    pub output: PathBuf,
}

pub fn run(cmd: ExportSymbolsCommand) -> Result<()> {
    let graph = CodeGraph::open(&cmd.db)?;

    // Build symbol map: {"symbol_name": {"file": "path", "line": 42}}
    let mut symbol_map: HashMap<String, SymbolLocation> = HashMap::new();

    for symbol in graph.all_symbols()? {
        let key = if symbol.file.is_empty() {
            symbol.display_fqn.clone()
        } else {
            format!("{}:{}", symbol.display_fqn, symbol.file)
        };

        symbol_map.insert(key, SymbolLocation {
            file: symbol.file,
            line: symbol.start_line,
            kind: symbol.kind,
        });
    }

    // Write JSON
    let json = serde_json::to_string_pretty(&symbol_map)?;
    let mut file = File::create(&cmd.output)?;
    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;

    eprintln!("Exported {} symbols to {}", symbol_map.len(), cmd.output.display());

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct SymbolLocation {
    file: String,
    line: usize,
    kind: String,
}
```

### 2.3 Wire into CLI
**File:** `llmgrep/src/main.rs`

Add to Commands enum:
```rust
ExportSymbols(export_symbols::ExportSymbolsCommand),
```

Add to match:
```rust
Commands::ExportSymbols(cmd) => {
    export_symbols::run(cmd)?;
}
```

**Verification:**
```bash
llmgrep export-symbols --db ~/.magellan/magellan/magellan.db --output /tmp/symbolindex.json
jq '. | length' /tmp/symbolindex.json
# Expected: ~3500 symbols
jq '."CodeGraph"' /tmp/symbolindex.json
# Expected: {"file": "src/graph/mod.rs", "line": 123, "kind": "Class"}
```

---

## Phase 3: Impact Export (magellan)

**Objective:** Add `magellan export-impact --output impact.json`

**Implementation:**

### 3.1 Add export format
**File:** `src/export_cmd.rs`

Add to `ExportFormat` enum:
```rust
Impact,  // Impact analysis with blast scores
```

### 3.2 Add impact export handler
**File:** `src/export_cmd.rs` (in run_export)

Add branch:
```rust
ExportFormat::Impact => {
    let impacted = match filters.symbol {
        Some(ref symbol_name) => {
            graph.context_impact(symbol_name, filters.file.as_deref(), depth)?
        }
        None => anyhow::bail!("Impact export requires --symbol <name>"),
    };

    let blast_score = compute_blast_score(
        &mut graph,
        &impacted.symbol_name,
        impacted.file.as_deref(),
        depth,
    )?;

    let export_data = serde_json::json!({
        "version": "2.0.0",
        "target": impacted.target,
        "depth_limit": depth,
        "blast_score": {
            "score": blast_score.score,
            "direct_count": blast_score.direct_count,
            "transitive_count": blast_score.transitive_count,
            "risk_level": blast_score.risk_level,
            "risk_percent": blast_score.risk_percent,
            "total_impacted": blast_score.total_impacted,
        },
        "impacted": impacted.symbols,
    });

    match output {
        Some(ref path) => {
            let mut file = File::create(path)?;
            file.write_all(serde_json::to_string_pretty(&export_data)?.as_bytes())?;
            print_export_summary(path, ExportFormat::Impact, &mut graph)?;
        }
        None => {
            println!("{}", serde_json::to_string_pretty(&export_data)?);
        }
    }
}
```

**Verification:**
```bash
magellan export --db ~/.magellan/magellan/magellan.db --format impact --symbol CodeGraph --output /tmp/impact.json
jq '.blast_score' /tmp/impact.json
# Expected: score, direct_count, transitive_count, risk_level
```

---

## Phase 4: Repo-Root Discovery Convention

**Objective:** Establish `.magellan/` directory convention for repo-root exports

**Implementation:**

### 4.1 Add repo-root detection
**File:** `src/lib.rs` (new utility function)

```rust
use std::path::{Path, PathBuf};

/// Find repository root by looking for .git directory
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = start;

    loop {
        let git_dir = current.join(".git");
        if git_dir.is_dir() || git_dir.is_file() {
            return Some(current.to_path_buf());
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Get or create .magellan directory in repo root
pub fn magellan_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".magellan")
}
```

### 4.2 Update export commands to use repo-root
**File:** `src/export_cmd.rs` (modify run_export)

When output is None, default to repo-root:
```rust
let output_path = match output {
    Some(ref path) => path.clone(),
    None => {
        let repo_root = find_repo_root(&db_path)
            .ok_or_else(|| anyhow!("Cannot find repository root"))?;
        let mag_dir = magellan_dir(&repo_root);

        std::fs::create_dir_all(&mag_dir)?;

        match format {
            ExportFormat::Json => mag_dir.join("export.json"),
            ExportFormat::Impact => mag_dir.join("impact.json"),
            _ => anyhow::bail!("Format requires explicit --output for repo-root export"),
        }
    }
};
```

### 4.3 Update llmgrep export-symbols
**File:** `llmgrep/src/commands/export_symbols.rs`

```rust
let output_path = if cmd.output == PathBuf::from("symbolindex.json") {
    let repo_root = find_repo_root(&cmd.db)?;
    let mag_dir = magellan_dir(&repo_root);
    std::fs::create_dir_all(&mag_dir)?;
    mag_dir.join("symbolindex.json")
} else {
    cmd.output
};
```

**Verification:**
```bash
cd /home/feanor/Projects/magellan
magellan export --db ~/.magellan/magellan/magellan.db --format json
# Expected: .magellan/export.json created

llmgrep export-symbols --db ~/.magellan/magellan/magellan.db
# Expected: .magellan/symbolindex.json created

ls -la .magellan/
# Expected: export.json, symbolindex.json files
```

---

## Phase 5: Pre-Commit Hook Infrastructure

**Objective:** Add `magellan install-hook --threshold 10`

**Implementation:**

### 5.1 Add hook command
**File:** `src/cli.rs`

```rust
InstallHook {
    #[clap(long = "threshold", default_value = "10")]
    threshold: f64,

    #[clap(long = "strict")]
    strict: bool,
}
```

### 5.2 Implement hook installer
**File:** `src/hook_cmd.rs` (new file)

```rust
use anyhow::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn run_install_hook(threshold: f64, strict: bool) -> Result<()> {
    let repo_root = find_repo_root(&std::env::current_dir()?)
        .ok_or_else(|| anyhow!("Not in a git repository"))?;

    let hooks_dir = repo_root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join("pre-commit");

    let hook_script = format!(
        r#"#!/bin/bash
# Magellan blast-score pre-commit hook
# Threshold: {:.1} (strict: {})

set -e

STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM)

if [ -z "$STAGED_FILES" ]; then
    exit 0
fi

# Find affected symbols in staged files
for FILE in $STAGED_FILES; do
    # Extract symbols from file (simplified - real version needs AST)
    SYMBOLS=$(magellan query --db ~/.magellan/magellan/magellan.db --file "$FILE" --output json | jq -r '.symbols[].name')

    for SYMBOL in $SYMBOLS; do
        SCORE=$(magellan blast-score --db ~/.magellan/magellan/magellan.db --symbol "$SYMBOL" --output json | jq -r '.data.score')

        if (( $(echo "$SCORE > {0}" | bc -l) )); then
            echo "WARNING: '$SYMBOL' has blast score $SCORE (threshold: {0})"
            echo "Run 'magellan context impact --symbol $SYMBOL' for full analysis"

            if [ "{1}" = "true" ]; then
                echo "Blocking commit due to strict mode"
                exit 1
            fi
        fi
    done
done

exit 0
"#,
        threshold, strict, threshold, strict
    );

    fs::write(&hook_path, hook_script)?;

    // Make executable
    let mut perms = fs::metadata(&hook_path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms)?;

    println!("Installed pre-commit hook (threshold: {:.1}, strict: {})", threshold, strict);
    println!("Hook location: {}", hook_path.display());

    Ok(())
}
```

### 5.3 Wire into main
**File:** `src/main.rs`

```rust
Command::InstallHook { threshold, strict } => {
    hook_cmd::run_install_hook(threshold, strict)?;
}
```

**Verification:**
```bash
magellan install-hook --threshold 10
cat .git/hooks/pre-commit
# Expected: Hook script with blast-score checks

git commit -m "test"  # with high-blast file staged
# Expected: Warning or block (if --strict)
```

---

## Phase 6: Documentation & Integration

**Objective:** Update CLAUDE.md, add usage examples

### 6.1 Update grounded-coding-tools skill
**File:** `~/.claude/skills/grounded-coding-tools/SKILL.md`

Add section:
```markdown
## Blast Scoring & Impact

```bash
# Single blast score (codeindex-style)
magellan blast-score --db <db> --symbol "<name>" --depth 3
# Returns: "Blast Score: 8.5 (2 direct · 7 transitive) [HIGH]"

# Impact export to repo-root
magellan export --db <db> --format impact --symbol "<name>"
# Writes: .magellan/impact.json

# Symbol map export
llmgrep export-symbols --db <db>
# Writes: .magellan/symbolindex.json

# Pre-commit hook
magellan install-hook --threshold 10
# Warns/block commits exceeding threshold
```
```

### 6.2 Add magellan doctor check
**File:** `src/doctor_cmd.rs`

Add check for repo-root exports:
```rust
// Check 15: Repo-root exports
let repo_root = find_repo_root(&db_path);
if let Some(root) = repo_root {
    let mag_dir = magellan_dir(&root);

    let symbol_index = mag_dir.join("symbolindex.json");
    if !symbol_index.exists() {
        checks.push(CheckResult {
            name: "Repo-root symbol index".to_string(),
            status: "missing".to_string(),
            message: Some("Run: llmgrep export-symbols --db <db>".to_string()),
            fix_hint: None,
        });
        issues_found += 1;
    }

    let impact_json = mag_dir.join("impact.json");
    // Optional: impact.json is per-symbol, not always present
}
```

### 6.3 Test workflow
```bash
# Full workflow test
cd ~/Projects/magellan
magellan blast-score --db ~/.magellan/magellan/magellan.db --symbol CodeGraph
llmgrep export-symbols --db ~/.magellan/magellan/magellan.db
magellan install-hook --threshold 5 --strict
git commit -m "test high-blast change"  # Should block
```

---

## Implementation Priority

**Phase 1 (Blast Score):** Highest priority. Foundation for everything else. Easy win from existing `impact_analysis`.

**Phase 2 (Symbol Export):** High priority. Enables O(1) lookups. Simple export from existing symbol index.

**Phase 3 (Impact Export):** Medium priority. Depends on Phase 1. Nice-to-have for team sharing.

**Phase 4 (Repo-Root):** High priority. UX improvement. Makes exports discoverable. Low complexity.

**Phase 5 (Pre-Commit):** Medium priority. Safety enforcement. Complex shell integration. Can defer.

**Phase 6 (Docs):** Low priority. Documentation follows implementation.

---

## Risk Assessment

**From graph analysis:**
- `run_export` has 75 blocks, complex control flow (mirage cfg shows 76 blocks with many switches)
- `impact_analysis` uses BFS with visited set - safe, no cycles
- No dead cycles detected in export/impact code paths

**Implementation risks:**
- Blast score formula differs from codeindex (direct + 0.5*transitive vs unknown) - verify semantics
- Pre-commit hook shell script escape issues - test with symbols containing quotes
- Repo-root detection fails for worktrees - handle gracefully

**Mitigation:**
- Start with Phase 1 (blast score) - isolated, testable
- Add tests for score computation edge cases (0 direct, 0 transitive)
- Test pre-commit hook with staged files containing special characters

---

## Success Criteria

**Phase 1:**
```bash
magellan blast-score --db ~/.magellan/magellan/magellan.db --symbol navigate_cmd
# Output: "Blast Score: X.X (N direct · M transitive) [HIGH/MEDIUM/LOW]"
```

**Phase 2:**
```bash
llmgrep export-symbols --db ~/.magellan/magellan/magellan.db
ls .magellan/symbolindex.json
# File exists, valid JSON, jq queries work
```

**Phase 4:**
```bash
cd /repo/root
magellan export --db ~/.magellan/magellan/magellan.db --format json
ls .magellan/export.json
# File created in repo root, not absolute path
```

**Phase 5:**
```bash
magellan install-hook --threshold 5
# Hook installed, executable
git commit -m "test"  # with high-blast change
# Blocks or warns based on --strict flag
```

---

## Future Enhancements (Out of Scope)

- MCP server exposing blast-score/lookup tools
- Watch mode auto-updating .magellan/ exports
- Web UI showing blast scores per symbol
- Integration with envoy/atheneum for blast-score history
- Per-file blast scores (aggregated from symbols)
