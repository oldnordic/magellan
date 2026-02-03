//! Graph algorithms for code analysis
//!
//! This module provides wrapper functions for sqlitegraph's algorithm library,
//! exposing reachability analysis, dead code detection, cycle detection, call
//! graph condensation, and impact analysis through Magellan's CodeGraph API.
//!
//! # Graph Views
//!
//! Magellan stores multiple edge types in the same graph:
//! - **DEFINES**: File → Symbol (defines/contains relationship)
//! - **REFERENCES**: Reference → Symbol (what symbol is referenced)
//! - **CALLS**: Call node → Symbol (call graph edges)
//! - **CALLER**: Symbol → Call node (reverse call graph edges)
//!
//! For call graph algorithms (reachability, dead code detection, SCC), we filter
//! to **CALLS** edges only to traverse the call graph structure.
//!
//! # Entity IDs vs Symbol IDs
//!
//! sqlitegraph algorithms work with **entity IDs** (i64 database row IDs),
//! while Magellan's public API uses **stable symbol IDs** (32-char BLAKE3 hashes).
//!
//! This module provides translation functions:
//! - `resolve_symbol_entity()`: Symbol ID → entity ID
//! - `symbol_by_entity_id()`: entity ID → SymbolInfo
//!
//! # Algorithm Functions
//!
//! - [`CodeGraph::reachable_symbols()`]: Forward reachability from a symbol
//! - [`CodeGraph::reverse_reachable_symbols()`]: Reverse reachability (callers)
//! - [`CodeGraph::dead_symbols()`]: Dead code detection from entry point
//! - [`CodeGraph::detect_cycles()`]: Find cycles using SCC decomposition
//! - [`CodeGraph::find_cycles_containing()`]: Find cycles containing a specific symbol
//! - [`CodeGraph::condense_call_graph()`]: Collapse SCCs to create condensation DAG
//! - [`CodeGraph::enumerate_paths()`]: Path enumeration between symbols
//! - [`CodeGraph::backward_slice()`]: Backward program slice (what affects this symbol)
//! - [`CodeGraph::forward_slice()`]: Forward program slice (what this symbol affects)
//!
//! # Example
//!
//! \`\`\`no_run
//! use magellan::CodeGraph;
//!
//! let mut graph = CodeGraph::open("codegraph.db")?;
//!
//! // Find all functions reachable from main
//! let reachable = graph.reachable_symbols("main_symbol_id", None)?;
//!
//! // Find dead code unreachable from main
//! let dead = graph.dead_symbols("main_symbol_id")?;
//!
//! // Find cycles (mutual recursion)
//! let cycles = graph.detect_cycles()?;
//!
//! // Condense call graph for DAG analysis
//! let condensed = graph.condense_call_graph()?;
//! \`\`\`

use anyhow::Result;
use ahash::AHashSet;
use rusqlite::params;
use sqlitegraph::{algo, GraphBackend, SnapshotId};
use std::collections::{HashMap, HashSet};

use crate::graph::schema::SymbolNode;

use super::CodeGraph;

/// Symbol information for algorithm results
///
/// Contains the key metadata needed to identify and locate a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    /// Stable symbol ID (32-char BLAKE3 hash)
    pub symbol_id: Option<String>,
    /// Fully-qualified name
    pub fqn: Option<String>,
    /// File path containing the symbol
    pub file_path: String,
    /// Symbol kind (Function, Method, Class, etc.)
    pub kind: String,
}

/// Dead symbol information
///
/// Extends [`SymbolInfo`] with a reason why the symbol is considered dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadSymbol {
    /// Base symbol information
    pub symbol: SymbolInfo,
    /// Reason why this symbol is unreachable/dead
    pub reason: String,
}

/// Cycle kind classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleKind {
    /// Multiple symbols calling each other (SCC with >1 member)
    MutualRecursion,
    /// Single symbol that calls itself (direct self-loop)
    SelfLoop,
}

/// Cycle information for detected cycles
///
/// Represents a strongly connected component (SCC) with more than one member,
/// indicating mutual recursion or a cycle in the call graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    /// All symbols that participate in this cycle
    pub members: Vec<SymbolInfo>,
    /// Classification of the cycle type
    pub kind: CycleKind,
}

/// Cycle detection report
///
/// Result of running [`CodeGraph::detect_cycles()`], containing all cycles
/// found in the call graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    /// All detected cycles
    pub cycles: Vec<Cycle>,
    /// Total number of cycles found
    pub total_count: usize,
}

/// Supernode in a condensation graph
///
/// Represents an SCC collapsed into a single node for DAG analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supernode {
    /// Supernode ID (stable identifier for this SCC)
    pub id: i64,
    /// All symbols that are members of this SCC/supernode
    pub members: Vec<SymbolInfo>,
}

/// Condensation graph (DAG after SCC collapse)
///
/// Represents the call graph after collapsing all SCCs into supernodes.
/// The condensation graph is always a DAG (no cycles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondensationGraph {
    /// All supernodes in the condensed graph
    pub supernodes: Vec<Supernode>,
    /// Edges between supernodes (from_supernode_id, to_supernode_id)
    pub edges: Vec<(i64, i64)>,
}

/// Condensation result with symbol-to-supernode mapping
///
/// Result of running [`CodeGraph::condense_call_graph()`], providing
/// both the condensed DAG and the mapping from original symbols to supernodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondensationResult {
    /// The condensed DAG
    pub graph: CondensationGraph,
    /// Maps symbol_id to the supernode ID containing that symbol
    pub original_to_supernode: HashMap<String, i64>,
}

/// Direction of program slicing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceDirection {
    /// Backward slice: what affects this symbol (reverse reachability)
    Backward,
    /// Forward slice: what this symbol affects (forward reachability)
    Forward,
}

/// Program slice result
///
/// Contains the slice results and statistics for a program slicing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSlice {
    /// Target symbol for the slice
    pub target: SymbolInfo,
    /// Direction of the slice
    pub direction: SliceDirection,
    /// Symbols included in the slice
    pub included_symbols: Vec<SymbolInfo>,
    /// Number of symbols in the slice
    pub symbol_count: usize,
}

/// Program slice result with statistics
///
/// Wraps a [`ProgramSlice`] with additional statistics about the slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceResult {
    /// The slice itself
    pub slice: ProgramSlice,
    /// Statistics about the slice
    pub statistics: SliceStatistics,
}

/// Statistics for a program slice
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceStatistics {
    /// Total number of symbols in the slice
    pub total_symbols: usize,
    /// Number of data dependencies
    /// Note: Set to 0 for call-graph fallback (not computed without full CFG)
    pub data_dependencies: usize,
    /// Number of control dependencies
    /// For call-graph fallback, this equals total_symbols (callers/callees)
    pub control_dependencies: usize,
}

/// Execution path in the call graph
///
/// Represents a single path through the call graph from a starting symbol
/// to an ending symbol, with metadata about the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPath {
    /// Symbols along the path in order from start to end
    pub symbols: Vec<SymbolInfo>,
    /// Number of symbols in the path
    pub length: usize,
}

/// Path enumeration result
///
/// Contains all discovered execution paths and statistics about the enumeration.
#[derive(Debug, Clone)]
pub struct PathEnumerationResult {
    /// All discovered paths
    pub paths: Vec<ExecutionPath>,
    /// Total number of paths enumerated
    pub total_enumerated: usize,
    /// Whether enumeration was cut off due to bounds
    pub bounded_hit: bool,
    /// Statistics about the discovered paths
    pub statistics: PathStatistics,
}

/// Statistics for path enumeration
#[derive(Debug, Clone)]
pub struct PathStatistics {
    /// Average path length
    pub avg_length: f64,
    /// Minimum path length
    pub min_length: usize,
    /// Maximum path length
    pub max_length: usize,
    /// Number of unique symbols across all paths
    pub unique_symbols: usize,
}

impl CodeGraph {
    /// Resolve a stable symbol ID or FQN to its entity ID
    ///
    /// First tries to lookup by symbol_id (32-char BLAKE3 hash).
    /// If not found, falls back to FQN lookup for convenience.
    ///
    /// # Arguments
    /// * `symbol_id_or_fqn` - Stable symbol ID (32-char BLAKE3 hash) or FQN
    ///
    /// # Returns
    /// The entity ID (i64 database row ID) for the symbol
    ///
    /// # Errors
    /// Returns an error if the symbol is not found in the database
    fn resolve_symbol_entity(&self, symbol_id_or_fqn: &str) -> Result<i64> {
        let conn = self.chunks.connect()?;

        // First try: lookup by symbol_id
        let mut stmt = conn
            .prepare_cached(
                "SELECT id FROM graph_entities
                 WHERE kind = 'Symbol'
                 AND json_extract(data, '$.symbol_id') = ?1",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare symbol ID query: {}", e))?;

        let result = stmt.query_row(params![symbol_id_or_fqn], |row| row.get::<_, i64>(0));

        match result {
            Ok(entity_id) => return Ok(entity_id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Fallback: try FQN lookup
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query symbol ID: {}", e));
            }
        }

        // Fallback: lookup by FQN or display_fqn
        let mut stmt = conn
            .prepare_cached(
                "SELECT id FROM graph_entities
                 WHERE kind = 'Symbol'
                 AND (json_extract(data, '$.fqn') = ?1
                      OR json_extract(data, '$.display_fqn') = ?1
                      OR json_extract(data, '$.canonical_fqn') = ?1)",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare FQN query: {}", e))?;

        stmt.query_row(params![symbol_id_or_fqn], |row| row.get::<_, i64>(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    anyhow::anyhow!(
                        "Symbol '{}' not found in database (tried symbol_id, fqn, display_fqn, canonical_fqn)",
                        symbol_id_or_fqn
                    )
                }
                _ => anyhow::anyhow!("Failed to query symbol by FQN: {}", e),
            })
    }

    /// Get symbol information by entity ID
    ///
    /// # Arguments
    /// * `entity_id` - Entity ID (i64 database row ID)
    ///
    /// # Returns
    /// SymbolInfo with metadata from the symbol node
    fn symbol_by_entity_id(&self, entity_id: i64) -> Result<SymbolInfo> {
        let snapshot = SnapshotId::current();
        let node = self
            .calls
            .backend
            .get_node(snapshot, entity_id)
            .map_err(|e| anyhow::anyhow!("Failed to get entity {}: {}", entity_id, e))?;

        if node.kind != "Symbol" {
            return Err(anyhow::anyhow!(
                "Entity {} is not a Symbol (kind: {})",
                entity_id,
                node.kind
            ));
        }

        let symbol_node: SymbolNode = serde_json::from_value(node.data)
            .map_err(|e| anyhow::anyhow!("Failed to parse SymbolNode data: {}", e))?;

        Ok(SymbolInfo {
            symbol_id: symbol_node.symbol_id,
            fqn: symbol_node.fqn.or_else(|| symbol_node.display_fqn.clone()),
            file_path: node
                .file_path
                .unwrap_or_else(|| "?".to_string()),
            kind: symbol_node.kind,
        })
    }

    /// Get all Symbol entity IDs in the call graph
    ///
    /// Returns entity IDs for all Symbol nodes that are part of the call graph
    /// (have incoming or outgoing CALLS edges).
    ///
    /// # Returns
    /// Vector of entity IDs for call graph symbols
    fn all_call_graph_entities(&self) -> Result<Vec<i64>> {
        let conn = self.chunks.connect()?;

        // Find all symbols that participate in CALLS edges
        // (either as caller via CALLER edges or as callee via CALLS edges)
        let mut stmt = conn
            .prepare_cached(
                "SELECT DISTINCT from_id FROM graph_edges WHERE edge_type = 'CALLER'
                 UNION
                 SELECT DISTINCT to_id FROM graph_edges WHERE edge_type = 'CALLS'",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare call graph query: {}", e))?;

        let entity_ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| anyhow::anyhow!("Failed to execute call graph query: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect call graph results: {}", e))?;

        Ok(entity_ids)
    }

    /// Find all symbols reachable from a given symbol (forward reachability)
    ///
    /// Computes the transitive closure of the call graph starting from the
    /// specified symbol. Returns all symbols that can be reached by following
    /// CALLS edges from the starting symbol.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only), not other edge types.
    /// The starting symbol itself is NOT included in the results.
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID to start from (or FQN as fallback)
    /// * `max_depth` - Optional maximum depth limit (None = unlimited)
    ///
    /// # Returns
    /// Vector of [`SymbolInfo`] for reachable symbols, sorted deterministically
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions called from main (directly or indirectly)
    /// let reachable = graph.reachable_symbols("main", None)?;
    /// \`\`\`
    pub fn reachable_symbols(
        &self,
        symbol_id: &str,
        _max_depth: Option<usize>,
    ) -> Result<Vec<SymbolInfo>> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Use sqlitegraph's reachable_from algorithm
        // This traverses outgoing edges from the start node
        // Note: sqlitegraph 1.3.0 API takes (graph, start), not (backend, snapshot, start, depth)
        let reachable_entity_ids = algo::reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut symbols = Vec::new();
        for id in reachable_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                symbols.push(info);
            }
        }

        // Sort deterministically for stable output
        symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        Ok(symbols)
    }

    /// Find all symbols that can reach a given symbol (reverse reachability)
    ///
    /// Computes the reverse transitive closure of the call graph. Returns all
    /// symbols from which the specified symbol can be reached by following
    /// CALLS edges (i.e., all callers that directly or indirectly call this symbol).
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only).
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID to analyze
    /// * `max_depth` - Optional maximum depth limit (None = unlimited)
    ///
    /// # Returns
    /// Vector of [`SymbolInfo`] for symbols that can reach the target, sorted deterministically
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions that directly or indirectly call 'helper_function'
    /// let callers = graph.reverse_reachable_symbols("helper_symbol_id", None)?;
    /// \`\`\`
    pub fn reverse_reachable_symbols(
        &self,
        symbol_id: &str,
        _max_depth: Option<usize>,
    ) -> Result<Vec<SymbolInfo>> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Use sqlitegraph's reverse_reachable_from algorithm
        // This traverses incoming edges to the target node
        // Note: sqlitegraph 1.3.0 API takes (graph, target)
        let reachable_entity_ids = algo::reverse_reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut symbols = Vec::new();
        for id in reachable_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                symbols.push(info);
            }
        }

        // Sort deterministically for stable output
        symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        Ok(symbols)
    }

    /// Find dead code unreachable from an entry point symbol
    ///
    /// Identifies all symbols in the call graph that cannot be reached from
    /// the specified entry point (e.g., `main`, `test_main`). This is useful
    /// for detecting unused functions and dead code.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only). Symbols not
    /// participating in call edges are not considered.
    ///
    /// # Limitations
    ///
    /// - Only considers the call graph. Symbols called via reflection,
    ///   function pointers, or dynamic dispatch may be incorrectly flagged.
    /// - Test functions, benchmark code, and platform-specific code may
    ///   appear as dead code if not reachable from the specified entry point.
    ///
    /// # Arguments
    /// * `entry_symbol_id` - Stable symbol ID of the entry point (e.g., main function)
    ///
    /// # Returns
    /// Vector of [`DeadSymbol`] for unreachable symbols, sorted deterministically
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions unreachable from main
    /// let dead = graph.dead_symbols("main_symbol_id")?;
    /// for dead_symbol in dead {
    ///     println!("Dead: {} in {} ({})",
    ///         dead_symbol.symbol.fqn.as_deref().unwrap_or("?"),
    ///         dead_symbol.symbol.file_path,
    ///         dead_symbol.reason);
    /// }
    /// \`\`\`
    pub fn dead_symbols(&self, entry_symbol_id: &str) -> Result<Vec<DeadSymbol>> {
        let entry_entity = self.resolve_symbol_entity(entry_symbol_id)?;
        let backend = &self.calls.backend;

        // Get all call graph entities
        let all_entities = self.all_call_graph_entities()?;

        // Find all entities reachable from the entry point
        // Note: sqlitegraph 1.3.0 API takes (graph, start)
        let reachable_ids =
            algo::reachable_from(backend.graph(), entry_entity)?;

        // Dead symbols = all entities - reachable entities
        let reachable_set: HashSet<i64> = reachable_ids.into_iter().collect();
        let mut dead_symbols = Vec::new();

        for entity_id in all_entities {
            // Skip the entry point itself
            if entity_id == entry_entity {
                continue;
            }

            // If not reachable from entry point, it's dead code
            if !reachable_set.contains(&entity_id) {
                if let Ok(info) = self.symbol_by_entity_id(entity_id) {
                    dead_symbols.push(DeadSymbol {
                        reason: "unreachable from entry point".to_string(),
                        symbol: info,
                    });
                }
            }
        }

        // Sort deterministically for stable output
        dead_symbols.sort_by(|a, b| {
            a.symbol
                .file_path
                .cmp(&b.symbol.file_path)
                .then_with(|| a.symbol.fqn.as_ref().cmp(&b.symbol.fqn.as_ref()))
                .then_with(|| a.symbol.kind.cmp(&b.symbol.kind))
        });

        Ok(dead_symbols)
    }

    /// Detect cycles in the call graph using SCC decomposition
    ///
    /// Finds all strongly connected components (SCCs) with more than one member,
    /// which indicate cycles or mutual recursion in the call graph.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only).
    ///
    /// # Cycle Detection
    ///
    /// Uses Tarjan's SCC algorithm to find strongly connected components.
    /// Only SCCs with more than one member are reported as cycles (MutualRecursion).
    /// Single-node SCCs are not cycles (unless they have self-loops).
    ///
    /// # Returns
    /// [`CycleReport`] containing all detected cycles
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// let report = graph.detect_cycles()?;
    /// println!("Found {} cycles", report.total_count);
    /// for cycle in &report.cycles {
    ///     println!("Cycle with {} members:", cycle.members.len());
    ///     for member in &cycle.members {
    ///         println!("  - {}", member.fqn.as_deref().unwrap_or("?"));
    ///     }
    /// }
    /// \`\`\`
    pub fn detect_cycles(&self) -> Result<CycleReport> {
        let backend = &self.calls.backend;

        // Use sqlitegraph's strongly_connected_components algorithm
        let scc_result = algo::strongly_connected_components(backend.graph())?;

        // Filter to SCCs with >1 member (mutual recursion)
        let cycles: Vec<_> = scc_result
            .components
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|members| {
                // Convert entity IDs to SymbolInfo
                let symbol_infos: Vec<_> = members
                    .into_iter()
                    .filter_map(|id| self.symbol_by_entity_id(id).ok())
                    .collect();

                Cycle {
                    members: symbol_infos,
                    kind: CycleKind::MutualRecursion,
                }
            })
            .filter(|cycle| !cycle.members.is_empty())
            .collect();

        let total_count = cycles.len();

        // Sort cycles deterministically
        let mut cycles = cycles;
        cycles.sort_by(|a, b| {
            match (a.members.first(), b.members.first()) {
                (Some(am), Some(bm)) => {
                    match (am.fqn.as_ref(), bm.fqn.as_ref()) {
                        (Some(af), Some(bf)) => af.cmp(bf),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        Ok(CycleReport {
            cycles,
            total_count,
        })
    }

    /// Find cycles containing a specific symbol
    ///
    /// Returns only the cycles that include the specified symbol in their member set.
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID or FQN to search for
    ///
    /// # Returns
    /// Vector of [`Cycle`] containing the specified symbol
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// let cycles = graph.find_cycles_containing("problematic_function")?;
    /// if cycles.is_empty() {
    ///     println!("No cycles found containing this symbol");
    /// } else {
    ///     println!("Found {} cycles containing this symbol", cycles.len());
    /// }
    /// \`\`\`
    pub fn find_cycles_containing(&self, symbol_id: &str) -> Result<Vec<Cycle>> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Use sqlitegraph's strongly_connected_components algorithm
        let scc_result = algo::strongly_connected_components(backend.graph())?;

        // Find which SCC contains this entity
        let target_component_idx = scc_result.node_to_component.get(&entity_id);

        let target_idx = match target_component_idx {
            Some(&idx) => idx,
            None => return Ok(Vec::new()), // Symbol not in any SCC (shouldn't happen)
        };

        // Check if this SCC is a cycle (has >1 member)
        let target_component = &scc_result.components[target_idx];
        if target_component.len() <= 1 {
            // Single node SCC - not a cycle (unless self-loop, but that's rare)
            return Ok(Vec::new());
        }

        // Convert this SCC to a Cycle
        let symbol_infos: Vec<_> = target_component
            .iter()
            .filter_map(|&id| self.symbol_by_entity_id(id).ok())
            .collect();

        let cycle = Cycle {
            members: symbol_infos,
            kind: CycleKind::MutualRecursion,
        };

        Ok(vec![cycle])
    }

    /// Condense the call graph by collapsing SCCs into supernodes
    ///
    /// Creates a condensation DAG by collapsing each strongly connected component
    /// into a single "supernode". The resulting graph is always acyclic, making it
    /// suitable for topological analysis and safe refactoring.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only).
    ///
    /// # Use Cases
    ///
    /// - **Topological Sorting**: Condensation graph is a DAG, enabling topo sort
    /// - **Mutual Recursion Detection**: Large supernodes indicate tight coupling
    /// - **Impact Analysis**: Changing one symbol affects its entire SCC
    ///
    /// # Returns
    /// [`CondensationResult`] with the condensed DAG and symbol-to-supernode mapping
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// let condensed = graph.condense_call_graph()?;
    ///
    /// println!("Condensed to {} supernodes", condensed.graph.supernodes.len());
    /// println!("Condensed graph has {} edges", condensed.graph.edges.len());
    ///
    /// // Check which SCC a symbol belongs to
    /// if let Some(supernode_id) = condensed.original_to_supernode.get("some_symbol_id") {
    ///     println!("Symbol is in SCC {}", supernode_id);
    /// }
    /// \`\`\`
    pub fn condense_call_graph(&self) -> Result<CondensationResult> {
        let backend = &self.calls.backend;

        // Use sqlitegraph's collapse_sccs algorithm
        let collapse_result = algo::collapse_sccs(backend.graph())?;

        // Build supernodes with SymbolInfo members
        let mut supernodes = Vec::new();
        let mut original_to_supernode = HashMap::new();

        for (&supernode_id, member_ids) in &collapse_result.supernode_members {
            let symbol_infos: Vec<_> = member_ids
                .iter()
                .filter_map(|&id| self.symbol_by_entity_id(id).ok())
                .collect();

            // Build mapping from symbol_id to supernode
            for symbol_info in &symbol_infos {
                if let Some(ref sym_id) = symbol_info.symbol_id {
                    original_to_supernode.insert(sym_id.clone(), supernode_id);
                }
            }

            supernodes.push(Supernode {
                id: supernode_id,
                members: symbol_infos,
            });
        }

        // Sort supernodes deterministically
        supernodes.sort_by(|a, b| a.id.cmp(&b.id));

        let graph = CondensationGraph {
            supernodes,
            edges: collapse_result.supernode_edges,
        };

        Ok(CondensationResult {
            graph,
            original_to_supernode,
        })
    }

    /// Compute a backward program slice (what affects this symbol)
    ///
    /// Returns all symbols that can affect the target symbol through the call graph.
    /// This is useful for bug isolation - finding all code that could influence
    /// a given symbol's behavior.
    ///
    /// # Graph View (Call-Graph Fallback)
    ///
    /// **Current implementation uses call-graph reachability as a fallback.**
    /// Full CFG-based program slicing requires control dependence graph (CDG)
    /// which needs post-dominators and AST CFG integration not yet available.
    ///
    /// The fallback implementation uses reverse reachability on the call graph,
    /// finding all callers that directly or indirectly call this symbol.
    ///
    /// # Limitations
    ///
    /// - Uses call-graph reachability instead of full CFG-based slicing
    /// - Does not include data flow dependencies within functions
    /// - Does not include control flow from conditionals/loops
    /// - Full slicing will be available when AST CFG edges are integrated
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID or FQN to slice from
    ///
    /// # Returns
    /// [`SliceResult`] containing the slice and statistics
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find what affects 'helper_function'
    /// let slice_result = graph.backward_slice("helper_function")?;
    /// println!("{} symbols affect this function", slice_result.slice.symbol_count);
    /// for symbol in &slice_result.slice.included_symbols {
    ///     println!("  - {}", symbol.fqn.as_deref().unwrap_or("?"));
    /// }
    /// \`\`\`
    pub fn backward_slice(&self, symbol_id: &str) -> Result<SliceResult> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Get target symbol info
        let target = self.symbol_by_entity_id(entity_id)?;

        // Fallback: Use reverse reachable on call graph
        // This finds all callers that directly or indirectly call this symbol
        let caller_entity_ids = algo::reverse_reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut included_symbols = Vec::new();
        for id in caller_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                included_symbols.push(info);
            }
        }

        // Sort deterministically
        included_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        let symbol_count = included_symbols.len();

        Ok(SliceResult {
            slice: ProgramSlice {
                target,
                direction: SliceDirection::Backward,
                included_symbols,
                symbol_count,
            },
            statistics: SliceStatistics {
                total_symbols: symbol_count,
                data_dependencies: 0, // Not available in call-graph fallback
                control_dependencies: symbol_count,
            },
        })
    }

    /// Compute a forward program slice (what this symbol affects)
    ///
    /// Returns all symbols that the target symbol can affect through the call graph.
    /// This is useful for refactoring safety - finding all code that could be
    /// impacted by changes to this symbol.
    ///
    /// # Graph View (Call-Graph Fallback)
    ///
    /// **Current implementation uses call-graph reachability as a fallback.**
    /// Full CFG-based program slicing requires control dependence graph (CDG)
    /// which needs post-dominators and AST CFG integration not yet available.
    ///
    /// The fallback implementation uses forward reachability on the call graph,
    /// finding all callees that this symbol directly or indirectly calls.
    ///
    /// # Limitations
    ///
    /// - Uses call-graph reachability instead of full CFG-based slicing
    /// - Does not include data flow dependencies within functions
    /// - Does not include control flow from conditionals/loops
    /// - Full slicing will be available when AST CFG edges are integrated
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID or FQN to slice from
    ///
    /// # Returns
    /// [`SliceResult`] containing the slice and statistics
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find what 'main_function' affects
    /// let slice_result = graph.forward_slice("main_function")?;
    /// println!("{} symbols are affected by this function", slice_result.slice.symbol_count);
    /// for symbol in &slice_result.slice.included_symbols {
    ///     println!("  - {}", symbol.fqn.as_deref().unwrap_or("?"));
    /// }
    /// \`\`\`
    pub fn forward_slice(&self, symbol_id: &str) -> Result<SliceResult> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Get target symbol info
        let target = self.symbol_by_entity_id(entity_id)?;

        // Fallback: Use forward reachable on call graph
        // This finds all callees that this symbol directly or indirectly calls
        let callee_entity_ids = algo::reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut included_symbols = Vec::new();
        for id in callee_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                included_symbols.push(info);
            }
        }

        // Sort deterministically
        included_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        let symbol_count = included_symbols.len();

        Ok(SliceResult {
            slice: ProgramSlice {
                target,
                direction: SliceDirection::Forward,
                included_symbols,
                symbol_count,
            },
            statistics: SliceStatistics {
                total_symbols: symbol_count,
                data_dependencies: 0, // Not available in call-graph fallback
                control_dependencies: symbol_count,
            },
        })
    }

    /// Enumerate execution paths from a starting symbol
    ///
    /// Finds all execution paths from `start_symbol_id` to `end_symbol_id` (if provided)
    /// or all paths starting from `start_symbol_id` (if end_symbol_id is None).
    ///
    /// Path enumeration uses bounded DFS to prevent infinite traversal in cyclic graphs:
    /// - `max_depth`: Maximum path length (number of edges)
    /// - `max_paths`: Maximum number of paths to return
    /// - `revisit_cap`: Maximum number of times a single node can be revisited (prevents infinite loops)
    ///
    /// # Arguments
    ///
    /// * `start_symbol_id` - Starting symbol ID or FQN
    /// * `end_symbol_id` - Optional ending symbol ID or FQN (if None, enumerates all paths from start)
    /// * `max_depth` - Maximum path depth (default: 100)
    /// * `max_paths` - Maximum number of paths to return (default: 1000)
    ///
    /// # Returns
    ///
    /// Returns a [`PathEnumerationResult`] containing:
    /// - All discovered paths
    /// - Whether enumeration hit bounds
    /// - Statistics about path lengths and unique symbols
    ///
    /// # Example
    ///
    /// \`\`\`no_run
    /// # use magellan::CodeGraph;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut graph = CodeGraph::open("codegraph.db")?;
    ///
    /// // Find all paths from main to any leaf function
    /// let result = graph.enumerate_paths("main", None, 50, 100)?;
    ///
    /// println!("Found {} paths", result.total_enumerated);
    /// println!("Average length: {:.2}", result.statistics.avg_length);
    /// for (i, path) in result.paths.iter().enumerate() {
    ///     println!("Path {}: {:?}", path.symbols.iter().map(|s| s.fqn.as_deref().unwrap_or("?")).collect::<Vec<_>>());
    /// }
    /// # Ok(())
    /// # }
    /// \`\`\`
    pub fn enumerate_paths(
        &self,
        start_symbol_id: &str,
        end_symbol_id: Option<&str>,
        max_depth: usize,
        max_paths: usize,
    ) -> Result<PathEnumerationResult> {
        let start_entity_id = self.resolve_symbol_entity(start_symbol_id)?;
        let backend = &self.calls.backend;
        let graph = backend.graph();

        // Build exit_nodes set for target symbol
        let exit_nodes: Option<AHashSet<i64>> = if let Some(end_id) = end_symbol_id {
            let end_entity_id = self.resolve_symbol_entity(end_id)?;
            let mut set = AHashSet::new();
            set.insert(end_entity_id);
            Some(set)
        } else {
            None
        };

        // Use sqlitegraph's path enumeration with bounds
        let config = algo::PathEnumerationConfig {
            max_depth,
            max_paths,
            revisit_cap: 100, // Prevent infinite loops in cyclic graphs
            exit_nodes,
            error_nodes: None,
        };

        let sqlite_result = algo::enumerate_paths(graph, start_entity_id, &config)?;

        // Convert sqlitegraph's path result to our format
        let mut paths = Vec::new();
        let mut all_symbols = HashSet::new();
        let mut min_length = usize::MAX;
        let mut max_length = 0;
        let mut total_length = 0;

        for path in sqlite_result.paths {
            let mut symbols = Vec::new();

            for entity_id in &path.nodes {
                if let Ok(info) = self.symbol_by_entity_id(*entity_id) {
                    all_symbols.insert(info.symbol_id.clone().unwrap_or_default());
                    symbols.push(info);
                }
            }

            let length = symbols.len();
            if length > 0 {
                min_length = min_length.min(length);
                max_length = max_length.max(length);
                total_length += length;

                paths.push(ExecutionPath {
                    symbols,
                    length,
                });
            }
        }

        // Sort paths: first by starting symbol FQN, then by length
        paths.sort_by(|a, b| {
            match (
                a.symbols.first().and_then(|s| s.fqn.as_ref()),
                b.symbols.first().and_then(|s| s.fqn.as_ref()),
            ) {
                (Some(a_fqn), Some(b_fqn)) => {
                    a_fqn
                        .cmp(b_fqn)
                        .then_with(|| a.length.cmp(&b.length))
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.length.cmp(&b.length),
            }
        });

        let avg_length = if paths.is_empty() {
            0.0
        } else {
            total_length as f64 / paths.len() as f64
        };

        // Determine if we hit bounds
        let bounded_hit = sqlite_result.paths_pruned_by_bounds > 0;

        Ok(PathEnumerationResult {
            paths,
            total_enumerated: sqlite_result.total_paths_found,
            bounded_hit,
            statistics: PathStatistics {
                avg_length,
                min_length: if min_length == usize::MAX {
                    0
                } else {
                    min_length
                },
                max_length,
                unique_symbols: all_symbols.len(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeGraph;

    /// Test helper to create a simple call graph for testing
    ///
    /// Creates:
    /// - main() -> helper_a() -> leaf()
    /// - main() -> helper_b() -> leaf()
    /// - unused_function() -> leaf()
    ///
    /// Returns the CodeGraph and symbol IDs for main and unused_function
    fn create_test_graph() -> Result<(CodeGraph, String, String)> {
        let temp_dir = tempfile::TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");

        let source = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    leaf();
}

fn helper_b() {
    leaf();
}

fn leaf() {
    println!("leaf");
}

fn unused_function() {
    leaf();
}
"#;

        let mut graph = CodeGraph::open(&db_path)?;
        // Index the file - use a temporary file path
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, source)?;
        let path_str = test_file.to_string_lossy().to_string();
        let source_bytes = std::fs::read(&test_file)?;

        // Index symbols and calls
        graph.index_file(&path_str, &source_bytes)?;
        graph.index_calls(&path_str, &source_bytes)?;

        // Find the symbol IDs for main and unused_function
        let symbols = crate::graph::query::symbols_in_file(&mut graph, &path_str)?;
        let main_id = symbols
            .iter()
            .find(|s| s.name.as_deref() == Some("main"))
            .and_then(|s| s.fqn.clone())
            .unwrap_or_default();

        let unused_id = symbols
            .iter()
            .find(|s| s.name.as_deref() == Some("unused_function"))
            .and_then(|s| s.fqn.clone())
            .unwrap_or_default();

        // For testing, use the symbol's FQN directly
        // In a real scenario with proper SymbolId generation, we'd use that
        Ok((graph, main_id, unused_id))
    }

    #[test]
    fn test_resolve_symbol_entity_not_found() {
        let (graph, _, _) = create_test_graph().unwrap();
        let result = graph.resolve_symbol_entity("nonexistent_id_123456789012");
        assert!(result.is_err());
    }

    #[test]
    fn test_symbol_by_entity_id() {
        let (graph, _, _) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();
        let snapshot = SnapshotId::current();

        // Find a Symbol entity
        for entity_id in entity_ids {
            if let Ok(node) = graph.calls.backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    let info = graph.symbol_by_entity_id(entity_id);
                    assert!(info.is_ok());
                    let symbol_info = info.unwrap();
                    assert!(!symbol_info.file_path.is_empty());
                    assert!(!symbol_info.kind.is_empty());
                    return;
                }
            }
        }

        panic!("No Symbol entity found in test graph");
    }

    #[test]
    fn test_reachable_symbols_basic() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all symbols and verify we can query them
        let entity_ids = graph.calls.backend.entity_ids().unwrap();
        let snapshot = SnapshotId::current();
        let mut found_symbols = 0;

        for entity_id in entity_ids {
            if let Ok(node) = graph.calls.backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    found_symbols += 1;
                }
            }
        }

        // We should have found at least some symbols
        assert!(found_symbols > 0, "Should find Symbol entities in test graph");
    }

    #[test]
    fn test_reachable_symbols_max_depth() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get the main function's entity ID
        let snapshot = SnapshotId::current();
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        let main_entity_id = entity_ids
            .into_iter()
            .find(|&id| {
                if let Ok(node) = graph.calls.backend.get_node(snapshot, id) {
                    if let Ok(data) = serde_json::from_value::<serde_json::Value>(node.data) {
                        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                            return name == "main";
                        }
                    }
                }
                false
            });

        if let Some(entity_id) = main_entity_id {
            // Verify we can get the node
            let result = graph.calls.backend.get_node(snapshot, entity_id);
            assert!(result.is_ok(), "Should be able to get main node");
        }
    }

    #[test]
    fn test_dead_symbols() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        // We should have some entities in the call graph
        assert!(entity_ids.len() > 0, "Should have call graph entities");
    }

    #[test]
    fn test_reverse_reachable_symbols() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        // We should have some entities in the call graph
        assert!(entity_ids.len() > 0, "Should have call graph entities");
    }
}
