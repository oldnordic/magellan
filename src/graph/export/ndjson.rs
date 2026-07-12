use anyhow::Result;
use serde::Serialize;
use sqlitegraph::SnapshotId;

use crate::graph::{CallNode, CodeGraph, FileNode, ReferenceNode, SymbolNode};

use super::{
    get_file_path_from_symbol, CallExport, ExportConfig, FileExport, ReferenceExport, SymbolExport,
};

#[derive(Debug, Clone, Serialize)]
enum JsonlRecord {
    Version { version: String },
    File(FileExport),
    Symbol(SymbolExport),
    Reference(ReferenceExport),
    Call(CallExport),
}

/// Export graph data to JSONL format
pub fn export_jsonl(graph: &mut CodeGraph) -> Result<String> {
    let config = ExportConfig::new(super::ExportFormat::JsonL);
    let mut buffer = Vec::new();
    stream_ndjson(graph, &config, &mut buffer)?;
    String::from_utf8(buffer).map_err(|e| anyhow::anyhow!("JSONL output is not valid UTF-8: {}", e))
}

/// Stream all graph data to JSONL format with reduced memory footprint
pub fn stream_ndjson<W: std::io::Write>(
    graph: &mut CodeGraph,
    config: &ExportConfig,
    writer: &mut W,
) -> Result<()> {
    let mut records = Vec::new();

    records.push(JsonlRecord::Version {
        version: "2.0.0".to_string(),
    });

    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;

        match entity.kind.as_str() {
            "File" => {
                if let Ok(file_node) = serde_json::from_value::<FileNode>(entity.data.clone()) {
                    records.push(JsonlRecord::File(FileExport {
                        path: file_node.path,
                        hash: file_node.hash,
                    }));
                }
            }
            "Symbol" if config.include_symbols => {
                if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(entity.data.clone()) {
                    let file = get_file_path_from_symbol(graph, entity_id)?;
                    records.push(JsonlRecord::Symbol(SymbolExport {
                        symbol_id: symbol_node.symbol_id,
                        canonical_fqn: symbol_node.canonical_fqn,
                        display_fqn: symbol_node.display_fqn,
                        name: symbol_node.name,
                        kind: symbol_node.kind,
                        kind_normalized: symbol_node.kind_normalized,
                        file,
                        byte_start: symbol_node.byte_start,
                        byte_end: symbol_node.byte_end,
                        start_line: symbol_node.start_line,
                        start_col: symbol_node.start_col,
                        end_line: symbol_node.end_line,
                        end_col: symbol_node.end_col,
                    }));
                }
            }
            "Reference" if config.include_references => {
                if let Ok(ref_node) = serde_json::from_value::<ReferenceNode>(entity.data.clone()) {
                    let referenced_symbol = entity
                        .name
                        .strip_prefix("ref to ")
                        .unwrap_or("")
                        .to_string();

                    records.push(JsonlRecord::Reference(ReferenceExport {
                        file: ref_node.file,
                        referenced_symbol,
                        target_symbol_id: None,
                        byte_start: ref_node.byte_start as usize,
                        byte_end: ref_node.byte_end as usize,
                        start_line: ref_node.start_line as usize,
                        start_col: ref_node.start_col as usize,
                        end_line: ref_node.end_line as usize,
                        end_col: ref_node.end_col as usize,
                    }));
                }
            }
            "Call" if config.include_calls => {
                if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data.clone()) {
                    records.push(JsonlRecord::Call(CallExport {
                        file: call_node.file,
                        caller: call_node.caller,
                        callee: call_node.callee,
                        caller_symbol_id: call_node.caller_symbol_id,
                        callee_symbol_id: call_node.callee_symbol_id,
                        byte_start: call_node.byte_start as usize,
                        byte_end: call_node.byte_end as usize,
                        start_line: call_node.start_line as usize,
                        start_col: call_node.start_col as usize,
                        end_line: call_node.end_line as usize,
                        end_col: call_node.end_col as usize,
                    }));
                }
            }
            _ => {}
        }
    }

    records.sort_by(|a, b| match (a, b) {
        (JsonlRecord::Version { .. }, _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Version { .. }) => std::cmp::Ordering::Greater,
        (JsonlRecord::File(a), JsonlRecord::File(b)) => a.path.cmp(&b.path),
        (JsonlRecord::Symbol(a), JsonlRecord::Symbol(b)) => {
            (&a.file, &a.name).cmp(&(&b.file, &b.name))
        }
        (JsonlRecord::Reference(a), JsonlRecord::Reference(b)) => {
            (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol))
        }
        (JsonlRecord::Call(a), JsonlRecord::Call(b)) => {
            (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee))
        }
        (JsonlRecord::File(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::File(_)) => std::cmp::Ordering::Greater,
        (JsonlRecord::Symbol(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Symbol(_)) => std::cmp::Ordering::Greater,
        (JsonlRecord::Reference(_), _) => std::cmp::Ordering::Less,
        (_, JsonlRecord::Reference(_)) => std::cmp::Ordering::Greater,
    });

    let mut first = true;
    for record in records {
        if !first {
            writeln!(&mut *writer)?;
        }
        serde_json::to_writer(&mut *writer, &record)
            .map_err(|e| anyhow::anyhow!("JSON serialization error: {}", e))?;
        first = false;
    }

    Ok(())
}
