//! Label command implementation for Magellan
//!
//! Provides label querying functionality (--list, --count, --show-code).

use anyhow::Result;
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run label query command
/// Usage: magellan label --db <FILE> --label <LABEL> [--list] [--count] [--show-code]
///
/// # Feature Availability
/// Label queries require SQLite backend (not available with native-v2)
#[cfg(not(feature = "native-v2"))]
pub fn run_label(
    db_path: PathBuf,
    labels: Vec<String>,
    list: bool,
    count: bool,
    show_code: bool,
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

    let tracker = crate::ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;

    // List all labels mode
    if list {
        let all_labels = graph.get_all_labels()?;
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

/// Run label query command (native-v2 variant - not supported)
///
/// # Feature Availability
/// Label queries are not supported with native-v2 backend
#[cfg(feature = "native-v2")]
pub fn run_label(
    _db_path: PathBuf,
    _labels: Vec<String>,
    _list: bool,
    _count: bool,
    _show_code: bool,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Label queries are not supported with the native-v2 backend. \
         Label queries depend on SQLite's graph_labels table which doesn't exist in Native V2."
    ))
}
