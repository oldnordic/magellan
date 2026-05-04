//! Registry command implementation
//!
//! Discovers and manages multiple Magellan databases across projects.

use anyhow::Result;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Discovered database entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDb {
    pub path: String,
    pub project_name: String,
    pub file_count: Option<i64>,
    pub symbol_count: Option<i64>,
}

/// Scan for Magellan databases in a directory tree
///
/// Searches for .magellan/*.db files and returns metadata about each discovered database.
pub fn run_registry_scan(root: PathBuf, output_format: OutputFormat) -> Result<()> {
    let discovered = scan_for_databases(&root)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = JsonResponse::new(discovered, "registry-scan");
            output_json(&response, output_format)?;
        }
        OutputFormat::Human => {
            if discovered.is_empty() {
                println!("No Magellan databases found under {}", root.display());
            } else {
                println!(
                    "Found {} database(s) under {}:\n",
                    discovered.len(),
                    root.display()
                );
                for db in &discovered {
                    println!("  {}: {}", db.project_name, db.path);
                    if let (Some(files), Some(symbols)) = (db.file_count, db.symbol_count) {
                        println!("    {} files, {} symbols", files, symbols);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Scan a directory tree for .magellan/*.db files
fn scan_for_databases(root: &Path) -> Result<Vec<DiscoveredDb>> {
    let mut databases = Vec::new();

    // Walk the directory tree looking for .magellan directories
    if !root.exists() {
        anyhow::bail!("Directory does not exist: {}", root.display());
    }

    collect_magellan_dbs(root, &mut databases)?;

    // Sort by project name for consistent output
    databases.sort_by(|a, b| a.project_name.cmp(&b.project_name));

    Ok(databases)
}

/// Recursively collect .magellan/*.db files
fn collect_magellan_dbs(dir: &Path, databases: &mut Vec<DiscoveredDb>) -> Result<()> {
    let magellan_dir = dir.join(".magellan");

    if magellan_dir.is_dir() {
        // Check for .db files in .magellan
        if let Ok(entries) = std::fs::read_dir(&magellan_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "db") {
                    if let Some(db_info) = discover_database(&path)? {
                        databases.push(db_info);
                    }
                }
            }
        }
    }

    // Recurse into subdirectories (skip .git and obvious ignored dirs)
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name != ".git" && !name.starts_with('.') {
                    collect_magellan_dbs(&path, databases)?;
                }
            }
        }
    }

    Ok(())
}

/// Discover metadata about a single database
fn discover_database(db_path: &Path) -> Result<Option<DiscoveredDb>> {
    let project_name = db_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Try to open and query the database for basic stats
    let mut file_count = None;
    let mut symbol_count = None;

    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        // Query file count
        if let Ok(count) = conn.query_row(
            "SELECT COUNT(*) FROM graph_entities WHERE kind = 'File'",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            file_count = Some(count);
        }

        // Query symbol count
        if let Ok(count) = conn.query_row(
            "SELECT COUNT(*) FROM graph_entities WHERE kind = 'Symbol'",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            symbol_count = Some(count);
        }
    }

    Ok(Some(DiscoveredDb {
        path: db_path.to_string_lossy().to_string(),
        project_name,
        file_count,
        symbol_count,
    }))
}

/// List known databases with detailed info
pub fn run_registry_list(root: PathBuf, output_format: OutputFormat) -> Result<()> {
    let discovered = scan_for_databases(&root)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = JsonResponse::new(discovered, "registry-list");
            output_json(&response, output_format)?;
        }
        OutputFormat::Human => {
            if discovered.is_empty() {
                println!("No databases found. Run 'magellan registry scan --root <dir>' first.");
            } else {
                println!(
                    "{:<30} {:<50} {:>10} {:>10}",
                    "Project", "Path", "Files", "Symbols"
                );
                println!("{}", "-".repeat(100));
                for db in &discovered {
                    let files = db.file_count.unwrap_or(-1);
                    let symbols = db.symbol_count.unwrap_or(-1);
                    println!(
                        "{:<30} {:<50} {:>10} {:>10}",
                        db.project_name, db.path, files, symbols
                    );
                }
            }
        }
    }

    Ok(())
}
