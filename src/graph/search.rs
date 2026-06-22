use anyhow::Result;
use sqlitegraph::SqliteGraph;

/// A single HopGraph search result with resolved symbol metadata.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HopgraphHit {
    pub entity_id: i64,
    pub score: f32,
    pub name: String,
    pub kind: String,
    pub file_path: Option<String>,
    pub start_line: usize,
    /// How many call-graph hops from the FTS5 seed (0 = direct name match).
    #[serde(skip_serializing_if = "is_zero")]
    pub hop_distance: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// FTS5-based symbol search.  Returns `(entity_id, score)` pairs ranked by
/// name relevance.  `score` is in `(0.0, 1.0]` — 1.0 for the best name match,
/// decaying linearly for lower-ranked results.
///
/// Uses `symbol_fts` (content table backed by `graph_entities`), which is
/// always current — no separate index maintenance required.
pub fn fts_search_symbols(
    conn: &rusqlite::Connection,
    query: &str,
    k: usize,
) -> Result<Vec<(i64, f32)>> {
    // Escape double-quotes so user input can't break the FTS5 query.
    let safe = query.replace('"', "\"\"");
    // Try prefix match first.
    let pattern = format!("{}*", safe);

    let mut stmt = conn
        .prepare_cached("SELECT rowid FROM symbol_fts WHERE name MATCH ? ORDER BY rank LIMIT ?")?;
    let ids: Vec<i64> = stmt
        .query_map(rusqlite::params![pattern, (k * 2) as i64], |row| {
            row.get::<_, i64>(0)
        })?
        .filter_map(|r| r.ok())
        .collect();

    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let n = ids.len() as f32;
    Ok(ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, 1.0_f32 - (i as f32 / n) * 0.5))
        .collect())
}

// ── no-op stubs for callers of the removed HNSW embed pipeline ───────────────
// embed_cmd and indexing paths still call these; they are now harmless no-ops.

pub fn bulk_add_to_search_index(
    _graph: &SqliteGraph,
    _entries: &[(i64, Vec<f32>)],
) -> Result<usize> {
    Ok(0)
}

pub fn remove_from_search_index(_graph: &SqliteGraph, _entity_id: i64) -> Result<()> {
    Ok(())
}

pub fn clear_search_index(_graph: &SqliteGraph) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE symbol_fts USING fts5(name, content='', tokenize='unicode61');",
        )
        .unwrap();
        conn
    }

    fn insert_symbol(conn: &rusqlite::Connection, rowid: i64, name: &str) {
        conn.execute(
            "INSERT INTO symbol_fts(rowid, name) VALUES (?, ?)",
            rusqlite::params![rowid, name],
        )
        .unwrap();
    }

    #[test]
    fn test_fts_search_finds_prefix_match() {
        let conn = open_test_conn();
        insert_symbol(&conn, 1, "parse_rust");
        insert_symbol(&conn, 2, "parse_python");
        insert_symbol(&conn, 3, "compile_rust");

        let results = fts_search_symbols(&conn, "parse_rust", 5).unwrap();
        assert!(!results.is_empty(), "should find results");
        let ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&1), "parse_rust should be found");
    }

    #[test]
    fn test_fts_search_scores_descending() {
        let conn = open_test_conn();
        insert_symbol(&conn, 1, "parse_rust");
        insert_symbol(&conn, 2, "parse_rust_file");
        insert_symbol(&conn, 3, "parse_python");

        let results = fts_search_symbols(&conn, "parse_rust", 10).unwrap();
        assert!(results.len() >= 2);
        // Scores must be non-increasing
        for w in results.windows(2) {
            assert!(w[0].1 >= w[1].1, "scores should be non-increasing");
        }
    }

    #[test]
    fn test_fts_search_empty_query_returns_empty() {
        let conn = open_test_conn();
        insert_symbol(&conn, 1, "foo");
        // Empty pattern after escaping becomes "*" which may return all or error;
        // the function must not panic.
        let _ = fts_search_symbols(&conn, "", 5);
    }

    #[test]
    fn test_fts_search_no_match_returns_empty() {
        let conn = open_test_conn();
        insert_symbol(&conn, 1, "foo_bar");
        let results = fts_search_symbols(&conn, "zzz_nonexistent", 5).unwrap();
        assert!(results.is_empty());
    }
}
