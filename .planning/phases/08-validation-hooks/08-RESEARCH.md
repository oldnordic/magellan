# Phase 8: Validation Hooks (Pre/Post) - Research

**Researched:** 2026-01-19
**Domain:** Graph validation, JSON diagnostics, SQLite invariants
**Confidence:** HIGH

## Summary

Phase 8 implements validation hooks for verifying indexing correctness with structured JSON diagnostics. The implementation builds on existing Magellan infrastructure: `execution_log` for run tracking, `JsonResponse` contract for output, and sqlitegraph for edge queries.

**Primary recommendation:** Use a new `src/graph/validation.rs` module with `ValidationReport` struct following the existing `VerifyReport` pattern, extend `JsonResponse<T>` with `ValidationResponse` type, and add CLI flags `--validate` and `--validate-only` to existing commands.

## Standard Stack

The validation system uses existing Magellan infrastructure with minimal additions:

| Component | Version/Source | Purpose |
|-----------|----------------|---------|
| `rusqlite` | 0.31.0 (existing) | SQL queries for orphan detection |
| `serde` | 1.0.228 (existing) | JSON serialization |
| `serde_json` | 1.0.147 (existing) | JSON output format |
| `anyhow` | 1.0.100 (existing) | Error handling |
| `execution_log` | existing module | Run tracking via execution_id |
| `JsonResponse<T>` | existing contract | JSON output wrapper |

**No new dependencies required.** The phase uses:
- Existing `sqlitegraph` edge query API (`neighbors()`, `entity_ids()`)
- Existing `JsonResponse` pattern from `src/output/command.rs`
- Existing `ExecutionTracker` wrapper from `src/main.rs`

### JSON Output Contract (Existing)

The existing `JsonResponse<T>` contract (lines 146-174 in `src/output/command.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonResponse<T> {
    pub schema_version: String,    // "1.0.0" constant
    pub execution_id: String,       // From output::generate_execution_id()
    pub data: T,
    pub partial: Option<bool>,
}
```

**All validation responses MUST use this wrapper.**

## Architecture Patterns

### Module Structure

Create new validation module alongside existing graph modules:

```
src/graph/
├── validation.rs        # NEW: Validation checks, ValidationReport
├── execution_log.rs     # EXISTING: Already has execution_id tracking
├── references.rs        # EXISTING: Reference node patterns
├── call_ops.rs          # EXISTING: Call node patterns
└── schema.rs            # EXISTING: Node/edge payloads
```

### Validation Module Pattern

Follow the existing `verify.rs` pattern (`src/verify.rs`):

```rust
//! Graph validation for pre/post indexing checks

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::graph::{CodeGraph, SymbolNode, ReferenceNode, CallNode};

/// Validation result with structured errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub passed: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,              // Machine-readable error class
    pub message: String,           // Human-readable description
    pub entity_id: Option<String>, // Related stable symbol_id
    pub details: serde_json::Value, // Additional structured data
}

/// Run all validation checks
pub fn validate_graph(graph: &mut CodeGraph) -> Result<ValidationReport> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Post-run checks
    errors.extend(check_orphan_references(graph)?);
    errors.extend(check_orphan_calls(graph)?);
    errors.extend(check_dangling_edges(graph)?);
    warnings.extend(check_file_consistency(graph)?);

    Ok(ValidationReport {
        passed: errors.is_empty(),
        errors,
        warnings,
    })
}
```

### Pre-Run Validation Pattern

Pre-run checks execute BEFORE indexing starts. These are lightweight "sanity" checks:

```rust
/// Pre-run validation: input manifest checks
pub fn pre_run_validate(
    db_path: &std::path::Path,
    root_path: &std::path::Path,
    input_paths: &[std::path::PathBuf],
) -> Result<PreValidationReport> {
    let mut errors = Vec::new();

    // Check 1: Database file is accessible
    if !db_path.exists() {
        // May be OK for first run, but parent dir must exist
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                errors.push(ValidationError {
                    code: "DB_PARENT_MISSING".to_string(),
                    message: format!("Database parent directory does not exist: {}", parent.display()),
                    entity_id: None,
                    details: serde_json::json!({"path": parent.to_string_lossy()}),
                });
            }
        }
    }

    // Check 2: Root path exists and is readable
    if !root_path.exists() {
        errors.push(ValidationError {
            code: "ROOT_PATH_MISSING".to_string(),
            message: format!("Root path does not exist: {}", root_path.display()),
            entity_id: None,
            details: serde_json::json!({"path": root_path.to_string_lossy()}),
        });
    }

    // Check 3: Input paths exist
    for path in input_paths {
        if !path.exists() {
            errors.push(ValidationError {
                code: "INPUT_PATH_MISSING".to_string(),
                message: format!("Input path does not exist: {}", path.display()),
                entity_id: None,
                details: serde_json::json!({"path": path.to_string_lossy()}),
            });
        }
    }

    Ok(PreValidationReport {
        passed: errors.is_empty(),
        errors,
        input_count: input_paths.len(),
    })
}
```

### Post-Run Validation: Orphan Detection

Orphan detection uses sqlitegraph's edge query API. Pattern from `src/graph/references.rs`:

```rust
/// Check for orphan references (REFERENCES edges pointing to non-existent symbols)
fn check_orphan_references(graph: &mut CodeGraph) -> Result<Vec<ValidationError>> {
    use sqlitegraph::{BackendDirection, NeighborQuery};

    let mut errors = Vec::new();
    let backend = &graph.references.backend;

    // Get all Reference nodes
    let entity_ids = backend.entity_ids()?;
    for ref_id in entity_ids {
        let node = backend.get_node(ref_id)?;
        if node.kind != "Reference" {
            continue;
        }

        // Check if REFERENCES edge points to valid target
        let neighbors = backend.neighbors(
            ref_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("REFERENCES".to_string()),
            },
        )?;

        if neighbors.is_empty() {
            // This Reference has no target - orphan!
            let ref_node: ReferenceNode = serde_json::from_value(node.data)
                .unwrap_or_else(|_| ReferenceNode {
                    file: "unknown".to_string(),
                    byte_start: 0,
                    byte_end: 0,
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 0,
                });

            errors.push(ValidationError {
                code: "ORPHAN_REFERENCE".to_string(),
                message: format!("Reference at {}:{}:{} has no target symbol",
                    ref_node.file, ref_node.start_line, ref_node.start_col),
                entity_id: None,
                details: serde_json::json!({
                    "file": ref_node.file,
                    "line": ref_node.start_line,
                    "col": ref_node.start_col,
                }),
            });
        }
    }

    Ok(errors)
}
```

### Post-Run Validation: Orphan Calls

Same pattern for CALLS/CALLER edges. From `src/graph/call_ops.rs`:

```rust
/// Check for orphan calls (caller or callee symbol missing)
fn check_orphan_calls(graph: &mut CodeGraph) -> Result<Vec<ValidationError>> {
    use sqlitegraph::{BackendDirection, NeighborQuery};

    let mut errors = Vec::new();
    let backend = &graph.calls.backend;

    let entity_ids = backend.entity_ids()?;
    for call_id in entity_ids {
        let node = backend.get_node(call_id)?;
        if node.kind != "Call" {
            continue;
        }

        // Check both ends of the call chain
        let caller_neighbors = backend.neighbors(
            call_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: Some("CALLER".to_string()),
            },
        )?;

        let callee_neighbors = backend.neighbors(
            call_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
        )?;

        if caller_neighbors.is_empty() {
            errors.push(ValidationError {
                code: "ORPHAN_CALL_NO_CALLER".to_string(),
                message: format!("Call node {} has no caller symbol", call_id),
                entity_id: None,
                details: serde_json::json!({"call_node_id": call_id}),
            });
        }

        if callee_neighbors.is_empty() {
            errors.push(ValidationError {
                code: "ORPHAN_CALL_NO_CALLEE".to_string(),
                message: format!("Call node {} has no callee symbol", call_id),
                entity_id: None,
                details: serde_json::json!({"call_node_id": call_id}),
            });
        }
    }

    Ok(errors)
}
```

### CLI Integration Pattern

Use existing `ExecutionTracker` wrapper from `src/main.rs` (lines 843-908):

```rust
// In src/main.rs parse_args()
let validate = matches.contains(&"--validate".to_string());
let validate_only = matches.contains(&"--validate-only".to_string());

// In command handler
let mut tracker = ExecutionTracker::new(args, root, db_path);
tracker.start(&graph)?;

// Pre-run validation
if validate || validate_only {
    let pre_report = magellan::graph::validation::pre_run_validate(
        &db_path, &root_path, &input_paths
    )?;
    if !pre_report.passed {
        tracker.set_error("Pre-validation failed".to_string());
        tracker.finish(&graph)?;
        output_validation_error(&pre_report, &tracker.exec_id)?;
        return ExitCode::from(1);
    }
}

if validate_only {
    // Run post-validation on existing DB and exit
    let post_report = magellan::graph::validation::validate_graph(&mut graph)?;
    output_validation_result(&post_report, &tracker.exec_id)?;
    return if post_report.passed { ExitCode::SUCCESS } else { ExitCode::from(1) };
}

// ... normal indexing ...

// Post-run validation
if validate {
    let post_report = magellan::graph::validation::validate_graph(&mut graph)?;
    if !post_report.passed {
        tracker.set_error(format!("Post-validation failed: {} errors", post_report.errors.len()));
        output_validation_result(&post_report, &tracker.exec_id)?;
        tracker.finish(&graph)?;
        return ExitCode::from(1);
    }
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization | Custom `write!` macros | `serde_json::to_string_pretty` | Standard, handles escaping |
| Execution IDs | Random UUID | `output::generate_execution_id()` | Consistent with existing logs |
| Edge queries | Raw SQL via `conn.execute()` | `backend.neighbors()` with `NeighborQuery` | Uses sqlitegraph indexes |
| Error handling | Custom error enums | `anyhow::Result` | Existing pattern throughout codebase |
| File hashing | Custom hash | Existing `graph.files.compute_hash()` | Consistent with FileNode.hash |

**Key insight:** The existing `VerifyReport` in `src/verify.rs` already implements a similar pattern for filesystem validation. Re-use this structure, extend it for graph invariants.

## Common Pitfalls

### Pitfall 1: Confusing "Orphan Rule" (Rust) with "Orphan Edges" (Graph)

**What goes wrong:** WebSearch results about Rust's orphan rule (foreign trait implementations) are unrelated to graph orphan detection.

**Why it happens:** Search query overlap on "orphan rust".

**How to avoid:**
- "Orphan edge" = edge whose source or target node doesn't exist
- "Orphan rule" = Rust trait coherence rule
- For this phase, we only care about orphan edges in the graph

### Pitfall 2: Running Validation During Every Index

**What goes wrong:** Validation is O(V+E) - checking every edge is expensive on large graphs.

**Why it happens:** Developer habit of making checks "always on".

**How to avoid:** Validation is OPT-IN (`--validate` flag). Never run validation by default in indexing path. The `--validate-only` flag allows validation without indexing.

### Pitfall 3: Not Sorting Results Before Output

**What goes wrong:** Non-deterministic JSON output makes testing impossible.

**Why it happens:** `HashMap` iteration order is non-deterministic.

**How to avoid:**
```rust
// BAD: errors may appear in any order
return ValidationReport { errors, warnings };

// GOOD: sort for deterministic output
errors.sort_by(|a, b| a.code.cmp(&b.code));
warnings.sort_by(|a, b| a.code.cmp(&b.code));
return ValidationReport { errors, warnings };
```

### Pitfall 4: Exiting Non-Zero Without Structured Output

**What goes wrong:** Scripts can't parse error details if exit code 1 prints to stderr only.

**Why it happens:** Human-readable errors are easy, JSON requires extra step.

**How to avoid:** When validation fails, ALWAYS output `ValidationResponse` via `JsonResponse` to stdout before exiting non-zero.

### Pitfall 5: Not Tying Results to execution_id

**What goes wrong:** Validation results can't be correlated with specific runs.

**Why it happens:** Forgetting to use `ExecutionTracker`.

**How to avoid:**
```rust
let response = JsonResponse::new(validation_response, &tracker.exec_id);
output_json(&response)?;
```

## Code Examples

### JsonResponse Output (Existing Pattern Verified)

From `src/query_cmd.rs` line 335:

```rust
let json_response = JsonResponse::new(response, exec_id);
output_json(&json_response)?;
```

**Validation output follows this exact pattern:**

```rust
use magellan::output::{JsonResponse, output_json};

// In validation command
let response = ValidationResponse {
    passed: report.passed,
    error_count: report.errors.len(),
    errors: report.errors,
    warning_count: report.warnings.len(),
    warnings: report.warnings,
};
let json_response = JsonResponse::new(response, exec_id);
output_json(&json_response)?;
```

### Error Code Naming Convention

Based on Rust community conventions (from WebSearch), use SCREAMING_SNAKE_CASE for error codes:

| Category | Error Code Pattern |
|----------|-------------------|
| Pre-run DB errors | `DB_*` (e.g., `DB_MISSING`, `DB_CORRUPT`) |
| Pre-run path errors | `PATH_*` (e.g., `PATH_NOT_FOUND`, `PATH_NOT_READABLE`) |
| Post-run orphan refs | `ORPHAN_REFERENCE` |
| Post-run orphan calls | `ORPHAN_CALL_*` (e.g., `ORPHAN_CALL_NO_CALLER`, `ORPHAN_CALL_NO_CALLEE`) |
| Post-run edge errors | `DANGLING_EDGE` |
| File consistency | `FILE_CONSISTENCY_*` |

### SQLite Edge Query Pattern

From `src/graph/references.rs` lines 199-214:

```rust
let neighbor_ids = self.backend.neighbors(
    symbol_id,
    NeighborQuery {
        direction: BackendDirection::Incoming,
        edge_type: Some("REFERENCES".to_string()),
    },
)?;
```

**Use this pattern for all orphan detection:**
- `Incoming` + `REFERENCES` = find what references this symbol
- `Outgoing` + `REFERENCES` = find what this reference points to
- Empty result = orphan

### Read-Only File Access Pattern

From `src/graph/freshness.rs` line 92:

```rust
let file_nodes = graph.files.all_file_nodes_readonly()?;
```

**Use read-only access for validation checks** to avoid accidental mutations.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No validation tracking | `execution_log` table | Phase 2 | Enables run correlation |
| Ad-hoc error messages | `JsonResponse<T>` contract | Phase 3 | Structured, parseable output |
| Manual freshness checks | `check_freshness()` in `freshness.rs` | Phase 5 | Automated staleness detection |

**For Phase 8:**
- Pre-run validation: NEW (no existing equivalent)
- Post-run invariants: NEW (no existing equivalent)
- JSON diagnostics: Extends existing `JsonResponse` contract

### Existing Similar Pattern: VerifyReport

From `src/verify.rs` lines 14-36:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub missing: Vec<String>,     // Files in DB but not on FS
    pub new: Vec<String>,         // Files on FS but not in DB
    pub modified: Vec<String>,    // Files with hash mismatch
    pub stale: Vec<String>,       // Files indexed > 5 min ago
}
```

**ValidationReport should follow this pattern:**
- `Serialize, Deserialize` for JSON
- Field accessors (`total_issues()`, `is_clean()`)
- Helper methods for output formatting

## Open Questions

1. **Validation Performance on Large Graphs**
   - What we know: O(V+E) edge queries could be slow on million-edge graphs
   - What's unclear: Acceptable threshold for validation time
   - Recommendation: Start with simple queries, add pagination/batching if needed

2. **Error Code Namespace Collisions**
   - What we know: Using SCREAMING_SNAKE_CASE for codes
   - What's unclear: Whether codes need prefix (e.g., `MAG_`) to avoid conflicts
   - Recommendation: No prefix needed unless integration requires it

3. **Storage of Validation Results**
   - What we know: `execution_log` exists for run tracking
   - What's unclear: Whether validation results need persistence beyond JSON output
   - Recommendation: JSON output is sufficient; defer persistence table to v2 if needed

## Sources

### Primary (HIGH confidence)
- `src/graph/execution_log.rs` - Execution tracking with execution_id
- `src/output/command.rs` lines 146-174 - JsonResponse contract
- `src/verify.rs` - VerifyReport pattern (similar structure)
- `src/graph/references.rs` - Reference node edge query patterns
- `src/graph/call_ops.rs` - Call node edge query patterns
- `src/main.rs` lines 843-908 - ExecutionTracker wrapper
- `src/graph/freshness.rs` - Read-only access pattern

### Secondary (MEDIUM confidence)
- [Rust Error Handling Best Practices](https://medium.com/@Murtza/error-handling-best-practices-in-rust-a-comprehensive-guide-to-building-resilient-applications-46bdf6fa6d9d) - Error naming patterns
- [JSON Schema Validation Errors 2025](https://dataformatterpro.com/blog/complete-json-validation-guide-2025/) - Validation error format best practices
- [Entity/Relationship Graphs: Data Integrity](https://www.researchgate.net/publication/388901196_EntityRelationshipGraphsPrincipledDesignModelingandDataIntegrityManagementofGraphDatabases) - Graph integrity constraints

### Tertiary (LOW confidence)
- [Modelling of Graph Databases](https://jaec.vn/index.php/JAEC/article/viewFile/44/17) - Referential integrity testing
- [API Error Handling Best Practices](https://zuplo.com/learning-center/best-practices-for-api-error-handling) - Error response formats

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All components are existing Magellan infrastructure
- Architecture: HIGH - Pattern follows existing verify.rs and ExecutionTracker
- Pitfalls: HIGH - Based on common mistakes in Rust CLI tools
- Orphan detection queries: HIGH - sqlitegraph API verified in source code

**Research date:** 2026-01-19
**Valid until:** 2026-02-18 (30 days - stack is stable, this is additive)
