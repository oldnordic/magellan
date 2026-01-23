//! Ambiguity tracking for CodeGraph
//!
//! Provides centralized API contract for ambiguity operations using graph structure.
//!
//! # Ambiguity Tracking
//!
//! Ambiguity occurs when multiple symbols share the same Display FQN
//! (human-readable name) but have different Canonical FQNs (unique identity).
//! Examples:
//! - Two functions named `parse` in different files
//! - Multiple `Handler` types across different modules
//!
//! # Graph-Based Ambiguity Model
//!
//! Ambiguity is tracked using `alias_of` edges:
//! - DisplayName node: Represents the human-readable name (Display FQN)
//! - alias_of edges: Connect DisplayName to each Symbol node with that Display FQN
//!
//! This approach:
//! - Aligns with sqlitegraph's node/edge model
//! - Enables transactional updates (symbol deletion cascades)
//! - Reuses existing edge query APIs

use anyhow::Result;
use sqlitegraph::{GraphBackend, NodeId};
use crate::graph::schema::SymbolNode;

/// Ambiguity operations for CodeGraph
///
/// This trait defines the API contract for ambiguity tracking, enabling
/// explicit graph-based resolution of Display FQN collisions.
///
/// # Pattern
///
/// This follows RESEARCH.md Pattern 2 from Phase 24:
/// - Graph-based tracking using alias_of edges
/// - No custom tables (uses sqlitegraph node/edge model)
/// - Three core operations: create groups, resolve, enumerate
///
/// # Methods
///
/// 1. **create_ambiguous_group**: Establish or update an ambiguity group for a Display FQN
/// 2. **resolve_by_symbol_id**: Resolve a Display FQN to a specific SymbolId
/// 3. **get_candidates**: Enumerate all SymbolIds for a Display FQN
pub trait AmbiguityOps {
    /// Create or update an ambiguity group for a Display FQN
    ///
    /// This operation establishes a graph structure linking a DisplayName node
    /// to multiple Symbol nodes via `alias_of` edges.
    ///
    /// # Behavior
    ///
    /// 1. Find or create DisplayName node for the given `display_fqn`
    /// 2. Create `alias_of` edges from DisplayName to each Symbol in `symbol_ids`
    /// 3. If DisplayName already exists with edges, this updates the group
    ///
    /// # Graph Structure
    ///
    /// ```text
    /// DisplayName(id=100, name="my_crate::Handler")
    ///   ├─ alias_of ─→ Symbol(id=200, canonical_fqn="my_crate::src/handler.rs::Function Handler")
    ///   └─ alias_of ─→ Symbol(id=201, canonical_fqn="my_crate::src/parser.rs::Function Handler")
    /// ```
    ///
    /// # Arguments
    ///
    /// * `display_fqn` - Display fully-qualified name (human-readable, potentially ambiguous)
    /// * `symbol_ids` - All Symbol node IDs that share this Display FQN
    ///
    /// # Returns
    ///
    /// `Ok(())` if the ambiguity group was created/updated successfully
    ///
    /// # Errors
    ///
    /// - Database operation failures
    /// - Graph backend errors
    ///
    /// # Determinism
    ///
    /// The operation is idempotent: calling multiple times with same inputs
    /// produces the same graph structure.
    fn create_ambiguous_group(
        &mut self,
        display_fqn: &str,
        symbol_ids: &[i64],
    ) -> Result<()>;

    /// Resolve a Display FQN to a specific Symbol by SymbolId
    ///
    /// This performs precise SymbolId-based resolution, returning the exact Symbol
    /// with the requested SymbolId.
    ///
    /// # Use Case
    ///
    /// When a user provides `--symbol-id <ID>` flag:
    /// ```text
    /// magellan find --symbol-id abc123...xyz my_crate::Handler
    /// ```
    ///
    /// The system should return the exact Symbol matching that ID, not just
    /// the first match by name.
    ///
    /// # Arguments
    ///
    /// * `display_fqn` - Display fully-qualified name being resolved
    /// * `preferred_symbol_id` - User-selected SymbolId (from --symbol-id flag)
    ///
    /// # Returns
    ///
    /// `Ok(Some(SymbolNode))` if the requested SymbolId exists and matches display_fqn
    /// `Ok(None)` if the SymbolId doesn't exist or doesn't match display_fqn
    ///
    /// # Errors
    ///
    /// - Database operation failures
    /// - Graph backend errors
    ///
    /// # Example
    ///
    /// ```rust
    /// let symbol = graph.resolve_by_symbol_id(
    ///     "my_crate::Handler",
    ///     "abc123..."
    /// )?;
    ///
    /// if let Some(symbol) = symbol {
    ///     println!("Found: {} at {}",
    ///         symbol.display_fqn.as_deref().unwrap_or("<none>"),
    ///         symbol.file_path.as_deref().unwrap_or("?")
    ///     );
    /// }
    /// ```
    fn resolve_by_symbol_id(
        &mut self,
        display_fqn: &str,
        preferred_symbol_id: &str,
    ) -> Result<Option<SymbolNode>>;

    /// Enumerate all SymbolIds for a Display FQN
    ///
    /// This operation finds all symbols that share a given Display FQN,
    /// enabling ambiguity detection and candidate enumeration.
    ///
    /// # Use Case
    ///
    /// When a user queries by name without specifying SymbolId:
    /// ```text
    /// magellan find my_crate::Handler
    /// ```
    ///
    /// If multiple symbols match, show all candidates:
    /// ```text
    /// Ambiguous symbol name 'my_crate::Handler': found 2 candidates
    ///   [1] Symbol ID: abc123...
    ///       Canonical: my_crate::src/handler.rs::Function Handler
    ///       File: src/handler.rs
    ///   [2] Symbol ID: def456...
    ///       Canonical: my_crate::src/parser.rs::Function Handler
    ///       File: src/parser.rs
    /// ```
    ///
    /// # Arguments
    ///
    /// * `display_fqn` - Display fully-qualified name to query
    ///
    /// # Returns
    ///
    /// Vector of `(entity_id, SymbolNode)` tuples for all symbols with this display_fqn
    ///
    /// # Errors
    ///
    /// - Database operation failures
    /// - Graph backend errors
    ///
    /// # Returns Empty Vec
    ///
    /// Returns an empty Vec (not an error) when no symbols match
    /// This allows CLI to distinguish "not found" from "ambiguous" cases.
    ///
    /// # Performance Note
    ///
    /// This performs a direct SQL query with json_extract for efficient
    /// filtering. Uses prepared statements for repeated queries.
    ///
    /// # Example
    ///
    /// ```rust
    /// let candidates = graph.get_candidates("my_crate::Handler")?;
    ///
    /// if candidates.is_empty() {
    ///     println!("No symbols found with display name 'my_crate::Handler'");
    /// } else if candidates.len() == 1 {
    ///     println!("Found: {}", candidates[0].1.display_fqn.unwrap_or("<none>"));
    /// } else {
    ///     println!("Ambiguous: found {} candidates", candidates.len());
    ///     for (idx, (_id, symbol)) in candidates.iter().enumerate() {
    ///         println!("  [{}]: {}", idx + 1, symbol.display_fqn.unwrap_or("<none>"));
    ///     }
    /// }
    /// ```
    fn get_candidates(&mut self, display_fqn: &str) -> Result<Vec<(i64, SymbolNode)>>;
}
