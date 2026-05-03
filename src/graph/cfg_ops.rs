//! CFG block storage and retrieval operations
//!
//! This module handles persistence of CFG (Control Flow Graph) data
//! extracted from AST nodes or MIR, plus 4D spatial-temporal analysis.

use anyhow::Result;
use rusqlite::params;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

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

    /// Extract and store CFG blocks for a function
    ///
    /// DEPRECATED: This is a compatibility shim. Use `index_cfg_with_4d_coordinates_from_node`
    /// for new code.
    pub fn index_cfg_for_function(
        &self,
        func_node: &tree_sitter::Node,
        source: &[u8],
        function_id: i64,
    ) -> Result<usize> {
        self.index_cfg_with_4d_coordinates_from_node(func_node, source, function_id)
    }

    /// Extract and store CFG with 4D spatial coordinates (from function node)
    ///
    /// This is the modern entry point that combines:
    /// 1. CFG extraction with edges from a function node
    /// 2. 4D coordinate computation (X, Y, Z)
    /// 3. Database persistence
    ///
    /// This function is designed to work with function nodes from a file's AST,
    /// making it compatible with the existing indexing pipeline.
    pub fn index_cfg_with_4d_coordinates_from_node(
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
        let mut cfg = extract_cfg_from_function_node(func_node, function_id, source_str);

        if cfg.blocks.is_empty() {
            return Ok(0);
        }

        // Compute 4D coordinates
        compute_4d_coordinates(&mut cfg);

        // Persist to database
        self.insert_cfg_blocks(&cfg.blocks)?;

        // Store edges (using per-function block indices)
        let _ = self.insert_cfg_edges(function_id, &cfg.edges);

        Ok(cfg.blocks.len())
    }

    /// Extract and store CFG with 4D spatial coordinates (from function source)
    ///
    /// This is an alternative entry point that accepts just the function source code
    /// and language, parsing it to extract the CFG. This is useful for standalone
    /// function analysis or when you don't have a function node.
    pub fn index_cfg_with_4d_coordinates(
        &self,
        source: &str,
        function_id: i64,
        language: tree_sitter::Language,
    ) -> Result<usize> {
        use crate::graph::cfg_edges_extract::extract_cfg_with_edges;

        // Extract CFG with edges
        let mut cfg = extract_cfg_with_edges(source, function_id, language);

        if cfg.blocks.is_empty() {
            return Ok(0);
        }

        // Compute 4D coordinates
        compute_4d_coordinates(&mut cfg);

        // Persist to database
        self.insert_cfg_blocks(&cfg.blocks)?;

        Ok(cfg.blocks.len())
    }

    /// Insert CFG blocks into database
    pub fn insert_cfg_blocks(&self, blocks: &[CfgBlock]) -> Result<usize> {
        use sha2::{Digest, Sha256};

        let conn = self.chunks.connect()?;

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
                    coord_x, coord_y, coord_z, coord_t
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            for block in blocks {
                let statements_json = block
                    .statements
                    .as_ref()
                    .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "[]".to_string()));

                // Compute hash from block data for cache invalidation
                // Use provided hash if available, otherwise compute it
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
                    block.coord_x,
                    block.coord_y,
                    block.coord_z,
                    block.coord_t.as_ref(),
                ])?;
            }
        }
        tx.commit()?;

        // Checkpoint WAL after bulk block insert
        if let Err(e) = crate::graph::wal::checkpoint_conn(&conn) {
            eprintln!("Warning: WAL checkpoint failed after CFG block insert: {}", e);
        }

        Ok(blocks.len())
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
        let conn = self.chunks.connect()?;
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
    }

    /// Get CFG edges for a function
    pub fn get_cfg_edges_for_function(
        &self,
        function_id: i64,
    ) -> Result<Vec<crate::graph::cfg_edges_extract::CfgEdge>> {
        let conn = self.chunks.connect()?;
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
    }

    pub fn delete_cfg_for_function(&self, function_id: i64) -> Result<usize> {
        let conn = self.chunks.connect()?;
        conn.execute(
            "DELETE FROM cfg_edges WHERE function_id = ?1",
            params![function_id],
        )?;
        let affected = conn.execute(
            "DELETE FROM cfg_blocks WHERE function_id = ?1",
            params![function_id],
        )?;
        Ok(affected)
    }

    pub fn delete_cfg_for_functions(&self, function_ids: &[i64]) -> Result<usize> {
        if function_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.chunks.connect()?;
        let placeholders = function_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
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
    }

    pub fn get_cfg_for_function(&self, function_id: i64) -> Result<Vec<CfgBlock>> {
        let conn = self.chunks.connect()?;
        let mut stmt = conn.prepare(
            "SELECT function_id, kind, terminator,
                    byte_start, byte_end,
                    start_line, start_col,
                    end_line, end_col,
                    cfg_hash, statements,
                    coord_x, coord_y, coord_z, coord_t
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
                    coord_x: row.get(11)?,
                    coord_y: row.get(12)?,
                    coord_z: row.get(13)?,
                    coord_t: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(blocks)
    }

    pub fn get_cfg_for_file(&self, file_path: &str) -> Result<Vec<(i64, Vec<CfgBlock>)>> {
        let conn = self.chunks.connect()?;
        let mut stmt = conn.prepare(
            "SELECT e.id AS function_id,
                    c.function_id, c.kind, c.terminator,
                    c.byte_start, c.byte_end,
                    c.start_line, c.start_col,
                    c.end_line, c.end_col,
                    c.cfg_hash, c.statements,
                    c.coord_x, c.coord_y, c.coord_z, c.coord_t
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
                coord_x: row.get(12)?,
                coord_y: row.get(13)?,
                coord_z: row.get(14)?,
                coord_t: row.get(15)?,
            };
            Ok((function_id, block))
        })?;

        for row in rows {
            let (function_id, block) = row?;
            result.entry(function_id).or_default().push(block);
        }
        Ok(result.into_iter().collect())
    }
}

/// Compute dominator depth (X coordinate) for all CFG blocks
///
/// Dominator depth represents structural hierarchy depth in the control flow.
/// Entry block has depth 0, its immediate children have depth 1, etc.
///
/// Algorithm: Cooper-Harvey-Kennedy for dominator tree construction
pub fn compute_dominator_depth(cfg: &CfgWithEdges) -> HashMap<usize, i64> {
    let mut depth = HashMap::new();

    if cfg.blocks.is_empty() {
        return depth;
    }

    // Build adjacency list for reverse CFG (predecessors)
    let mut predecessors: HashMap<usize, Vec<usize>> = HashMap::new();
    for edge in &cfg.edges {
        predecessors
            .entry(edge.target_idx)
            .or_default()
            .push(edge.source_idx);
    }

    // Ensure all blocks have entries in predecessors
    for i in 0..cfg.blocks.len() {
        predecessors.entry(i).or_default();
    }

    // Find entry block (first block or block with no predecessors)
    let entry_idx = 0; // CFG extraction puts entry block first

    // Initialize dominators: each node dominates itself
    let mut dominators: HashMap<usize, HashSet<usize>> = HashMap::new();
    for i in 0..cfg.blocks.len() {
        let mut set = HashSet::new();
        set.insert(i);
        dominators.insert(i, set);
    }

    // Entry block only dominates itself
    dominators.insert(entry_idx, {
        let mut set = HashSet::new();
        set.insert(entry_idx);
        set
    });

    // Iterative dataflow analysis for dominators
    let mut changed = true;
    while changed {
        changed = false;

        for i in 0..cfg.blocks.len() {
            if i == entry_idx {
                continue;
            }

            // Get intersection of predecessors' dominators
            let preds = predecessors.get(&i).unwrap();
            if preds.is_empty() {
                continue;
            }

            let mut new_doms: HashSet<usize> = (0..cfg.blocks.len()).collect();
            for pred in preds {
                if let Some(pred_doms) = dominators.get(pred) {
                    new_doms = new_doms.intersection(pred_doms).cloned().collect();
                }
            }

            // Add self to dominators
            new_doms.insert(i);

            if let Some(current_doms) = dominators.get(&i) {
                if new_doms != *current_doms {
                    dominators.insert(i, new_doms);
                    changed = true;
                }
            } else {
                dominators.insert(i, new_doms);
                changed = true;
            }
        }
    }

    // Build dominator tree and compute depths
    let mut immediate_dominator: HashMap<usize, Option<usize>> = HashMap::new();
    for i in 0..cfg.blocks.len() {
        if i == entry_idx {
            immediate_dominator.insert(i, None);
            continue;
        }

        let doms = dominators.get(&i).cloned().unwrap_or_default();

        let mut idom = None;

        for d in &doms {
            if *d == i {
                continue;
            }

            // idom(i) is the unique strict dominator of i that does NOT
            // strictly dominate any other strict dominator of i.
            let dominates_another = doms.iter().any(|&s| {
                s != i
                    && s != *d
                    && dominators
                        .get(&s)
                        .map(|set| set.contains(d))
                        .unwrap_or(false)
            });

            if !dominates_another {
                idom = Some(*d);
                break;
            }
        }

        immediate_dominator.insert(i, idom);
    }

    // Compute depths by traversing dominator tree
    fn compute_depth(
        node: usize,
        imm_dom: &HashMap<usize, Option<usize>>,
        cache: &mut HashMap<usize, i64>,
    ) -> i64 {
        if let Some(&depth) = cache.get(&node) {
            return depth;
        }

        let depth = match imm_dom.get(&node) {
            None | Some(None) => 0,
            Some(Some(parent)) => 1 + compute_depth(*parent, imm_dom, cache),
        };

        cache.insert(node, depth);
        depth
    }

    for i in 0..cfg.blocks.len() {
        let d = compute_depth(i, &immediate_dominator, &mut HashMap::new());
        depth.insert(i, d);
    }

    depth
}

/// Compute loop nesting level (Y coordinate) for all CFG blocks
///
/// Loop nesting represents iterative complexity depth.
/// 0 = no loops, 1 = inside one loop, 2 = inside nested loop, etc.
///
/// Algorithm: Detect back-edges and compute nesting depth
pub fn compute_loop_nesting(cfg: &CfgWithEdges) -> HashMap<usize, i64> {
    let mut nesting = HashMap::new();

    if cfg.blocks.is_empty() {
        return nesting;
    }

    // Find back-edges (edges that go to a dominator)
    let mut back_edges: Vec<(usize, usize)> = Vec::new();
    let mut successors: HashMap<usize, Vec<usize>> = HashMap::new();

    for edge in &cfg.edges {
        successors
            .entry(edge.source_idx)
            .or_default()
            .push(edge.target_idx);

        if edge.edge_type == CfgEdgeType::BackEdge {
            back_edges.push((edge.source_idx, edge.target_idx));
        }
    }

    // Build loop headers (targets of back-edges)
    let mut loop_headers: HashSet<usize> = HashSet::new();
    for (_, header) in &back_edges {
        loop_headers.insert(*header);
    }

    // Compute nesting using DFS from each block
    fn compute_nesting_depth(
        block: usize,
        loop_headers: &HashSet<usize>,
        successors: &HashMap<usize, Vec<usize>>,
        visited: &mut HashSet<usize>,
        cache: &mut HashMap<usize, i64>,
        entry_idx: usize,
    ) -> i64 {
        if let Some(&depth) = cache.get(&block) {
            return depth;
        }

        if visited.contains(&block) {
            return 0;
        }

        visited.insert(block);

        // Base depth: 1 if this is a loop header (but not entry block)
        let base_depth = if loop_headers.contains(&block) && block != entry_idx {
            1
        } else {
            0
        };

        // Max depth among successors
        let mut max_child_depth = 0;
        if let Some(succs) = successors.get(&block) {
            for &succ in succs {
                let child_depth = compute_nesting_depth(
                    succ,
                    loop_headers,
                    successors,
                    visited,
                    cache,
                    entry_idx,
                );
                max_child_depth = max_child_depth.max(child_depth);
            }
        }

        let depth = base_depth + max_child_depth;
        cache.insert(block, depth);
        depth
    }

    for i in 0..cfg.blocks.len() {
        let depth = compute_nesting_depth(
            i,
            &loop_headers,
            &successors,
            &mut HashSet::new(),
            &mut HashMap::new(),
            0, // entry block is always at index 0
        );
        nesting.insert(i, depth);
    }

    nesting
}

/// Compute branch distance (Z coordinate) for all CFG blocks
///
/// Branch distance represents decision density from entry.
/// Counts the number of conditional branches on the shortest path.
///
/// Algorithm: BFS from entry block, counting conditional edges
pub fn compute_branch_distance(cfg: &CfgWithEdges) -> HashMap<usize, i64> {
    let mut distance = HashMap::new();

    if cfg.blocks.is_empty() {
        return distance;
    }

    let entry_idx = 0;
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back((entry_idx, 0i64));
    visited.insert(entry_idx);

    // Build adjacency list
    let mut successors: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
    for edge in &cfg.edges {
        let is_conditional = matches!(
            edge.edge_type,
            CfgEdgeType::ConditionalTrue | CfgEdgeType::ConditionalFalse
        );
        successors
            .entry(edge.source_idx)
            .or_default()
            .push((edge.target_idx, is_conditional));
    }

    while let Some((block, dist)) = queue.pop_front() {
        distance.insert(block, dist);

        if let Some(succs) = successors.get(&block) {
            for &(succ, is_cond) in succs {
                if !visited.contains(&succ) {
                    visited.insert(succ);
                    let new_dist = dist + if is_cond { 1 } else { 0 };
                    queue.push_back((succ, new_dist));
                }
            }
        }
    }

    // Fill in unreachable blocks with 0
    for i in 0..cfg.blocks.len() {
        distance.entry(i).or_insert(0);
    }

    distance
}

/// Compute all 4D coordinates for a CFG and update blocks
///
/// This is the main entry point for 4D coordinate computation.
/// It calculates X, Y, Z coordinates and updates the CfgBlock structs.
/// Get current git commit hash for the T coordinate
///
/// Returns the current HEAD commit hash if in a git repository,
/// or None if not in a repository or git is not available.
pub fn get_current_git_commit() -> Option<String> {
    use git2::Repository;

    // Try to open the current directory as a git repository
    if let Ok(repo) = Repository::open(".") {
        // Get HEAD reference
        if let Ok(head) = repo.head() {
            // Get commit hash
            if let Some(commit_oid) = head.target() {
                return Some(commit_oid.to_string());
            }
        }
    }

    None
}

/// Compute all 4D coordinates for a CFG and update blocks
///
/// This is the main entry point for 4D coordinate computation.
/// It calculates X, Y, Z coordinates and updates the CfgBlock structs.
/// The T coordinate (git commit) is set if available.
pub fn compute_4d_coordinates(cfg: &mut CfgWithEdges) {
    compute_4d_coordinates_with_commit(cfg, get_current_git_commit())
}

/// Compute all 4D coordinates for a CFG with explicit commit hash
///
/// Same as `compute_4d_coordinates` but allows specifying the commit hash
/// explicitly for time-travel queries or testing.
pub fn compute_4d_coordinates_with_commit(cfg: &mut CfgWithEdges, commit: Option<String>) {
    let dom_depth = compute_dominator_depth(cfg);
    let loop_nest = compute_loop_nesting(cfg);
    let branch_dist = compute_branch_distance(cfg);

    for (i, block) in cfg.blocks.iter_mut().enumerate() {
        block.coord_x = dom_depth.get(&i).copied().unwrap_or(0);
        block.coord_y = loop_nest.get(&i).copied().unwrap_or(0);
        block.coord_z = branch_dist.get(&i).copied().unwrap_or(0);
        block.coord_t = commit.clone();
    }
}

#[cfg(test)]
mod spatial_tests {
    use super::*;

    fn get_test_language() -> tree_sitter::Language {
        // Use tree-sitter-rust language for testing
        tree_sitter_rust::language()
    }

    #[test]
    fn test_compute_dominator_depth_simple() {
        let source = r#"
        fn main() {
            let x = 1;
            if x > 0 {
                println!("positive");
            } else {
                println!("non-positive");
            }
        }
        "#;

        let cfg =
            crate::graph::cfg_edges_extract::extract_cfg_with_edges(source, 1, get_test_language());
        let depths = compute_dominator_depth(&cfg);

        // Entry block should have depth 0
        assert_eq!(depths.get(&0), Some(&0));

        // All blocks should have a depth
        assert_eq!(depths.len(), cfg.blocks.len());
    }

    #[test]
    fn test_compute_dominator_depth_linear_chain() {
        // Manually construct a 4-block linear chain to verify idom selection
        let cfg = CfgWithEdges {
            function_id: 1,
            blocks: vec![
                CfgBlock {
                    function_id: 1,
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
                    coord_x: 0,
                    coord_y: 0,
                    coord_z: 0,
                    coord_t: None,
                },
                CfgBlock {
                    function_id: 1,
                    kind: "BASIC".to_string(),
                    terminator: "FALLTHROUGH".to_string(),
                    byte_start: 10,
                    byte_end: 20,
                    start_line: 2,
                    start_col: 0,
                    end_line: 2,
                    end_col: 10,
                    cfg_hash: None,
                    statements: None,
                    coord_x: 0,
                    coord_y: 0,
                    coord_z: 0,
                    coord_t: None,
                },
                CfgBlock {
                    function_id: 1,
                    kind: "BASIC".to_string(),
                    terminator: "FALLTHROUGH".to_string(),
                    byte_start: 20,
                    byte_end: 30,
                    start_line: 3,
                    start_col: 0,
                    end_line: 3,
                    end_col: 10,
                    cfg_hash: None,
                    statements: None,
                    coord_x: 0,
                    coord_y: 0,
                    coord_z: 0,
                    coord_t: None,
                },
                CfgBlock {
                    function_id: 1,
                    kind: "EXIT".to_string(),
                    terminator: "RETURN".to_string(),
                    byte_start: 30,
                    byte_end: 40,
                    start_line: 4,
                    start_col: 0,
                    end_line: 4,
                    end_col: 10,
                    cfg_hash: None,
                    statements: None,
                    coord_x: 0,
                    coord_y: 0,
                    coord_z: 0,
                    coord_t: None,
                },
            ],
            edges: vec![
                CfgEdge {
                    source_idx: 0,
                    target_idx: 1,
                    edge_type: CfgEdgeType::Fallthrough,
                },
                CfgEdge {
                    source_idx: 1,
                    target_idx: 2,
                    edge_type: CfgEdgeType::Fallthrough,
                },
                CfgEdge {
                    source_idx: 2,
                    target_idx: 3,
                    edge_type: CfgEdgeType::Fallthrough,
                },
            ],
        };

        let depths = compute_dominator_depth(&cfg);

        // Entry block has depth 0
        assert_eq!(depths.get(&0), Some(&0), "entry should have depth 0");
        // Block 1 is directly dominated by entry
        assert_eq!(depths.get(&1), Some(&1), "block 1 should have depth 1");
        // Block 2 is dominated by entry -> block 1
        assert_eq!(depths.get(&2), Some(&2), "block 2 should have depth 2");
        // Block 3 is dominated by entry -> block 1 -> block 2
        assert_eq!(depths.get(&3), Some(&3), "block 3 should have depth 3");
    }

    #[test]
    fn test_compute_loop_nesting_simple() {
        let source = r#"
        fn main() {
            let mut x = 0;
            while x < 10 {
                x += 1;
            }
        }
        "#;

        let cfg =
            crate::graph::cfg_edges_extract::extract_cfg_with_edges(source, 1, get_test_language());
        let nesting = compute_loop_nesting(&cfg);

        // All blocks should have nesting level
        assert_eq!(nesting.len(), cfg.blocks.len());

        // All nesting levels should be non-negative
        for depth in nesting.values() {
            assert!(depth >= &0);
        }
    }

    #[test]
    fn test_compute_branch_distance_simple() {
        let source = r#"
        fn main() {
            let x = 1;
            if x > 0 {
                println!("positive");
            }
        }
        "#;

        let cfg =
            crate::graph::cfg_edges_extract::extract_cfg_with_edges(source, 1, get_test_language());
        let distance = compute_branch_distance(&cfg);

        // Entry block should have distance 0
        assert_eq!(distance.get(&0), Some(&0));

        // All blocks should have distance
        assert_eq!(distance.len(), cfg.blocks.len());
    }

    #[test]
    fn test_compute_4d_coordinates_integration() {
        let source = r#"
        fn example() {
            let mut x = 0;
            if x > 0 {
                while x < 10 {
                    x += 1;
                }
            } else {
                x = 5;
            }
        }
        "#;

        let mut cfg =
            crate::graph::cfg_edges_extract::extract_cfg_with_edges(source, 1, get_test_language());
        compute_4d_coordinates(&mut cfg);

        // All blocks should have coordinates set
        for block in &cfg.blocks {
            // X, Y, Z should be set (coord_t may be None if not in git repo)
            assert!(block.coord_x >= 0);
            assert!(block.coord_y >= 0);
            assert!(block.coord_z >= 0);
        }
    }

    #[test]
    fn test_git_commit_tracking() {
        // Test that git commit tracking works
        let commit = get_current_git_commit();

        // In a git repository, we should get Some(commit_hash)
        // Outside of git, we get None
        if let Some(hash) = commit {
            // Should be a valid git hash (40 hex characters)
            assert_eq!(hash.len(), 40);
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        } else {
            // Not in a git repository - that's okay for testing
        }
    }

    #[test]
    fn test_compute_4d_coordinates_with_explicit_commit() {
        let source = r#"
        fn example() {
            let x = 1;
        }
        "#;

        let mut cfg =
            crate::graph::cfg_edges_extract::extract_cfg_with_edges(source, 1, get_test_language());

        // Test with explicit commit hash
        let test_commit = Some("abc123def456789".repeat(2)); // 40 chars
        compute_4d_coordinates_with_commit(&mut cfg, test_commit.clone());

        // All blocks should have the specified commit
        for block in &cfg.blocks {
            assert_eq!(block.coord_t, test_commit);
        }
    }

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
                coord_x: 0,
                coord_y: 0,
                coord_z: 0,
                coord_t: None,
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
                coord_x: 1,
                coord_y: 0,
                coord_z: 1,
                coord_t: None,
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
}
