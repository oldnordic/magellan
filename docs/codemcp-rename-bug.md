# Bug: Reference Edges Missing Byte Offsets

## Summary

`graph_edges` with `edge_type = 'REFERENCES'` are created but **do not store byte offsets** in the edge data. This prevents rename refactoring from finding and updating all usages of a symbol.

## Current Behavior

### Entities (✅ working)
```sql
SELECT name, json_extract(data, '$.byte_start'), json_extract(data, '$.byte_end')
FROM graph_entities WHERE name = 'OldStruct';

-- Result: OldStruct | 987 | 1031  ✅ Byte offsets stored
```

### Edges (❌ broken)
```sql
SELECT edge_type, json_extract(data, '$.byte_start'), json_extract(data, '$.byte_end')
FROM graph_edges WHERE edge_type = 'REFERENCES';

-- Result: REFERENCES | |  ❌ Byte offsets are NULL/empty
```

## Expected Behavior

When `index_references()` is called, each `REFERENCES` edge should store:
- `byte_start`: Position where the reference starts in the source file
- `byte_end`: Position where the reference ends

Example edge data:
```json
{
  "byte_start": 1234,
  "byte_end": 1243,
  "start_line": 42,
  "start_col": 8,
  "end_line": 42,
  "end_col": 16
}
```

## Root Cause

The `index_references()` method in `CodeGraph` creates edges but doesn't populate the `data` field with byte offset information.

## Impact

The `codemcp` `refactor_rename` tool relies on:
1. Finding the definition via `graph_entities` ✅ works
2. Finding all references via `graph_edges` WHERE edge_type='REFERENCES' ❌ no byte offsets
3. Without byte offsets, references are filtered out and only the definition is renamed

## Files to Fix

Likely in `magellan/src/lib.rs` or similar:
- `index_references()` function
- Edge creation logic needs to capture and store reference positions

## Verification Query

```sql
-- Check if bug is fixed:
SELECT
    to_entity.name as symbol,
    COUNT(*) as ref_count,
    SUM(CASE WHEN json_extract(edge.data, '$.byte_start') IS NOT NULL THEN 1 ELSE 0 END) as with_offsets
FROM graph_edges edge
JOIN graph_entities to_entity ON edge.to_id = to_entity.id
WHERE edge.edge_type = 'REFERENCES'
GROUP BY to_entity.name;
```

After fix: `with_offsets` should equal `ref_count` for all symbols.
