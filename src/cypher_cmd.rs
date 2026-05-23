//! Cypher command implementation
//!
//! Execute Cypher-inspired queries against a sqlitegraph database.

use anyhow::{Context, Result};
use magellan::OutputFormat;
use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
use std::path::PathBuf;

/// Run a Cypher query against the database
pub fn run_cypher(db_path: PathBuf, query_str: String, output_format: OutputFormat) -> Result<()> {
    use sqlitegraph::cypher::{execute, parse};

    // Open the graph and wrap in a backend
    let sqlite_graph = SqliteGraph::open(&db_path)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;
    let backend = SqliteGraphBackend::from_graph(sqlite_graph);

    // Parse the query
    let query = parse(&query_str).map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

    // Execute
    let result =
        execute(&backend, &query).map_err(|e| anyhow::anyhow!("Execution error: {}", e))?;

    // Output
    match output_format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&result)?);
        }
        OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Human => {
            // Human: if the result is an array, print rows line by line
            match result {
                serde_json::Value::Array(rows) => {
                    if rows.is_empty() {
                        println!("0 rows");
                    } else {
                        for (i, row) in rows.iter().enumerate() {
                            println!("row {}: {}", i, serde_json::to_string_pretty(row)?);
                        }
                        println!("{}", rows.len());
                    }
                }
                other => {
                    println!("{}", serde_json::to_string_pretty(&other)?);
                }
            }
        }
    }

    Ok(())
}
