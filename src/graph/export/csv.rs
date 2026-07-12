use anyhow::Result;
use serde::Serialize;
use sqlitegraph::SnapshotId;

use crate::graph::{CallNode, CodeGraph, ReferenceNode, SymbolNode};

use super::get_file_path_from_symbol;

/// Unified CSV row for all record types
///
/// Single struct with optional fields for different record types ensures
/// consistent CSV headers across Symbol, Reference, and Call records.
///
/// NOTE: We do NOT use `skip_serializing_if` on optional fields because
/// the CSV crate writes headers based on the first record. If we skip fields,
/// subsequent records with different field sets will fail with "found record
/// with X fields, but the previous record has Y fields". Instead, we always
/// write all fields (empty strings for None values) to ensure consistent headers.
#[derive(Debug, Clone, Serialize)]
struct UnifiedCsvRow {
    // Universal fields (always present)
    record_type: String,
    file: String,
    byte_start: usize,
    byte_end: usize,
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,

    // Symbol-specific (optional, but always serialized as empty string if None)
    symbol_id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    kind_normalized: Option<String>,

    // Reference-specific (optional, but always serialized as empty string if None)
    referenced_symbol: Option<String>,
    target_symbol_id: Option<String>,

    // Call-specific (optional, but always serialized as empty string if None)
    caller: Option<String>,
    callee: Option<String>,
    caller_symbol_id: Option<String>,
    callee_symbol_id: Option<String>,
}

/// Export graph data to CSV format
///
/// Produces a combined CSV with a record_type column for discrimination.
/// Uses the csv crate for proper RFC 4180 compliance (quoting, escaping).
///
/// # Returns
/// CSV string with all requested entities, deterministically sorted
pub fn export_csv(graph: &mut CodeGraph, config: &super::ExportConfig) -> Result<String> {
    let mut records: Vec<UnifiedCsvRow> = Vec::new();

    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

        match entity.kind.as_str() {
            "Symbol" if config.include_symbols => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    let file = get_file_path_from_symbol(graph, entity_id)?;
                    records.push(UnifiedCsvRow {
                        record_type: "Symbol".to_string(),
                        file,
                        byte_start: symbol_node.byte_start,
                        byte_end: symbol_node.byte_end,
                        start_line: symbol_node.start_line,
                        start_col: symbol_node.start_col,
                        end_line: symbol_node.end_line,
                        end_col: symbol_node.end_col,
                        symbol_id: symbol_node.symbol_id,
                        name: symbol_node.name,
                        kind: Some(symbol_node.kind),
                        kind_normalized: symbol_node.kind_normalized,
                        referenced_symbol: None,
                        target_symbol_id: None,
                        caller: None,
                        callee: None,
                        caller_symbol_id: None,
                        callee_symbol_id: None,
                    });
                }
            }
            "Reference" if config.include_references => {
                if let Ok(ref_node) = serde_json::from_value::<ReferenceNode>(entity.data.clone()) {
                    let referenced_symbol = entity
                        .name
                        .strip_prefix("ref to ")
                        .unwrap_or("")
                        .to_string();

                    records.push(UnifiedCsvRow {
                        record_type: "Reference".to_string(),
                        file: ref_node.file,
                        byte_start: ref_node.byte_start as usize,
                        byte_end: ref_node.byte_end as usize,
                        start_line: ref_node.start_line as usize,
                        start_col: ref_node.start_col as usize,
                        end_line: ref_node.end_line as usize,
                        end_col: ref_node.end_col as usize,
                        symbol_id: None,
                        name: None,
                        kind: None,
                        kind_normalized: None,
                        referenced_symbol: Some(referenced_symbol),
                        target_symbol_id: None,
                        caller: None,
                        callee: None,
                        caller_symbol_id: None,
                        callee_symbol_id: None,
                    });
                }
            }
            "Call" if config.include_calls => {
                if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                    records.push(UnifiedCsvRow {
                        record_type: "Call".to_string(),
                        file: call_node.file,
                        byte_start: call_node.byte_start as usize,
                        byte_end: call_node.byte_end as usize,
                        start_line: call_node.start_line as usize,
                        start_col: call_node.start_col as usize,
                        end_line: call_node.end_line as usize,
                        end_col: call_node.end_col as usize,
                        symbol_id: None,
                        name: None,
                        kind: None,
                        kind_normalized: None,
                        referenced_symbol: None,
                        target_symbol_id: None,
                        caller: Some(call_node.caller),
                        callee: Some(call_node.callee),
                        caller_symbol_id: call_node.caller_symbol_id,
                        callee_symbol_id: call_node.callee_symbol_id,
                    });
                }
            }
            _ => {
                // Ignore File and unknown node types for CSV export
            }
        }
    }

    // Sort deterministically by record_type, then by type-specific fields
    records.sort_by(|a, b| {
        let type_order = match (a.record_type.as_str(), b.record_type.as_str()) {
            ("Call", "Call") => std::cmp::Ordering::Equal,
            ("Call", "Reference") => std::cmp::Ordering::Greater,
            ("Call", "Symbol") => std::cmp::Ordering::Greater,
            ("Reference", "Call") => std::cmp::Ordering::Less,
            ("Reference", "Reference") => std::cmp::Ordering::Equal,
            ("Reference", "Symbol") => std::cmp::Ordering::Greater,
            ("Symbol", "Call") => std::cmp::Ordering::Less,
            ("Symbol", "Reference") => std::cmp::Ordering::Less,
            ("Symbol", "Symbol") => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Equal,
        };

        if type_order != std::cmp::Ordering::Equal {
            return type_order;
        }

        match a.record_type.as_str() {
            "Symbol" => (&a.file, a.name.as_ref().unwrap_or(&String::new()))
                .cmp(&(&b.file, b.name.as_ref().unwrap_or(&String::new()))),
            "Reference" => (
                &a.record_type,
                &a.file,
                a.referenced_symbol.as_ref().unwrap_or(&String::new()),
            )
                .cmp(&(
                    &b.record_type,
                    &b.file,
                    b.referenced_symbol.as_ref().unwrap_or(&String::new()),
                )),
            "Call" => (
                &a.record_type,
                &a.file,
                a.caller.as_ref().unwrap_or(&String::new()),
                a.callee.as_ref().unwrap_or(&String::new()),
            )
                .cmp(&(
                    &b.record_type,
                    &b.file,
                    b.caller.as_ref().unwrap_or(&String::new()),
                    b.callee.as_ref().unwrap_or(&String::new()),
                )),
            _ => std::cmp::Ordering::Equal,
        }
    });

    let mut buffer = Vec::new();

    use std::io::Write;
    writeln!(buffer, "# Magellan Export Version: 2.0.0")?;

    {
        let mut writer = csv::Writer::from_writer(&mut buffer);
        for record in records {
            writer.serialize(record)?;
        }
        writer.flush()?;
    }

    String::from_utf8(buffer).map_err(|e| anyhow::anyhow!("CSV output is not valid UTF-8: {}", e))
}
