# Missing Exports in SQLiteGraph

**Purpose**: Document missing helper functions to be implemented in sqlitegraph
**Status**: Ready for implementation
**SQLiteGraph Version**: 0.2.10
**Date**: 2025-01-01

---

## Problem

During research testing, several helper functions were found to exist in `sqlitegraph/src/index.rs`
but are NOT exported in the public API. This forces users to write raw SQL queries.

---

## Missing Functions to Export

### 1. `get_entities_by_label`

**Current location**: `sqlitegraph/src/index.rs:19-35`

**Signature**:
```rust
pub fn get_entities_by_label(
    graph: &SqliteGraph,
    label: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError>
```

**Purpose**: Get all entities that have a specific label.

**Current implementation** (not exported):
```rust
pub fn get_entities_by_label(
    graph: &SqliteGraph,
    label: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError> {
    let conn = graph.connection();
    let mut stmt = conn
        .prepare_cached("SELECT entity_id FROM graph_labels WHERE label=?1 ORDER BY entity_id")
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let rows = stmt
        .query_map(params![label], |row| row.get(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }
    fetch_entities(graph, ids)
}
```

**Action**: Add to `src/lib.rs` re-exports:
```rust
pub use index::{add_label, get_entities_by_label};
```

---

### 2. `get_entities_by_property`

**Current location**: `sqlitegraph/src/index.rs:53-73`

**Signature**:
```rust
pub fn get_entities_by_property(
    graph: &SqliteGraph,
    key: &str,
    value: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError>
```

**Purpose**: Get all entities with a specific property key-value pair.

**Current implementation** (not exported):
```rust
pub fn get_entities_by_property(
    graph: &SqliteGraph,
    key: &str,
    value: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError> {
    let conn = graph.connection();
    let mut stmt = conn
        .prepare_cached(
            "SELECT entity_id FROM graph_properties \
             WHERE key=?1 AND value=?2 ORDER BY entity_id",
        )
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let rows = stmt
        .query_map(params![key, value], |row| row.get(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }
    fetch_entities(graph, ids)
}
```

**Action**: Add to `src/lib.rs` re-exports:
```rust
pub use index::{add_property, get_entities_by_property};
```

---

## New Functions to Add

### 3. `labels_for_entity`

**Purpose**: Get all labels for a specific entity.

**Proposed signature**:
```rust
pub fn labels_for_entity(
    graph: &SqliteGraph,
    entity_id: i64,
) -> Result<Vec<String>, SqliteGraphError>
```

**Proposed implementation**:
```rust
pub fn labels_for_entity(
    graph: &SqliteGraph,
    entity_id: i64,
) -> Result<Vec<String>, SqliteGraphError> {
    let conn = graph.connection();
    let mut stmt = conn
        .prepare_cached("SELECT label FROM graph_labels WHERE entity_id=?1 ORDER BY label")
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let rows = stmt
        .query_map(params![entity_id], |row| row.get::<_, String>(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let mut labels = Vec::new();
    for row in rows {
        labels.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }
    Ok(labels)
}
```

**Add to**: `src/index.rs`

---

### 4. `properties_for_entity`

**Purpose**: Get all properties for a specific entity.

**Proposed signature**:
```rust
pub fn properties_for_entity(
    graph: &SqliteGraph,
    entity_id: i64,
) -> Result<Vec<(String, String)>, SqliteGraphError>
```

**Proposed implementation**:
```rust
pub fn properties_for_entity(
    graph: &SqliteGraph,
    entity_id: i64,
) -> Result<Vec<(String, String)>, SqliteGraphError> {
    let conn = graph.connection();
    let mut stmt = conn
        .prepare_cached(
            "SELECT key, value FROM graph_properties \
             WHERE entity_id=?1 ORDER BY key"
        )
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let rows = stmt
        .query_map(params![entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let mut props = Vec::new();
    for row in rows {
        props.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }
    Ok(props)
}
```

**Add to**: `src/index.rs`

---

### 5. `remove_label`

**Purpose**: Remove a specific label from an entity.

**Proposed signature**:
```rust
pub fn remove_label(
    graph: &SqliteGraph,
    entity_id: i64,
    label: &str,
) -> Result<(), SqliteGraphError>
```

**Proposed implementation**:
```rust
pub fn remove_label(
    graph: &SqliteGraph,
    entity_id: i64,
    label: &str,
) -> Result<(), SqliteGraphError> {
    graph
        .connection()
        .execute(
            "DELETE FROM graph_labels WHERE entity_id=?1 AND label=?2",
            params![entity_id, label],
        )
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    Ok(())
}
```

**Add to**: `src/index.rs`

---

### 6. `remove_property`

**Purpose**: Remove a specific property from an entity.

**Proposed signature**:
```rust
pub fn remove_property(
    graph: &SqliteGraph,
    entity_id: i64,
    key: &str,
) -> Result<(), SqliteGraphError>
```

**Proposed implementation**:
```rust
pub fn remove_property(
    graph: &SqliteGraph,
    entity_id: i64,
    key: &str,
) -> Result<(), SqliteGraphError> {
    graph
        .connection()
        .execute(
            "DELETE FROM graph_properties WHERE entity_id=?1 AND key=?2",
            params![entity_id, key],
        )
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    Ok(())
}
```

**Add to**: `src/index.rs`

---

### 7. `get_labels_by_prefix`

**Purpose**: Get all unique labels that start with a prefix (useful for autocomplete).

**Proposed signature**:
```rust
pub fn get_labels_by_prefix(
    graph: &SqliteGraph,
    prefix: &str,
) -> Result<Vec<String>, SqliteGraphError>
```

**Proposed implementation**:
```rust
pub fn get_labels_by_prefix(
    graph: &SqliteGraph,
    prefix: &str,
) -> Result<Vec<String>, SqliteGraphError> {
    let conn = graph.connection();
    let mut stmt = conn
        .prepare_cached("SELECT DISTINCT label FROM graph_labels WHERE label LIKE ?1 ORDER BY label")
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let pattern = format!("{}%", prefix);
    let rows = stmt
        .query_map(params![pattern], |row| row.get::<_, String>(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;
    let mut labels = Vec::new();
    for row in rows {
        labels.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }
    Ok(labels)
}
```

**Add to**: `src/index.rs`

---

## Summary of Changes

### File: `sqlitegraph/src/lib.rs`

**Current re-exports**:
```rust
pub use index::{add_label, add_property};
```

**Proposed re-exports**:
```rust
pub use index::{
    add_label,
    add_property,
    get_entities_by_label,      // Already exists, just export
    get_entities_by_property,   // Already exists, just export
    labels_for_entity,          // New function to add
    properties_for_entity,      // New function to add
    remove_label,               // New function to add
    remove_property,            // New function to add
    get_labels_by_prefix,       // New function to add
};
```

### File: `sqlitegraph/src/index.rs`

Add the new functions proposed above.

---

## Testing

Once implemented, update the test in `magellan/tests/sqlitegraph_exploration.rs`:

```rust
// Before (using raw SQL)
let labels = query_sql(&test_db, "SELECT label FROM graph_labels WHERE entity_id=?1", &[id])?;

// After (using helper)
let labels = labels_for_entity(&graph, entity_id)?;

// Before (using raw SQL)
let props = query_sql(&test_db, "SELECT key, value FROM graph_properties WHERE entity_id=?1", &[id])?;

// After (using helper)
let props = properties_for_entity(&graph, entity_id)?;
```

---

## Priority

1. **High** (Already exist, just need export):
   - `get_entities_by_label`
   - `get_entities_by_property`

2. **Medium** (New functions, commonly needed):
   - `labels_for_entity`
   - `properties_for_entity`

3. **Low** (Nice to have):
   - `remove_label`
   - `remove_property`
   - `get_labels_by_prefix`
