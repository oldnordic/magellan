# Phase 9: Interop Export - Context

**Gathered:** 2026-01-19
**Status:** Ready for planning

## Phase Boundary

Export functionality that transforms Magellan's indexed graph into standard interoperable index formats (SCIP and LSIF) for consumption by external tools:
- Export to SCIP format with documented position encoding
- Export to LSIF format with documented position encoding
- Symbol identity rules documented (how Magellan's symbol_id maps to SCIP/LSIF symbols)
- Standard consumers can parse exported artifacts without format errors

---

## Implementation Decisions

### Format Scope
- Implement SCIP export first (more widely adopted, simpler schema)
- LSIF export can be deferred to v2 or implemented in same phase if time permits
- SCIP format: Source Code Intelligence Protocol (sourcegraph.com)
- Use existing export command pattern from Phase 7 (--format flag)

### Symbol Identity
- **SCIP symbol format**: `language:path#symbol` for fully-qualified symbols
- Map Magellan's `symbol_id` (SHA-256 hash) to SCIP symbols by name lookup
- For v1: use symbol name + file path as SCIP symbol (stable enough for cross-reference)
- Document the mapping scheme in code comments and user-facing docs

### Data Mapping
- **Symbols**: Map to SCIP `Symbol` (name, kind, documentation)
- **References**: Map to SCIP `Relationship` (symbol, reference_kind)
- **Spans**: Map to SCIP `Range` (start_line, start_character, end_line, end_character)
- **Files**: Map to SCIP `Document` (relative_path, language)
- **Magellan-specific fields**: Omit or extend with custom metadata (prefer omit for v1)

### CLI Integration
- Extend existing `export` command from Phase 7
- Add `--format scip` option (alongside json/jsonl/dot/csv)
- Output to stdout or file via `-o` flag (consistent with other exports)
- Add `scip` alias command for direct access

### Claude's Discretion
- Exact SCIP schema version to target (v1 or latest)
- How to handle Magellan's span model (half-open) vs SCIP (inclusive ranges)
- Whether to include implementation references
- Metadata fields (documentation, etc.) - include if readily available

---

## Specific Ideas

No specific requirements — follow SCIP specification and existing export patterns from Phase 7.

---

## Deferred Ideas

- LSIF export (defer to v2 unless time permits)
- SCIP index format (vs export format)
- Custom metadata extensions
- Multi-file index artifacts

---

*Phase: 09-interop-export*
*Context gathered: 2026-01-19*
