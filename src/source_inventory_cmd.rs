//! Source inventory command implementation
//!
//! Scans wiki pages and message files, extracts deterministic metadata
//! (frontmatter, wikilinks, title, tags), and stores in the database.

use anyhow::{Context, Result};
use magellan::graph::source_inventory::{self, ScanResult};
use magellan::output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
use rusqlite::Connection;
use std::path::PathBuf;

/// Run the source inventory command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `scan_dirs` - Optional list of (directory, kind) tuples to scan
/// * `list_kind` - Optional kind filter for listing
/// * `show_stale` - Whether to show stale documents
/// * `output_format` - Output format (Human or Json)
///
/// # Returns
/// Result indicating success or failure
pub fn run_source_inventory(
    db_path: PathBuf,
    scan_dirs: Vec<(PathBuf, String)>,
    list_kind: Option<String>,
    show_stale: bool,
    output_format: OutputFormat,
) -> Result<()> {
    let exec_id = generate_execution_id();

    // Open a direct SQLite connection for source inventory operations
    let conn = Connection::open(&db_path)
        .with_context(|| format!("open database: {}", db_path.display()))?;

    source_inventory::ensure_schema(&conn).context("ensure source inventory schema")?;

    // Scan directories if requested
    let mut scan_results: Vec<(String, ScanResult)> = Vec::new();
    for (dir, kind) in scan_dirs {
        let result = source_inventory::scan_directory(&conn, &dir, &kind, "md")
            .with_context(|| format!("scan directory: {}", dir.display()))?;
        scan_results.push((format!("{} ({})", dir.display(), kind), result));
    }

    // List documents if requested
    let docs = if list_kind.is_some() || (!show_stale && scan_results.is_empty()) {
        Some(
            source_inventory::list_by_kind(&conn, list_kind.as_deref())
                .context("list documents")?,
        )
    } else {
        None
    };

    // Find stale documents if requested
    let stale = if show_stale {
        Some(source_inventory::find_stale(&conn).context("find stale documents")?)
    } else {
        None
    };

    // Output
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = SourceInventoryResponse {
                scan_results,
                documents: docs,
                stale_documents: stale.map(|s| {
                    s.into_iter()
                        .map(|(doc, current_hash)| StaleDocument {
                            path: doc.path_or_uri,
                            old_hash: doc.content_hash,
                            current_hash,
                        })
                        .collect()
                }),
            };
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            for (label, result) in &scan_results {
                println!(
                    "Scanned {}: {} files, {} inserted, {} updated, {} unchanged",
                    label, result.scanned, result.inserted, result.updated, result.unchanged,
                );
                if !result.errors.is_empty() {
                    for err in &result.errors {
                        eprintln!("  Error: {}", err);
                    }
                }
            }

            if let Some(docs) = docs {
                if docs.is_empty() {
                    println!("No source documents found.");
                } else {
                    println!("\nSource documents ({}):", docs.len());
                    for doc in docs {
                        let title = doc.title.as_deref().unwrap_or("(no title)");
                        let kind = doc.source_kind;
                        println!("  [{}] {} - {}", kind, doc.path_or_uri, title);
                        if !doc.tags.is_empty() {
                            println!("    tags: {}", doc.tags.join(", "));
                        }
                        if !doc.wikilinks.is_empty() {
                            println!("    wikilinks: {}", doc.wikilinks.join(", "));
                        }
                    }
                }
            }

            if let Some(stale) = stale {
                if stale.is_empty() {
                    println!("\nNo stale documents.");
                } else {
                    println!("\nStale documents ({}):", stale.len());
                    for (doc, current_hash) in stale {
                        println!(
                            "  {} (old: {}, current: {})",
                            doc.path_or_uri,
                            &doc.content_hash[..8.min(doc.content_hash.len())],
                            &current_hash[..8.min(current_hash.len())],
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct SourceInventoryResponse {
    scan_results: Vec<(String, ScanResult)>,
    documents: Option<Vec<source_inventory::SourceDocument>>,
    stale_documents: Option<Vec<StaleDocument>>,
}

#[derive(Debug, serde::Serialize)]
struct StaleDocument {
    path: String,
    old_hash: String,
    current_hash: String,
}
