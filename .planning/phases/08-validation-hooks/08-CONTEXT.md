# Phase 8: Validation Hooks - Context

**Gathered:** 2026-01-19
**Status:** Ready for planning

## Phase Boundary

Validation system that verifies indexing correctness and surfaces diagnostics via JSON output:
- Pre-run validation: input manifest/checksum summary before indexing
- Post-run validation: invariant checks after indexing
- Structured JSON diagnostics with execution_id correlation
- Non-zero exit on invariant failure with actionable error classes

---

## Implementation Decisions

### Validation Mode
- Opt-in via `--validate` flag (not enabled by default for performance)
- When enabled, runs both pre-run and post-run validation
- Separate `--validate-only` flag to run validation without indexing

### Validation Rules
**Pre-run checks:**
- Database file is readable and valid
- Schema version compatibility
- Input paths exist and are accessible

**Post-run checks:**
- No orphan references (references point to non-existent symbols)
- No orphan calls (caller/callee symbols exist)
- File consistency: all indexed files have corresponding File nodes
- No dangling edges (all edge types reference valid source/target nodes)

### Diagnostics Format
- Follow existing JsonResponse contract (schema_version, execution_id)
- ValidationResponse with:
  - `passed: bool` - overall validation result
  - `errors: Array<ValidationError>` - structured error details
  - `warnings: Array<ValidationWarning>` - non-fatal issues
- ValidationError includes:
  - `code: String` - machine-readable error class
  - `message: String` - human-readable description
  - `entity_id: Option<String>` - related stable ID if applicable
  - `details: Value` - additional structured data

### CLI Integration
- Global `--validate` flag works with all indexing commands (index, watch, status)
- Validation output goes to stdout in JSON mode, stderr in human mode
- Exit code 1 on validation failure, 0 on success
- Results tied to execution_id from execution_log table

### Claude's Discretion
- Exact error code names and format
- Warning vs error classification for specific checks
- Performance optimizations for large graphs
- Whether to support configurable validation rule sets (defer to v2)

---

## Specific Ideas

No specific requirements — open to standard approaches for validation infrastructure.

---

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 08-validation-hooks*
*Context gathered: 2026-01-19*
