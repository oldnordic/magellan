//! Repair pass for CALLER/CALLS edges mis-wired at ingest (BUG-2).
//!
//! At ingest time, edge construction fell back to "first same-named symbol
//! DB-wide" whenever the FQN-keyed lookup missed (always, for methods). The
//! correct, file-scoped stable symbol IDs were already persisted on each Call
//! node (`caller_symbol_id` / `callee_symbol_id`) but never consulted for edges.
//!
//! This module recomputes CALLER/CALLS edges from that persisted data — no
//! re-parse required — and can either report (dry-run) or transactionally
//! rewrite the mis-wired edges (apply).

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Outcome of a repair pass over a database's CALLER/CALLS edges.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EdgeRepairReport {
    /// Total Call nodes inspected.
    pub call_nodes_total: usize,
    /// Call nodes carrying at least one persisted stable symbol ID.
    pub call_nodes_with_stable_ids: usize,
    /// Call nodes whose caller stable ID could be evaluated.
    pub caller_edges_checked: usize,
    /// Call nodes whose callee stable ID could be evaluated.
    pub calls_edges_checked: usize,
    /// CALLER edges pointing at the wrong symbol (rewired on apply).
    pub caller_edges_miswired: usize,
    /// CALLS edges pointing at the wrong symbol (rewired on apply).
    pub calls_edges_miswired: usize,
    /// CALLER edges absent entirely (created on apply).
    pub caller_edges_missing: usize,
    /// CALLS edges absent entirely (created on apply).
    pub calls_edges_missing: usize,
    /// Persisted stable IDs with no matching Symbol entity in the DB.
    pub unresolved_stable_ids: usize,
    /// Edge rows deleted (apply mode only).
    pub edges_deleted: usize,
    /// Edge rows inserted (apply mode only).
    pub edges_inserted: usize,
    /// Whether the rewrite was applied (false = dry-run).
    pub applied: bool,
}

/// A pending edge rewrite for one call node.
struct EdgeAction {
    /// IDs of existing edge rows to delete (empty when the edge is missing).
    delete_edge_ids: Vec<i64>,
    /// (from_id, to_id, edge_type) to insert.
    insert: (i64, i64, &'static str),
}

/// Recompute CALLER/CALLS edges from Call nodes' persisted stable symbol IDs.
///
/// When `apply` is false this is a pure read-only dry-run: the report describes
/// what would change and the database is not modified. When `apply` is true,
/// all rewrites happen inside a single transaction.
pub fn repair_call_edges(db_path: &Path, apply: bool) -> Result<EdgeRepairReport> {
    let mut conn = Connection::open(db_path)?;
    repair_call_edges_conn(&mut conn, apply)
}

/// Connection-taking variant of [`repair_call_edges`] for in-process use/tests.
pub fn repair_call_edges_conn(conn: &mut Connection, apply: bool) -> Result<EdgeRepairReport> {
    // 1. stable symbol_id -> Symbol entity id (deterministic: lowest id wins).
    let mut stable_to_entity: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, json_extract(data, '$.symbol_id') FROM graph_entities
             WHERE kind = 'Symbol' AND json_extract(data, '$.symbol_id') IS NOT NULL
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (entity_id, stable_id) = row?;
            stable_to_entity.entry(stable_id).or_insert(entity_id);
        }
    }

    // 2. Call nodes with their persisted stable IDs.
    let mut calls: Vec<(i64, Option<String>, Option<String>)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id,
                    json_extract(data, '$.caller_symbol_id'),
                    json_extract(data, '$.callee_symbol_id')
             FROM graph_entities WHERE kind = 'Call' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            calls.push(row?);
        }
    }

    // 3. Existing CALLER/CALLS edge rows, grouped by their call node.
    //    CALLER: from = caller symbol, to = call node.
    //    CALLS:  from = call node,   to = callee symbol.
    let mut caller_edges_by_call: HashMap<i64, Vec<(i64, i64)>> = HashMap::new(); // call -> [(edge_id, symbol_id)]
    let mut calls_edges_by_call: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, from_id, to_id, edge_type FROM graph_edges
             WHERE edge_type IN ('CALLER', 'CALLS') ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (edge_id, from_id, to_id, edge_type) = row?;
            match edge_type.as_str() {
                "CALLER" => caller_edges_by_call
                    .entry(to_id)
                    .or_default()
                    .push((edge_id, from_id)),
                "CALLS" => calls_edges_by_call
                    .entry(from_id)
                    .or_default()
                    .push((edge_id, to_id)),
                _ => {}
            }
        }
    }

    // 4. Diff persisted stable IDs against current edges.
    let mut report = EdgeRepairReport {
        applied: apply,
        ..Default::default()
    };
    let mut actions: Vec<EdgeAction> = Vec::new();

    for (call_id, caller_stable, callee_stable) in &calls {
        report.call_nodes_total += 1;
        if caller_stable.is_some() || callee_stable.is_some() {
            report.call_nodes_with_stable_ids += 1;
        }

        if let Some(stable_id) = caller_stable {
            report.caller_edges_checked += 1;
            match stable_to_entity.get(stable_id) {
                Some(&expected) => {
                    let current = caller_edges_by_call.get(call_id);
                    let current_symbols: Vec<i64> = current
                        .map(|v| v.iter().map(|(_, sym)| *sym).collect())
                        .unwrap_or_default();
                    if current_symbols != [expected] {
                        if current_symbols.is_empty() {
                            report.caller_edges_missing += 1;
                        } else {
                            report.caller_edges_miswired += 1;
                        }
                        actions.push(EdgeAction {
                            delete_edge_ids: current
                                .map(|v| v.iter().map(|(id, _)| *id).collect())
                                .unwrap_or_default(),
                            insert: (expected, *call_id, "CALLER"),
                        });
                    }
                }
                None => report.unresolved_stable_ids += 1,
            }
        }

        if let Some(stable_id) = callee_stable {
            report.calls_edges_checked += 1;
            match stable_to_entity.get(stable_id) {
                Some(&expected) => {
                    let current = calls_edges_by_call.get(call_id);
                    let current_symbols: Vec<i64> = current
                        .map(|v| v.iter().map(|(_, sym)| *sym).collect())
                        .unwrap_or_default();
                    if current_symbols != [expected] {
                        if current_symbols.is_empty() {
                            report.calls_edges_missing += 1;
                        } else {
                            report.calls_edges_miswired += 1;
                        }
                        actions.push(EdgeAction {
                            delete_edge_ids: current
                                .map(|v| v.iter().map(|(id, _)| *id).collect())
                                .unwrap_or_default(),
                            insert: (*call_id, expected, "CALLS"),
                        });
                    }
                }
                None => report.unresolved_stable_ids += 1,
            }
        }
    }

    // 5. Apply rewrites transactionally (dry-run: report only).
    if apply && !actions.is_empty() {
        let tx = conn.transaction()?;
        for action in &actions {
            for edge_id in &action.delete_edge_ids {
                report.edges_deleted +=
                    tx.execute("DELETE FROM graph_edges WHERE id = ?1", [edge_id])?;
            }
            let (from_id, to_id, edge_type) = action.insert;
            report.edges_inserted += tx.execute(
                "INSERT INTO graph_edges (from_id, to_id, edge_type, data)
                 VALUES (?1, ?2, ?3, '{}')",
                rusqlite::params![from_id, to_id, edge_type],
            )?;
        }
        tx.commit()?;
    }

    Ok(report)
}
