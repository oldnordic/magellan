use anyhow::Result;
use rusqlite::params;
use sqlitegraph::{backend::BackendDirection, multi_hop::ChainStep, pattern::PatternQuery};

use super::CodeGraph;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SymbolInfo {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub kind_normalized: Option<String>,
    pub file_path: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub byte_start: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DepthSymbol {
    pub depth: u32,
    pub info: SymbolInfo,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TypedEdgeHop {
    pub edge_type: String,
    pub direction: BackendDirection,
    pub target: SymbolInfo,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EdgeHop {
    pub edge_type: String,
    pub target: SymbolInfo,
}

pub struct SymbolNavigator<'a> {
    graph: &'a CodeGraph,
}

fn entity_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolInfo> {
    let data_str: String = row.get(4)?;
    let (kind_normalized, start_line, end_line, byte_start) = parse_entity_data(&data_str);
    Ok(SymbolInfo {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        file_path: row.get(3)?,
        kind_normalized,
        start_line,
        end_line,
        byte_start,
    })
}

fn parse_entity_data(data: &str) -> (Option<String>, usize, usize, usize) {
    let v: Option<serde_json::Value> = serde_json::from_str(data).ok();
    let Some(v) = v else {
        return (None, 0, 0, 0);
    };
    let kind_normalized = v
        .get("kind_normalized")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let start_line = v.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let end_line = v.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let byte_start = v.get("byte_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    (kind_normalized, start_line, end_line, byte_start)
}

fn query_neighbors(
    conn: &rusqlite::Connection,
    id: i64,
    edge_type: &str,
    incoming: bool,
) -> Result<Vec<i64>> {
    let sql = if incoming {
        "SELECT from_id FROM graph_edges WHERE to_id = ?1 AND edge_type = ?2 ORDER BY from_id"
    } else {
        "SELECT to_id FROM graph_edges WHERE from_id = ?1 AND edge_type = ?2 ORDER BY to_id"
    };
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt
        .query_map(params![id, edge_type], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

impl<'a> SymbolNavigator<'a> {
    pub fn new(graph: &'a CodeGraph) -> Self {
        Self { graph }
    }

    fn sqlite_graph(&self) -> Option<&sqlitegraph::SqliteGraph> {
        self.graph
            .symbols
            .sqlite_backend
            .as_ref()
            .map(|b| b.graph())
    }

    fn conn(&self) -> parking_lot::MutexGuard<'_, rusqlite::Connection> {
        self.graph.side_conn.lock()
    }

    fn resolve_entity_with_conn(
        conn: &rusqlite::Connection,
        id: i64,
    ) -> Result<Option<SymbolInfo>> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, kind, name, file_path, data FROM graph_entities WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], entity_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub(crate) fn resolve_entities_with_conn(
        conn: &rusqlite::Connection,
        ids: &[i64],
    ) -> Result<Vec<SymbolInfo>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut results = Vec::with_capacity(ids.len());
        let mut stmt = conn.prepare_cached(
            "SELECT id, kind, name, file_path, data FROM graph_entities WHERE id = ?1",
        )?;
        for &id in ids {
            let mut rows = stmt.query_map(params![id], entity_from_row)?;
            if let Some(row) = rows.next() {
                results.push(row?);
            }
        }
        Ok(results)
    }

    pub fn resolve(&self, name: &str) -> Result<Vec<SymbolInfo>> {
        use crate::graph::cache::NameCacheKey;
        let key = NameCacheKey(name.to_string());
        if let Some(cached) = self.graph.name_cache.get_cloned(&key) {
            return Ok(cached);
        }
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, kind, name, file_path, data FROM graph_entities WHERE name = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![name], entity_from_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        self.graph.name_cache.put(key, results.clone());
        Ok(results)
    }

    pub fn resolve_by_prefix(&self, prefix: &str) -> Result<Vec<SymbolInfo>> {
        let like = format!("{}%", prefix);
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, kind, name, file_path, data FROM graph_entities WHERE name LIKE ?1 AND kind = 'Symbol' ORDER BY id",
        )?;
        let rows = stmt.query_map(params![like], entity_from_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn info(&self, id: i64) -> Result<Option<SymbolInfo>> {
        use crate::graph::cache::EntityCacheKey;
        let key = EntityCacheKey(id);
        if let Some(cached) = self.graph.entity_cache.get_cloned(&key) {
            return Ok(Some(cached));
        }
        let conn = self.conn();
        let result = Self::resolve_entity_with_conn(&conn, id)?;
        if let Some(ref info) = result {
            self.graph.entity_cache.put(key, info.clone());
        }
        Ok(result)
    }

    pub fn expand(&self, id: i64) -> Result<Vec<TypedEdgeHop>> {
        use crate::graph::cache::ExpandCacheKey;
        let key = ExpandCacheKey(id);
        if let Some(cached) = self.graph.expand_cache.get_cloned(&key) {
            return Ok(cached);
        }
        let conn = self.conn();

        let mut stmt = conn.prepare_cached(
            "SELECT edge_type, to_id FROM graph_edges WHERE from_id = ?1 ORDER BY edge_type, to_id",
        )?;
        let outgoing: Vec<(String, i64)> = stmt
            .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut stmt = conn.prepare_cached(
            "SELECT edge_type, from_id FROM graph_edges WHERE to_id = ?1 ORDER BY edge_type, from_id",
        )?;
        let incoming: Vec<(String, i64)> = stmt
            .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut hops = Vec::new();
        for (edge_type, target_id) in outgoing {
            if let Some(info) = Self::resolve_entity_with_conn(&conn, target_id)? {
                hops.push(TypedEdgeHop {
                    edge_type,
                    direction: BackendDirection::Outgoing,
                    target: info,
                });
            }
        }
        for (edge_type, source_id) in incoming {
            if let Some(info) = Self::resolve_entity_with_conn(&conn, source_id)? {
                hops.push(TypedEdgeHop {
                    edge_type,
                    direction: BackendDirection::Incoming,
                    target: info,
                });
            }
        }
        self.graph.expand_cache.put(key, hops.clone());
        Ok(hops)
    }

    pub fn expand_typed(&self, id: i64, edge_type: &str) -> Result<Vec<EdgeHop>> {
        let sg = self
            .sqlite_graph()
            .ok_or_else(|| anyhow::anyhow!("no sqlite backend"))?;
        let targets = sg.query().edges_of_type(id, edge_type)?;
        let conn = self.conn();
        let infos = Self::resolve_entities_with_conn(&conn, &targets)?;
        Ok(infos
            .into_iter()
            .map(|info| EdgeHop {
                edge_type: edge_type.to_string(),
                target: info,
            })
            .collect())
    }

    pub fn chain(&self, start: i64, steps: &[ChainStep]) -> Result<Vec<SymbolInfo>> {
        let sg = self
            .sqlite_graph()
            .ok_or_else(|| anyhow::anyhow!("no sqlite backend"))?;
        let ids = sg.query().chain(start, steps)?;
        let conn = self.conn();
        Self::resolve_entities_with_conn(&conn, &ids)
    }

    pub fn k_hop_callers(&self, start: i64, max_depth: u32) -> Result<Vec<DepthSymbol>> {
        self.call_bfs(start, max_depth, true)
    }

    pub fn k_hop_callees(&self, start: i64, max_depth: u32) -> Result<Vec<DepthSymbol>> {
        self.call_bfs(start, max_depth, false)
    }

    fn call_bfs(&self, start: i64, max_depth: u32, incoming: bool) -> Result<Vec<DepthSymbol>> {
        let conn = self.conn();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut results: Vec<DepthSymbol> = Vec::new();
        visited.insert(start);
        queue.push_back((start, 0u32));

        while let Some((node, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let (first_edge, second_edge) = if incoming {
                ("CALLS", "CALLER")
            } else {
                ("CALLER", "CALLS")
            };
            let intermediates = query_neighbors(&conn, node, first_edge, incoming)?;
            for inter_id in intermediates {
                let targets = query_neighbors(&conn, inter_id, second_edge, incoming)?;
                for target_id in targets {
                    if visited.insert(target_id) {
                        if let Some(info) = Self::resolve_entity_with_conn(&conn, target_id)? {
                            if info.kind == "Symbol" {
                                results.push(DepthSymbol {
                                    depth: depth + 1,
                                    info,
                                });
                                queue.push_back((target_id, depth + 1));
                            }
                        }
                    }
                }
            }
        }
        results.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| a.info.name.cmp(&b.info.name))
        });
        Ok(results)
    }

    pub fn k_hop_references(&self, start: i64, depth: u32) -> Result<Vec<SymbolInfo>> {
        let sg = self
            .sqlite_graph()
            .ok_or_else(|| anyhow::anyhow!("no sqlite backend"))?;
        let ids =
            sg.query()
                .k_hop_filtered(start, depth, BackendDirection::Incoming, &["REFERENCES"])?;
        let conn = self.conn();
        Self::resolve_entities_with_conn(&conn, &ids)
    }

    pub fn pattern(&self, start: i64, query: &PatternQuery) -> Result<Vec<Vec<SymbolInfo>>> {
        let sg = self
            .sqlite_graph()
            .ok_or_else(|| anyhow::anyhow!("no sqlite backend"))?;
        let matches = sg.query().pattern_matches(start, query)?;
        let conn = self.conn();
        let mut result = Vec::new();
        for pattern_match in matches {
            let infos = Self::resolve_entities_with_conn(&conn, &pattern_match.nodes)?;
            result.push(infos);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeGraph;
    use sqlitegraph::multi_hop::ChainStep;

    fn setup_graph() -> (tempfile::TempDir, CodeGraph) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nav_test.db");
        let mut graph = CodeGraph::open(&db_path).unwrap();

        let src = r#"
fn alpha() {
    bravo();
    charlie();
}

fn bravo() {
    delta();
}

fn charlie() {
    delta();
}

fn delta() {
    echo();
}

fn echo() {}
"#;
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, src).unwrap();
        let path_str = test_file.to_string_lossy().to_string();
        let source = std::fs::read(&test_file).unwrap();
        graph.index_file(&path_str, &source).unwrap();

        (temp_dir, graph)
    }

    #[test]
    fn test_resolve_finds_symbol_by_exact_name() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let results = nav.resolve("alpha").unwrap();
        assert!(!results.is_empty(), "should find alpha");
        let alpha = results.iter().find(|s| s.name == "alpha").unwrap();
        assert_eq!(alpha.kind, "Symbol");
        assert!(alpha.file_path.is_some());
    }

    #[test]
    fn test_resolve_returns_empty_for_unknown() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let results = nav.resolve("nonexistent_xyzzy").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_resolve_by_prefix() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let results = nav.resolve_by_prefix("alph").unwrap();
        assert!(!results.is_empty(), "should find alpha via prefix");
        assert!(results.iter().any(|s| s.name == "alpha"));
    }

    #[test]
    fn test_info_returns_symbol_details() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_results = nav.resolve("alpha").unwrap();
        let alpha_id = alpha_results[0].id;

        let info = nav.info(alpha_id).unwrap().unwrap();
        assert_eq!(info.name, "alpha");
        assert_eq!(info.kind, "Symbol");
    }

    #[test]
    fn test_info_returns_none_for_invalid_id() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let info = nav.info(9999999).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_expand_returns_edges_for_symbol() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_id = nav.resolve("alpha").unwrap()[0].id;
        let hops = nav.expand(alpha_id).unwrap();

        let has_outgoing = hops
            .iter()
            .any(|h| h.direction == BackendDirection::Outgoing);
        let has_incoming = hops
            .iter()
            .any(|h| h.direction == BackendDirection::Incoming);

        assert!(has_outgoing, "alpha should have outgoing edges");
        assert!(
            has_incoming,
            "alpha should have incoming edges (defined in a file)"
        );
    }

    #[test]
    fn test_expand_typed_filters_by_edge_type() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_id = nav.resolve("alpha").unwrap()[0].id;
        let hops = nav.expand_typed(alpha_id, "CALLER").unwrap();

        let names: Vec<&str> = hops.iter().map(|h| h.target.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("bravo")),
            "alpha calls bravo, got: {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n.contains("charlie")),
            "alpha calls charlie, got: {:?}",
            names
        );
    }

    #[test]
    fn test_chain_follows_call_edges() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_id = nav.resolve("alpha").unwrap()[0].id;
        let steps = vec![
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLER".to_string()),
            },
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
        ];
        let results = nav.chain(alpha_id, &steps).unwrap();

        let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"bravo"),
            "chain should reach bravo, got: {:?}",
            names
        );
        assert!(
            names.contains(&"charlie"),
            "chain should reach charlie, got: {:?}",
            names
        );
    }

    #[test]
    fn test_chain_two_hops() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_id = nav.resolve("alpha").unwrap()[0].id;
        let steps = vec![
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLER".to_string()),
            },
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLER".to_string()),
            },
            ChainStep {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
        ];
        let results = nav.chain(alpha_id, &steps).unwrap();

        let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"delta"),
            "alpha -> bravo/charlie -> delta, got: {:?}",
            names
        );
    }

    #[test]
    fn test_k_hop_callers() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let delta_id = nav.resolve("delta").unwrap()[0].id;
        let callers = nav.k_hop_callers(delta_id, 2).unwrap();

        let names: Vec<&str> = callers.iter().map(|s| s.info.name.as_str()).collect();
        assert!(
            names.contains(&"bravo"),
            "bravo calls delta, got: {:?}",
            names
        );
        assert!(
            names.contains(&"charlie"),
            "charlie calls delta, got: {:?}",
            names
        );
        assert!(
            names.contains(&"alpha"),
            "alpha calls bravo/charlie who call delta, got: {:?}",
            names
        );

        let depth_1: Vec<&str> = callers
            .iter()
            .filter(|s| s.depth == 1)
            .map(|s| s.info.name.as_str())
            .collect();
        let depth_2: Vec<&str> = callers
            .iter()
            .filter(|s| s.depth == 2)
            .map(|s| s.info.name.as_str())
            .collect();
        assert!(
            depth_1.contains(&"bravo") && depth_1.contains(&"charlie"),
            "bravo and charlie should be depth 1, got depth_1: {:?}",
            depth_1
        );
        assert!(
            depth_2.contains(&"alpha"),
            "alpha should be depth 2, got depth_2: {:?}",
            depth_2
        );
    }

    #[test]
    fn test_k_hop_callees() {
        let (_td, graph) = setup_graph();
        let nav = SymbolNavigator::new(&graph);

        let alpha_id = nav.resolve("alpha").unwrap()[0].id;
        let callees = nav.k_hop_callees(alpha_id, 3).unwrap();

        let names: Vec<&str> = callees.iter().map(|s| s.info.name.as_str()).collect();
        assert!(names.contains(&"bravo"), "alpha -> bravo, got: {:?}", names);
        assert!(
            names.contains(&"delta"),
            "alpha -> bravo -> delta, got: {:?}",
            names
        );
        assert!(
            names.contains(&"echo"),
            "alpha -> bravo -> delta -> echo, got: {:?}",
            names
        );

        let depth_1: Vec<&str> = callees
            .iter()
            .filter(|s| s.depth == 1)
            .map(|s| s.info.name.as_str())
            .collect();
        let depth_2: Vec<&str> = callees
            .iter()
            .filter(|s| s.depth == 2)
            .map(|s| s.info.name.as_str())
            .collect();
        let depth_3: Vec<&str> = callees
            .iter()
            .filter(|s| s.depth == 3)
            .map(|s| s.info.name.as_str())
            .collect();
        assert!(
            depth_1.contains(&"bravo") && depth_1.contains(&"charlie"),
            "bravo and charlie should be depth 1, got: {:?}",
            depth_1
        );
        assert!(
            depth_2.contains(&"delta"),
            "delta should be depth 2, got: {:?}",
            depth_2
        );
        assert!(
            depth_3.contains(&"echo"),
            "echo should be depth 3, got: {:?}",
            depth_3
        );
    }
}
