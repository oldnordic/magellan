//! Catalog command — cheap enumeration of all indexed databases.
//!
//! Reads the canonical project registry from `~/.magellan/meta.db` (maintained
//! by the magellan daemon) and introspects each database's actual schema:
//! entity kinds, edge kinds, tables, and counts. This gives an LLM (or human)
//! a token-cheap menu of "what exists and what's queryable" before committing
//! to expensive graph queries.
//!
//! Usage:
//!   magellan catalog                    — list all databases (compact table)
//!   magellan catalog --json             — structured JSON for LLM consumption
//!   magellan catalog describe <name>    — deep-dive one database
//!   magellan catalog describe <name> --json

use anyhow::{Context, Result};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Count of entities or edges of a particular kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindCount {
    pub kind: String,
    pub count: i64,
}

/// A database discovered in the registry, with introspected schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub name: String,
    pub db_path: String,
    pub exists: bool,
    pub entity_count: i64,
    pub edge_count: i64,
    pub entity_kinds: Vec<KindCount>,
    pub edge_kinds: Vec<KindCount>,
    pub tables: Vec<String>,
    /// Query capabilities derived from which tables are present.
    pub capabilities: Vec<String>,
}

impl CatalogEntry {
    /// A compact comma-joined list of entity-kind names (for table display).
    fn entity_kind_summary(&self) -> String {
        if self.entity_kinds.is_empty() {
            return "—".to_string();
        }
        self.entity_kinds
            .iter()
            .map(|k| k.kind.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The canonical meta.db location: `$HOME/.magellan/meta.db`.
fn meta_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".magellan").join("meta.db")
}

/// Read the project registry from `~/.magellan/meta.db`.
/// Returns `(name, db_path)` pairs, sorted by name.
fn read_registry() -> Result<Vec<(String, String)>> {
    let path = meta_db_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open meta.db at {}", path.display()))?;

    // The project_registry table is created by the daemon; if it's absent the
    // daemon has never run, so there's nothing to catalog.
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='project_registry'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !table_exists {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare("SELECT name, db_path FROM project_registry ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Introspect a single magellan database: entity kinds, edge kinds, tables, counts.
fn introspect_db(name: &str, db_path: &str) -> CatalogEntry {
    let path = Path::new(db_path);
    if !path.exists() {
        return CatalogEntry {
            name: name.to_string(),
            db_path: db_path.to_string(),
            exists: false,
            entity_count: 0,
            edge_count: 0,
            entity_kinds: Vec::new(),
            edge_kinds: Vec::new(),
            tables: Vec::new(),
            capabilities: Vec::new(),
        };
    }

    let conn = match Connection::open(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: cannot open {} for {}: {}", db_path, name, e);
            return CatalogEntry {
                name: name.to_string(),
                db_path: db_path.to_string(),
                exists: true,
                entity_count: 0,
                edge_count: 0,
                entity_kinds: Vec::new(),
                edge_kinds: Vec::new(),
                tables: Vec::new(),
                capabilities: Vec::new(),
            };
        }
    };

    let tables = list_tables(&conn);
    let entity_kinds = count_kinds(&conn, "graph_entities", "kind");
    let edge_kinds = count_kinds(&conn, "graph_edges", "edge_type");
    let entity_count: i64 = entity_kinds.iter().map(|k| k.count).sum();
    let edge_count: i64 = edge_kinds.iter().map(|k| k.count).sum();
    let capabilities = derive_capabilities(&tables);

    CatalogEntry {
        name: name.to_string(),
        db_path: db_path.to_string(),
        exists: true,
        entity_count,
        edge_count,
        entity_kinds,
        edge_kinds,
        tables,
        capabilities,
    }
}

/// List all user table names in a database.
fn list_tables(conn: &Connection) -> Vec<String> {
    let mut stmt =
        match conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
    let rows = stmt.query_map([], |row| row.get::<_, String>(0));
    let mut out = Vec::new();
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            out.push(row);
        }
    }
    out
}

/// Count rows grouped by a kind column. Returns empty vec if the table or
/// column doesn't exist.
fn count_kinds(conn: &Connection, table: &str, kind_col: &str) -> Vec<KindCount> {
    let sql = format!(
        "SELECT \"{col}\" AS kind, COUNT(*) AS cnt FROM \"{tbl}\" GROUP BY \"{col}\" ORDER BY cnt DESC",
        col = kind_col,
        tbl = table
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| {
        Ok(KindCount {
            kind: row.get::<_, String>(0)?,
            count: row.get::<_, i64>(1)?,
        })
    });
    let mut out = Vec::new();
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            out.push(row);
        }
    }
    out
}

/// Derive which magellan query commands are usable from the set of tables
/// present in the database. This is the "what's queryable" menu for an LLM.
fn derive_capabilities(tables: &[String]) -> Vec<String> {
    let has = |name: &str| tables.iter().any(|t| t == name);
    let mut caps = Vec::new();

    if has("graph_entities") && has("graph_edges") {
        caps.push("find".to_string());
        caps.push("refs".to_string());
        caps.push("calls".to_string());
        caps.push("navigate".to_string());
        caps.push("context".to_string());
    }
    if has("cfg_blocks") && has("cfg_edges") {
        caps.push("cfg".to_string());
        caps.push("paths".to_string());
        caps.push("dead-code".to_string());
        caps.push("cycles".to_string());
        caps.push("reachable".to_string());
    }
    if has("code_chunks") {
        caps.push("chunks".to_string());
        caps.push("slice".to_string());
    }
    if has("symbol_fts") || has("symbol_fts_data") {
        caps.push("search".to_string());
    }
    if has("cross_file_refs") {
        caps.push("cross-file-refs".to_string());
    }
    if has("graph_labels") {
        caps.push("label".to_string());
    }
    caps
}

/// Run the catalog command: list all databases with introspected schema.
pub fn run_catalog(output_format: OutputFormat) -> Result<()> {
    let registry = read_registry()?;

    if registry.is_empty() {
        match output_format {
            OutputFormat::Human => {
                println!("No databases in registry (~/.magellan/meta.db).");
                println!("Hint: start the magellan daemon or run 'magellan watch' to populate.");
            }
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = JsonResponse::new(Vec::<CatalogEntry>::new(), "catalog");
                output_json(&response, output_format)?;
            }
        }
        return Ok(());
    }

    let mut entries: Vec<CatalogEntry> = registry
        .iter()
        .map(|(name, db_path)| introspect_db(name, db_path))
        .collect();

    // Separate existing from stale for clearer display.
    entries.sort_by(|a, b| b.exists.cmp(&a.exists).then_with(|| a.name.cmp(&b.name)));

    match output_format {
        OutputFormat::Human => {
            let live: Vec<&CatalogEntry> = entries.iter().filter(|e| e.exists).collect();
            let stale: Vec<&CatalogEntry> = entries.iter().filter(|e| !e.exists).collect();

            println!(
                "magellan catalog — {} databases ({} live, {} stale)\n",
                entries.len(),
                live.len(),
                stale.len()
            );
            println!(
                "{:<24} {:<10} {:>8} {:>8}  KINDS",
                "NAME", "STATUS", "ENTITY", "EDGE"
            );
            println!("{}", "─".repeat(90));
            for e in &live {
                println!(
                    "{:<24} {:<10} {:>8} {:>8}  {}",
                    truncate(&e.name, 24),
                    "live",
                    e.entity_count,
                    e.edge_count,
                    truncate(&e.entity_kind_summary(), 40),
                );
            }
            for e in &stale {
                println!(
                    "{:<24} {:<10} {:>8} {:>8}  (db not found on disk)",
                    truncate(&e.name, 24),
                    "stale",
                    "—",
                    "—",
                );
            }
        }
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = JsonResponse::new(&entries, "catalog");
            output_json(&response, output_format)?;
        }
    }

    Ok(())
}

/// Describe a single database in detail: full schema, all kinds, capabilities.
pub fn run_catalog_describe(name: &str, output_format: OutputFormat) -> Result<()> {
    let registry = read_registry()?;
    let db_path = registry
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, p)| p.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Project '{}' not found in registry. Run 'magellan catalog' to list available databases.",
                name
            )
        })?;

    let entry = introspect_db(name, &db_path);

    match output_format {
        OutputFormat::Human => {
            println!("=== {} ===", entry.name);
            println!("path:       {}", entry.db_path);
            println!(
                "status:     {}",
                if entry.exists {
                    "live"
                } else {
                    "stale (db not on disk)"
                }
            );
            if !entry.exists {
                return Ok(());
            }
            println!("entities:   {} total", entry.entity_count);
            for k in &entry.entity_kinds {
                println!("            {:<16} {}", k.kind, k.count);
            }
            println!("edges:      {} total", entry.edge_count);
            for k in &entry.edge_kinds {
                println!("            {:<16} {}", k.kind, k.count);
            }
            println!("tables:     {}", entry.tables.join(", "));
            println!("queryable:  {}", entry.capabilities.join(", "));
        }
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = JsonResponse::new(&entry, "catalog-describe");
            output_json(&response, output_format)?;
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}
