# Unified JSON Schema Gaps (Magellan, Splice, LLM Tools)

**Purpose:** Track current mismatches vs `/home/feanor/Projects/magellan/docs/UNIFIED_JSON_SCHEMA.md` so each tool can be aligned to a shared, auditable JSON substrate.
**Scope:** magellan, splice, llmsearch, llmastsearch, llm-discover, llm-transform, llmdocs, llm-file-write

## Sources Read
- /home/feanor/Projects/magellan/docs/UNIFIED_JSON_SCHEMA.md
- /home/feanor/Projects/splice/src/output.rs
- /home/feanor/Projects/llmsearch/src/main.rs
- /home/feanor/Projects/llmastsearch/src/query/result.rs
- /home/feanor/Projects/llmastsearch/src/query/ast.rs
- /home/feanor/Projects/llmastsearch/src/query/position.rs
- /home/feanor/Projects/llmdiscover/src/main.rs
- /home/feanor/Projects/llmtransform/llm-transform/src/json.rs

## Unified Schema Requirements (Baseline)
- Top-level wrapper: `schema_version`, `execution_id`, `data`, `tool`, `timestamp`.
- Span model: half-open `[byte_start, byte_end)`, UTF-8 byte offsets, line 1-indexed, column 0-indexed.
- Stable IDs: `execution_id` UUID v4, `span_id` deterministic hash of `file_path:byte_start:byte_end`.

## Gaps by Tool

### Splice
- Wrapper uses `version` + `operation_id` + `result`; schema expects `schema_version` + `execution_id` + `data` + `tool` + `timestamp`.
- Span fields are `line_start/line_end`, `col_start/col_end`; schema expects `start_line/end_line`, `start_col/end_col`.
- `span_id` is random UUID; schema expects deterministic hash.
- Output mixes tool-specific fields inside span without standardized wrapper.

### llmsearch
- No wrapper fields (`schema_version`, `tool`, `timestamp`, `data`).
- `file` vs `file_path` naming mismatch.
- `line_number`/`column_number` use Unicode codepoint columns; schema requires UTF-8 byte columns.
- No `span` object and no `span_id`.

### llmastsearch
- No unified wrapper; emits raw `QueryResult`.
- Uses `Position { row, column }` embedded in `Match`, not a standardized `Span`.
- No deterministic `span_id`.

### llm-discover
- No wrapper fields (`schema_version`, `tool`, `timestamp`, `data`).
- No `match_id` per result.
- `path` naming is ok but not standardized across tools.

### llm-transform
- No unified wrapper; emits `EditRequest`/`EditResponse` only.
- No `schema_version` or `tool` fields.
- No deterministic `span_id` for edits.

### llmdocs / llm-file-write
- Design docs exist, but no implementation yet to validate schema compliance.

## Alignment Checklist (Apply Everywhere)
1. **Wrapper:** Add `schema_version`, `execution_id`, `tool`, `timestamp`, `data` to all JSON outputs.
2. **Span:** Use canonical `Span` fields and half-open ranges with UTF-8 byte offsets.
3. **Stable IDs:** Implement deterministic `span_id` hashing for all span-carrying results.
4. **Naming:** Standardize to `file_path`, `start_line`, `start_col`, `end_line`, `end_col`.
5. **Errors:** Use unified error wrapper with structured error codes and remediation hints.
6. **Backward Compat:** If needed, emit legacy fields as deprecated aliases during transition.

## Suggested Order
1. Splice (highest impact on editing safety)
2. llmsearch + llmastsearch (primary discovery tools)
3. llm-discover + llm-transform
4. llmdocs + llm-file-write (when implementations begin)
