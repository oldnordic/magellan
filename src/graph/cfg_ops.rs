//! CFG block storage and retrieval operations.
//!
//! This module handles persistence of CFG (Control Flow Graph) data
//! extracted from AST nodes or MIR.

use anyhow::Result;
// use fixedbitset::FixedBitSet; // removed: migrated to petgraph dominators
use petgraph::algo::dominators::simple_fast;
use petgraph::graph::Graph;
use rusqlite::params;
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::generation::ChunkStore;
use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType, CfgWithEdges};
use crate::graph::schema::CfgBlock;

/// CFG block operations
pub struct CfgOps {
    /// ChunkStore for database connection (shares connection with AST nodes)
    chunks: ChunkStore,
}

impl CfgOps {
    /// Create new CfgOps instance
    pub fn new(chunks: ChunkStore) -> Self {
        Self { chunks }
    }

    /// Extract and store CFG blocks from a function node.
    pub fn index_cfg_from_node(
        &self,
        func_node: &tree_sitter::Node,
        source: &[u8],
        function_id: i64,
    ) -> Result<usize> {
        use crate::graph::cfg_edges_extract::extract_cfg_from_function_node;

        // Convert source to string for the new extraction
        let source_str = std::str::from_utf8(source)
            .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in function source: {}", e))?;

        // Extract CFG with edges from function node
        let cfg = extract_cfg_from_function_node(func_node, function_id, source_str);

        if cfg.blocks.is_empty() {
            return Ok(0);
        }

        // Persist to database
        self.insert_cfg_blocks(&cfg.blocks)?;

        // Store edges (using per-function block indices)
        let _ = self.insert_cfg_edges(function_id, &cfg.edges);

        Ok(cfg.blocks.len())
    }

    /// Extract and store CFG blocks from function source text.
    pub fn index_cfg(
        &self,
        source: &str,
        function_id: i64,
        language: tree_sitter::Language,
    ) -> Result<usize> {
        use crate::graph::cfg_edges_extract::extract_cfg_with_edges;

        // Extract CFG with edges
        let cfg = extract_cfg_with_edges(source, function_id, language);

        if cfg.blocks.is_empty() {
            return Ok(0);
        }

        // Persist to database
        self.insert_cfg_blocks(&cfg.blocks)?;

        Ok(cfg.blocks.len())
    }

    /// Extract and store CFG blocks from Rust source using MIR frontend.
    #[cfg(feature = "mir-frontend")]
    pub fn index_cfg_from_mir(
        &self,
        source: &str,
        function_symbols: &[(String, i64, i64, i64)],
    ) -> Result<usize> {
        use crate::graph::mir_frontend::extract_cfg_from_rust_source;

        let mut total_blocks = 0;

        for (func_name, function_id, _byte_start, _byte_end) in function_symbols {
            // Call MIR frontend with complete source (not just function body)
            match extract_cfg_from_rust_source(func_name, source) {
                Ok(mir_result) => {
                    // Convert MIR blocks to database format
                    let blocks: Vec<crate::graph::schema::CfgBlock> = mir_result
                        .blocks
                        .into_iter()
                        .map(|block| crate::graph::schema::CfgBlock {
                            function_id: *function_id,
                            kind: block.kind,
                            terminator: block.terminator,
                            byte_start: block.byte_start,
                            byte_end: block.byte_end,
                            start_line: block.start_line,
                            start_col: block.start_col,
                            end_line: block.end_line,
                            end_col: block.end_col,
                            cfg_hash: block.cfg_hash,
                            statements: block.statements,
                            cfg_condition: block.cfg_condition,
                        })
                        .collect();

                    // Insert CFG blocks
                    self.insert_cfg_blocks(&blocks)?;

                    // Insert CFG edges
                    let edges: Vec<crate::graph::cfg_edges_extract::CfgEdge> = mir_result
                        .edges
                        .into_iter()
                        .map(|edge| crate::graph::cfg_edges_extract::CfgEdge {
                            source_idx: edge.source_idx,
                            target_idx: edge.target_idx,
                            edge_type: edge.edge_type,
                        })
                        .collect();

                    let _ = self.insert_cfg_edges(*function_id, &edges);

                    total_blocks += blocks.len();
                }
                Err(e) => {
                    eprintln!(
                        "Warning: MIR CFG extraction failed for function {}: {}",
                        func_name, e
                    );
                }
            }
        }

        Ok(total_blocks)
    }

    /// Insert CFG blocks into database
    pub fn insert_cfg_blocks(&self, blocks: &[CfgBlock]) -> Result<usize> {
        use sha2::{Digest, Sha256};

        self.chunks.with_connection_mut(|conn| {
            let tx = conn.unchecked_transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO cfg_blocks (
                        function_id, kind, terminator,
                        byte_start, byte_end,
                        start_line, start_col,
                        end_line, end_col,
                        cfg_hash,
                        statements,
                        cfg_condition
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )?;

                for block in blocks {
                    let statements_json = block
                        .statements
                        .as_ref()
                        .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "[]".to_string()));

                    let cfg_hash = if let Some(ref h) = block.cfg_hash {
                        h.clone()
                    } else {
                        let mut hasher = Sha256::new();
                        hasher.update(block.function_id.to_le_bytes());
                        hasher.update(&block.kind);
                        hasher.update(&block.terminator);
                        hasher.update(block.byte_start.to_le_bytes());
                        hasher.update(block.byte_end.to_le_bytes());
                        if let Some(ref s) = statements_json {
                            hasher.update(s.as_bytes());
                        }
                        format!("{:x}", hasher.finalize())[..16].to_string()
                    };

                    stmt.execute(params![
                        block.function_id,
                        block.kind,
                        block.terminator,
                        block.byte_start,
                        block.byte_end,
                        block.start_line,
                        block.start_col,
                        block.end_line,
                        block.end_col,
                        cfg_hash,
                        statements_json,
                        block.cfg_condition.as_ref(),
                    ])?;
                }
            }
            tx.commit()?;
            Ok(blocks.len())
        })
    }

    /// Insert CFG edges into database
    pub fn insert_cfg_edges(
        &self,
        function_id: i64,
        edges: &[crate::graph::cfg_edges_extract::CfgEdge],
    ) -> Result<usize> {
        if edges.is_empty() {
            return Ok(0);
        }
        self.chunks.with_connection_mut(|conn| {
            let tx = conn.unchecked_transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO cfg_edges (function_id, source_idx, target_idx, edge_type)
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for edge in edges {
                    stmt.execute(params![
                        function_id,
                        edge.source_idx as i64,
                        edge.target_idx as i64,
                        edge.edge_type.as_str(),
                    ])?;
                }
            }
            tx.commit()?;
            Ok(edges.len())
        })
    }

    /// Get CFG edges for a function
    pub fn get_cfg_edges_for_function(
        &self,
        function_id: i64,
    ) -> Result<Vec<crate::graph::cfg_edges_extract::CfgEdge>> {
        self.chunks.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT source_idx, target_idx, edge_type
                 FROM cfg_edges
                 WHERE function_id = ?1
                 ORDER BY id",
            )?;
            let edges = stmt
                .query_map(params![function_id], |row| {
                    let source_idx: i64 = row.get(0)?;
                    let target_idx: i64 = row.get(1)?;
                    let edge_type_str: String = row.get(2)?;
                    let edge_type = match edge_type_str.as_str() {
                        "fallthrough" => crate::graph::cfg_edges_extract::CfgEdgeType::Fallthrough,
                        "conditional_true" => {
                            crate::graph::cfg_edges_extract::CfgEdgeType::ConditionalTrue
                        }
                        "conditional_false" => {
                            crate::graph::cfg_edges_extract::CfgEdgeType::ConditionalFalse
                        }
                        "jump" => crate::graph::cfg_edges_extract::CfgEdgeType::Jump,
                        "back_edge" => crate::graph::cfg_edges_extract::CfgEdgeType::BackEdge,
                        "call" => crate::graph::cfg_edges_extract::CfgEdgeType::Call,
                        "return" => crate::graph::cfg_edges_extract::CfgEdgeType::Return,
                        _ => crate::graph::cfg_edges_extract::CfgEdgeType::Fallthrough,
                    };
                    Ok(crate::graph::cfg_edges_extract::CfgEdge {
                        source_idx: source_idx as usize,
                        target_idx: target_idx as usize,
                        edge_type,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(edges)
        })
    }

    pub fn delete_cfg_for_function(&self, function_id: i64) -> Result<usize> {
        self.chunks.with_connection_mut(|conn| {
            conn.execute(
                "DELETE FROM cfg_edges WHERE function_id = ?1",
                params![function_id],
            )?;
            let affected = conn.execute(
                "DELETE FROM cfg_blocks WHERE function_id = ?1",
                params![function_id],
            )?;
            Ok(affected)
        })
    }

    pub fn delete_cfg_for_functions(&self, function_ids: &[i64]) -> Result<usize> {
        if function_ids.is_empty() {
            return Ok(0);
        }
        self.chunks.with_connection_mut(|conn| {
            let placeholders = function_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");

            // Delete path elements first (FK to cfg_paths without cascade)
            let path_elements_sql = format!(
                "DELETE FROM cfg_path_elements WHERE path_id IN (SELECT path_id FROM cfg_paths WHERE function_id IN ({}))",
                placeholders
            );
            if let Ok(mut path_elements_stmt) = conn.prepare(&path_elements_sql) {
                let params = function_ids.to_vec();
                let _ = path_elements_stmt.execute(rusqlite::params_from_iter(&params));
            }

            // Delete cfg_paths (FK to graph_entities without cascade)
            let paths_sql = format!(
                "DELETE FROM cfg_paths WHERE function_id IN ({})",
                placeholders
            );
            if let Ok(mut paths_stmt) = conn.prepare(&paths_sql) {
                let params = function_ids.to_vec();
                let _ = paths_stmt.execute(rusqlite::params_from_iter(&params));
            }

            // Delete dominators referencing blocks of these functions
            let dom_sql = format!(
                "DELETE FROM cfg_dominators WHERE block_id IN (SELECT id FROM cfg_blocks WHERE function_id IN ({})) OR dominator_id IN (SELECT id FROM cfg_blocks WHERE function_id IN ({}))",
                placeholders, placeholders
            );
            if let Ok(mut dom_stmt) = conn.prepare(&dom_sql) {
                let params: Vec<i64> = function_ids
                    .iter()
                    .chain(function_ids.iter())
                    .copied()
                    .collect();
                let _ = dom_stmt.execute(rusqlite::params_from_iter(&params));
            }

            let post_dom_sql = format!(
                "DELETE FROM cfg_post_dominators WHERE block_id IN (SELECT id FROM cfg_blocks WHERE function_id IN ({})) OR post_dominator_id IN (SELECT id FROM cfg_blocks WHERE function_id IN ({}))",
                placeholders, placeholders
            );
            if let Ok(mut post_dom_stmt) = conn.prepare(&post_dom_sql) {
                let params: Vec<i64> = function_ids
                    .iter()
                    .chain(function_ids.iter())
                    .copied()
                    .collect();
                let _ = post_dom_stmt.execute(rusqlite::params_from_iter(&params));
            }

            let edge_sql = format!(
                "DELETE FROM cfg_edges WHERE function_id IN ({})",
                placeholders
            );
            let mut edge_stmt = conn.prepare(&edge_sql)?;
            let params = function_ids.to_vec();
            edge_stmt.execute(rusqlite::params_from_iter(&params))?;

            let sql = format!(
                "DELETE FROM cfg_blocks WHERE function_id IN ({})",
                placeholders
            );
            let mut stmt = conn.prepare(&sql)?;
            let params = function_ids.to_vec();
            let affected = stmt.execute(rusqlite::params_from_iter(params))?;
            Ok(affected)
        })
    }

    pub fn get_cfg_for_function(&self, function_id: i64) -> Result<Vec<CfgBlock>> {
        self.chunks.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT function_id, kind, terminator,
                        byte_start, byte_end,
                        start_line, start_col,
                        end_line, end_col,
                        cfg_hash, statements,
                        cfg_condition
                 FROM cfg_blocks
                 WHERE function_id = ?1
                 ORDER BY byte_start",
            )?;

            let blocks = stmt
                .query_map(params![function_id], |row| {
                    let statements_json: Option<String> = row.get(10)?;
                    let statements = statements_json.and_then(|s| serde_json::from_str(&s).ok());

                    Ok(CfgBlock {
                        function_id: row.get(0)?,
                        kind: row.get(1)?,
                        terminator: row.get(2)?,
                        byte_start: row.get(3)?,
                        byte_end: row.get(4)?,
                        start_line: row.get(5)?,
                        start_col: row.get(6)?,
                        end_line: row.get(7)?,
                        end_col: row.get(8)?,
                        cfg_hash: row.get(9)?,
                        statements,
                        cfg_condition: row.get(11)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(blocks)
        })
    }

    pub fn get_cfg_for_file(&self, file_path: &str) -> Result<Vec<(i64, Vec<CfgBlock>)>> {
        self.chunks.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT e.id AS function_id,
                        c.function_id, c.kind, c.terminator,
                        c.byte_start, c.byte_end,
                        c.start_line, c.start_col,
                        c.end_line, c.end_col,
                        c.cfg_hash, c.statements,
                        c.cfg_condition
                 FROM cfg_blocks c
                 JOIN graph_entities e ON c.function_id = e.id
                 WHERE e.file_path = ?1
                 ORDER BY e.id, c.byte_start",
            )?;

            let mut result: std::collections::HashMap<i64, Vec<CfgBlock>> =
                std::collections::HashMap::new();
            let rows = stmt.query_map(params![file_path], |row| {
                let function_id: i64 = row.get(0)?;
                let statements_json: Option<String> = row.get(11)?;
                let statements = statements_json.and_then(|s| serde_json::from_str(&s).ok());

                let block = CfgBlock {
                    function_id: row.get(1)?,
                    kind: row.get(2)?,
                    terminator: row.get(3)?,
                    byte_start: row.get(4)?,
                    byte_end: row.get(5)?,
                    start_line: row.get(6)?,
                    start_col: row.get(7)?,
                    end_line: row.get(8)?,
                    end_col: row.get(9)?,
                    cfg_hash: row.get(10)?,
                    statements,
                    cfg_condition: row.get(12)?,
                };
                Ok((function_id, block))
            })?;

            for row in rows {
                let (function_id, block) = row?;
                result.entry(function_id).or_default().push(block);
            }
            Ok(result.into_iter().collect())
        })
    }

    /// Read active feature flags from `magellan_meta.project_metadata`.
    ///
    /// Returns empty set when no metadata exists or parsing fails.
    pub fn get_active_features(&self) -> Result<HashSet<String>> {
        self.chunks.with_conn(|conn| {
            let meta_json: Option<String> = conn
                .query_row(
                    "SELECT project_metadata FROM magellan_meta WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap_or(None);

            let mut features = HashSet::new();
            if let Some(json) = meta_json {
                if let Ok(manifest) = serde_json::from_str::<crate::manifest::CargoManifest>(&json)
                {
                    for (feature, _deps) in manifest.features {
                        features.insert(feature);
                    }
                }
            }
            Ok(features)
        })
    }

    /// Get CFG blocks for a function, filtering out dead `#[cfg]` branches.
    ///
    /// A block is dead when its `cfg_condition` evaluates false against
    /// the project's active features. Blocks without a condition are always live.
    pub fn get_live_cfg_for_function(&self, function_id: i64) -> Result<Vec<CfgBlock>> {
        let active_features = self.get_active_features()?;
        let all_blocks = self.get_cfg_for_function(function_id)?;

        Ok(all_blocks
            .into_iter()
            .filter(|b| {
                b.cfg_condition
                    .as_ref()
                    .map(|c| evaluate_cfg_condition(c, &active_features))
                    .unwrap_or(true)
            })
            .collect())
    }
}

/// Evaluate a simple cfg condition against active features.
///
/// Supports: `feature = "name"`, `all(feature = "a", feature = "b")`,
/// `any(feature = "a", feature = "b")`, `not(feature = "a")`.
/// Unknown conditions conservatively return `true`.
pub fn evaluate_cfg_condition(condition: &str, active_features: &HashSet<String>) -> bool {
    let condition = condition.trim();

    // `feature = "name"`
    if let Some(feature_name) = condition
        .strip_prefix("feature = \"")
        .and_then(|s| s.strip_suffix("\""))
    {
        return active_features.contains(feature_name);
    }

    // `all(...)` — all sub-conditions must be true
    if let Some(inner) = condition
        .strip_prefix("all(")
        .and_then(|s| s.strip_suffix(")"))
    {
        return inner
            .split(',')
            .all(|c| evaluate_cfg_condition(c.trim(), active_features));
    }

    // `any(...)` — at least one sub-condition must be true
    if let Some(inner) = condition
        .strip_prefix("any(")
        .and_then(|s| s.strip_suffix(")"))
    {
        return inner
            .split(',')
            .any(|c| evaluate_cfg_condition(c.trim(), active_features));
    }

    // `not(...)` — negate the sub-condition
    if let Some(inner) = condition
        .strip_prefix("not(")
        .and_then(|s| s.strip_suffix(")"))
    {
        return !evaluate_cfg_condition(inner.trim(), active_features);
    }

    // Unknown condition — conservatively include the block
    true
}

#[cfg(test)]
mod spatial_tests {
    use super::*;

    #[test]
    fn test_cfg_edges_roundtrip() {
        // Create an in-memory ChunkStore (uses temp file) with full schema
        let chunks = ChunkStore::in_memory();
        let ops = CfgOps::new(chunks);

        // Insert two blocks for function 42
        let blocks = vec![
            CfgBlock {
                function_id: 42,
                kind: "ENTRY".to_string(),
                terminator: "FALLTHROUGH".to_string(),
                byte_start: 0,
                byte_end: 10,
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 10,
                cfg_hash: None,
                statements: None,
                cfg_condition: None,
            },
            CfgBlock {
                function_id: 42,
                kind: "BASIC".to_string(),
                terminator: "RETURN".to_string(),
                byte_start: 10,
                byte_end: 20,
                start_line: 2,
                start_col: 0,
                end_line: 2,
                end_col: 10,
                cfg_hash: None,
                statements: None,
                cfg_condition: None,
            },
        ];
        ops.insert_cfg_blocks(&blocks).unwrap();

        // Insert edges (per-function block indices)
        let edges = vec![
            CfgEdge {
                source_idx: 0,
                target_idx: 1,
                edge_type: CfgEdgeType::Fallthrough,
            },
            CfgEdge {
                source_idx: 1,
                target_idx: 1,
                edge_type: CfgEdgeType::BackEdge,
            },
        ];
        let inserted = ops.insert_cfg_edges(42, &edges).unwrap();
        assert_eq!(inserted, 2);

        // Retrieve edges and verify round-trip
        let retrieved = ops.get_cfg_edges_for_function(42).unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0].source_idx, 0);
        assert_eq!(retrieved[0].target_idx, 1);
        assert_eq!(retrieved[0].edge_type, CfgEdgeType::Fallthrough);
        assert_eq!(retrieved[1].source_idx, 1);
        assert_eq!(retrieved[1].target_idx, 1);
        assert_eq!(retrieved[1].edge_type, CfgEdgeType::BackEdge);

        // Delete edges + blocks for the function
        let deleted = ops.delete_cfg_for_function(42).unwrap();
        assert_eq!(deleted, 2); // 2 blocks deleted

        // Verify edges are also deleted
        let after_delete = ops.get_cfg_edges_for_function(42).unwrap();
        assert!(after_delete.is_empty());
    }

    #[test]
    fn test_cfg_extract_ops_functions_no_stack_overflow() {
        let source = std::fs::read_to_string("src/graph/ops.rs").unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();

        let root = tree.root_node();

        fn find_functions<'a>(
            node: tree_sitter::Node<'a>,
            result: &mut Vec<tree_sitter::Node<'a>>,
        ) {
            if node.kind() == "function_item" {
                result.push(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                find_functions(child, result);
            }
        }

        let mut functions = Vec::new();
        find_functions(root, &mut functions);

        println!("Found {} functions", functions.len());

        for (i, func) in functions.iter().enumerate() {
            let name = func
                .child_by_field_name("name")
                .map(|n| {
                    let bytes = &source.as_bytes()[n.start_byte()..n.end_byte()];
                    String::from_utf8_lossy(bytes).to_string()
                })
                .unwrap_or_else(|| format!("func_{}", i));

            println!("Testing {} ...", name);
            let cfg = crate::graph::cfg_edges_extract::extract_cfg_from_function_node(
                func,
                i as i64 + 1,
                &source,
            );
            println!(
                "  Extracted {} blocks, {} edges",
                cfg.blocks.len(),
                cfg.edges.len()
            );

            println!("  PASSED");
        }
    }

    #[test]
    fn test_evaluate_cfg_condition_feature() {
        let mut features = HashSet::new();
        features.insert("tokio".to_string());

        assert!(
            evaluate_cfg_condition(r#"feature = "tokio""#, &features),
            "active feature should evaluate true"
        );
        assert!(
            !evaluate_cfg_condition(r#"feature = "async-std""#, &features),
            "inactive feature should evaluate false"
        );
    }

    #[test]
    fn test_evaluate_cfg_condition_all() {
        let mut features = HashSet::new();
        features.insert("a".to_string());
        features.insert("b".to_string());

        assert!(
            evaluate_cfg_condition(r#"all(feature = "a", feature = "b")"#, &features),
            "all active should be true"
        );
        assert!(
            !evaluate_cfg_condition(r#"all(feature = "a", feature = "c")"#, &features),
            "one missing should be false"
        );
    }

    #[test]
    fn test_evaluate_cfg_condition_any() {
        let mut features = HashSet::new();
        features.insert("a".to_string());

        assert!(
            evaluate_cfg_condition(r#"any(feature = "a", feature = "b")"#, &features),
            "one active in any should be true"
        );
        assert!(
            !evaluate_cfg_condition(r#"any(feature = "c", feature = "d")"#, &features),
            "none active in any should be false"
        );
    }

    #[test]
    fn test_evaluate_cfg_condition_not() {
        let mut features = HashSet::new();
        features.insert("a".to_string());

        assert!(
            !evaluate_cfg_condition(r#"not(feature = "a")"#, &features),
            "not active should be false"
        );
        assert!(
            evaluate_cfg_condition(r#"not(feature = "b")"#, &features),
            "not inactive should be true"
        );
    }

    #[test]
    fn test_evaluate_cfg_condition_unknown_is_true() {
        let features = HashSet::new();

        // Unknown conditions are conservatively included
        assert!(
            evaluate_cfg_condition("target_os = \"linux\"", &features),
            "unknown condition should conservatively return true"
        );
    }

    #[test]
    fn test_get_live_cfg_filters_dead_branches() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a fresh database
        let _graph = crate::CodeGraph::open(&db_path).unwrap();

        // Seed project_metadata with features and create a function entity for FK
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE magellan_meta SET project_metadata = ?1 WHERE id = 1",
            rusqlite::params![r#"{"features":{"tokio":["sync"]},"dependencies":[],"targets":[],"package_name":null}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO graph_entities (id, kind, name, file_path, data) VALUES (1, 'Function', 'test_fn', '/test.rs', '{}')",
            [],
        )
        .unwrap();
        drop(conn);

        let store = crate::generation::ChunkStore::new(&db_path);
        let cfg_ops = CfgOps::new(store);

        // Insert blocks with and without cfg_condition
        let blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "FALLTHROUGH".to_string(),
                byte_start: 0,
                byte_end: 10,
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 10,
                cfg_hash: None,
                statements: None,
                cfg_condition: None,
            },
            CfgBlock {
                function_id: 1,
                kind: "conditional".to_string(),
                terminator: "IF".to_string(),
                byte_start: 10,
                byte_end: 20,
                start_line: 2,
                start_col: 0,
                end_line: 2,
                end_col: 10,
                cfg_hash: None,
                statements: None,
                cfg_condition: Some(r#"feature = "tokio""#.to_string()),
            },
            CfgBlock {
                function_id: 1,
                kind: "dead".to_string(),
                terminator: "RETURN".to_string(),
                byte_start: 20,
                byte_end: 30,
                start_line: 3,
                start_col: 0,
                end_line: 3,
                end_col: 10,
                cfg_hash: None,
                statements: None,
                cfg_condition: Some(r#"feature = "async-std""#.to_string()),
            },
        ];

        cfg_ops.insert_cfg_blocks(&blocks).unwrap();

        // get_cfg_for_function returns all blocks
        let all = cfg_ops.get_cfg_for_function(1).unwrap();
        assert_eq!(all.len(), 3, "all blocks should be stored");

        // get_live_cfg_for_function filters dead branches
        let live = cfg_ops.get_live_cfg_for_function(1).unwrap();
        assert_eq!(live.len(), 2, "dead branch should be filtered out");
        assert!(
            live.iter().any(|b| b.kind == "entry"),
            "entry block should be live"
        );
        assert!(
            live.iter().any(|b| b.kind == "conditional"),
            "tokio block should be live"
        );
        assert!(
            !live.iter().any(|b| b.kind == "dead"),
            "async-std block should be dead"
        );
    }
}
