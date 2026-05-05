//! Label command implementation for Magellan
//!
//! Provides label querying functionality (--list, --count, --show-code).

use anyhow::Result;
use magellan::output::{output_json, JsonResponse};
use magellan::{CodeGraph, OutputFormat};
use serde::Serialize;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

#[derive(Debug, Clone, Serialize)]
struct LabelInfo {
    name: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SymbolByLabel {
    name: String,
    kind: String,
    file_path: String,
    byte_start: usize,
    byte_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

/// Run label query command
/// Usage: magellan label --db <FILE> --label <LABEL> [--list] [--count] [--show-code]
pub fn run_label(
    db_path: PathBuf,
    labels: Vec<String>,
    list: bool,
    count: bool,
    show_code: bool,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let mut args = vec!["label".to_string()];
    for label in &labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }
    if list {
        args.push("--list".to_string());
    }
    if count {
        args.push("--count".to_string());
    }
    if show_code {
        args.push("--show-code".to_string());
    }

    let tracker = ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;

    // List all labels mode
    if list {
        let all_labels = graph.get_all_labels()?;

        if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
            let labels_info: Vec<LabelInfo> = all_labels
                .iter()
                .map(|label| {
                    let count = graph.count_entities_by_label(label).unwrap_or(0);
                    LabelInfo {
                        name: label.clone(),
                        count,
                    }
                })
                .collect();
            let json_response = JsonResponse::new(labels_info, "");
            output_json(&json_response, output_format)?;
            tracker.finish(&graph)?;
            return Ok(());
        }

        println!("{} labels in use:", all_labels.len());
        for label in all_labels {
            let count = graph.count_entities_by_label(&label)?;
            println!("  {} ({})", label, count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Count mode
    if count {
        if labels.is_empty() {
            tracker.finish(&graph)?;
            return Err(anyhow::anyhow!("--count requires --label"));
        }

        if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
            let labels_info: Vec<LabelInfo> = labels
                .iter()
                .map(|label| {
                    let count = graph.count_entities_by_label(label).unwrap_or(0);
                    LabelInfo {
                        name: label.clone(),
                        count,
                    }
                })
                .collect();
            let json_response = JsonResponse::new(labels_info, "");
            output_json(&json_response, output_format)?;
            tracker.finish(&graph)?;
            return Ok(());
        }

        for label in &labels {
            let entity_count = graph.count_entities_by_label(label)?;
            println!("{}: {} entities", label, entity_count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Query mode - get symbols by label(s)
    if labels.is_empty() {
        tracker.finish(&graph)?;
        return Err(anyhow::anyhow!(
            "No labels specified. Use --label <LABEL> or --list to see all labels"
        ));
    }

    let labels_ref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let results = if labels.len() == 1 {
        graph.get_symbols_by_label(&labels[0])?
    } else {
        graph.get_symbols_by_labels(&labels_ref)?
    };

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let symbols: Vec<SymbolByLabel> = results
            .iter()
            .map(|result| {
                let code = if show_code {
                    graph
                        .get_code_chunk_by_span(
                            &result.file_path,
                            result.byte_start,
                            result.byte_end,
                        )
                        .ok()
                        .flatten()
                        .map(|c| c.content)
                } else {
                    None
                };
                SymbolByLabel {
                    name: result.name.clone(),
                    kind: result.kind.clone(),
                    file_path: result.file_path.clone(),
                    byte_start: result.byte_start,
                    byte_end: result.byte_end,
                    code,
                }
            })
            .collect();
        let json_response = JsonResponse::new(symbols, "");
        output_json(&json_response, output_format)?;
        tracker.finish(&graph)?;
        return Ok(());
    }

    if results.is_empty() {
        if labels.len() == 1 {
            println!("No symbols found with label '{}'", labels[0]);
        } else {
            println!("No symbols found with labels: {}", labels.join(", "));
        }
    } else {
        if labels.len() == 1 {
            println!("{} symbols with label '{}':", results.len(), labels[0]);
        } else {
            println!(
                "{} symbols with labels [{}]:",
                results.len(),
                labels.join(", ")
            );
        }

        for result in results {
            println!();
            println!(
                "  {} ({}) in {} [{}-{}]",
                result.name, result.kind, result.file_path, result.byte_start, result.byte_end
            );

            // Show code chunk if requested
            if show_code {
                // Get code chunk by exact byte span instead of by name
                // This avoids getting chunks for other symbols with the same name
                if let Ok(Some(chunk)) = graph.get_code_chunk_by_span(
                    &result.file_path,
                    result.byte_start,
                    result.byte_end,
                ) {
                    for line in chunk.content.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}
