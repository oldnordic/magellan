use anyhow::Result;

use crate::graph::{CallNode, CodeGraph};

use super::ExportConfig;

/// Escape a string for use as a DOT label
///
/// DOT labels must be wrapped in double quotes and escape special characters.
pub(super) fn escape_dot_label(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', r#"\""#)
            .replace('\n', "\\n")
    )
}

/// Create a valid DOT identifier from a string
///
/// DOT identifiers should not contain special characters.
/// If symbol_id is available, it's used as a stable identifier.
pub(super) fn escape_dot_id(symbol_id: &Option<String>, name: &str) -> String {
    if let Some(id) = symbol_id {
        id.clone()
    } else {
        name.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}

/// Export call graph to DOT (Graphviz) format
pub fn export_dot(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut dot_output = String::from("strict digraph call_graph {\n");
    dot_output.push_str("  node [shape=box, style=rounded];\n");

    let entity_ids = graph.files.backend.entity_ids()?;
    let snapshot = sqlitegraph::SnapshotId::current();
    let mut calls = Vec::new();

    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(snapshot, entity_id)?;
        if entity.kind == "Call" {
            if let Ok(call_node) = serde_json::from_value::<CallNode>(entity.data) {
                calls.push(call_node);
            }
        }
    }

    if let Some(file_filter) = &config.filters.file {
        calls.retain(|c| c.file.contains(file_filter));
    }
    if let Some(symbol_filter) = &config.filters.symbol {
        calls.retain(|c| c.caller.contains(symbol_filter) || c.callee.contains(symbol_filter));
    }

    calls.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.caller.cmp(&b.caller))
            .then_with(|| a.callee.cmp(&b.callee))
    });

    let mut nodes: BTreeSet<(String, String)> = BTreeSet::new();
    let mut file_to_nodes: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for call in &calls {
        for (name, symbol_id) in [
            (call.caller.as_str(), call.caller_symbol_id.as_ref()),
            (call.callee.as_str(), call.callee_symbol_id.as_ref()),
        ] {
            let node_id = escape_dot_id(&symbol_id.cloned(), name);
            let label = format!(
                "{}\\n{}",
                escape_dot_label(name),
                escape_dot_label(&call.file)
            );
            nodes.insert((node_id.clone(), label.clone()));

            if config.filters.cluster {
                file_to_nodes
                    .entry(call.file.clone())
                    .or_default()
                    .push((node_id, label));
            }
        }
    }

    if config.filters.cluster {
        for (file, file_nodes) in &file_to_nodes {
            let cluster_id = file
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect::<String>();

            dot_output.push_str(&format!("  subgraph cluster_{} {{\n", cluster_id));
            dot_output.push_str(&format!("    label = {};\n", escape_dot_label(file)));
            dot_output.push_str("    style = dashed;\n");

            let mut seen = BTreeSet::new();
            for (node_id, label) in file_nodes {
                if seen.insert(node_id.clone()) {
                    dot_output.push_str(&format!("    {} [label={}];\n", node_id, label));
                }
            }

            dot_output.push_str("  }\n");
        }
    } else {
        for (node_id, label) in &nodes {
            dot_output.push_str(&format!("  {} [label={}];\n", node_id, label));
        }
    }

    for call in &calls {
        let caller_id = escape_dot_id(&call.caller_symbol_id, &call.caller);
        let callee_id = escape_dot_id(&call.callee_symbol_id, &call.callee);
        dot_output.push_str(&format!("  {} -> {};\n", caller_id, callee_id));
    }

    dot_output.push_str("}\n");

    Ok(dot_output)
}
