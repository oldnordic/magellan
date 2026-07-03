//! Search command implementation
//!
//! Full-text search over code chunk content via FTS5.
//! Returns symbol-level hits (function name, file, line range, excerpt).

use anyhow::Result;
use magellan::output::OutputFormat;
use magellan::CodeGraph;
use serde_json::json;
use std::path::PathBuf;

pub fn run_search(
    db_path: PathBuf,
    pattern: String,
    limit: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let results = graph.search_code_content(&pattern, limit)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = json!({
                "query": pattern,
                "count": results.len(),
                "results": results,
            });
            if output_format == OutputFormat::Pretty {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("{}", response);
            }
        }
        OutputFormat::Human => {
            if results.is_empty() {
                println!("No content matches for: {pattern}");
            } else {
                println!("_{} match(s)_ `{}`\n", results.len(), pattern);
                for r in &results {
                    let name = r.symbol_name.as_deref().unwrap_or("(no symbol)");
                    let kind = r.symbol_kind.as_deref().unwrap_or("?");
                    println!(
                        "- **{name}** ({kind}) `{file_path}`:{start_line}-{end_line}",
                        file_path = r.file_path,
                        start_line = r.start_line,
                        end_line = r.end_line
                    );
                    if !r.excerpt.is_empty() {
                        let preview: String = r.excerpt.chars().take(200).collect();
                        println!("  {preview}");
                    }
                    println!();
                }
            }
        }
    }

    Ok(())
}
