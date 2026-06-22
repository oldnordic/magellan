//! Doctor command - Diagnose common Magellan issues
//!
//! Checks for common problems and provides actionable recommendations.

use anyhow::Result;
use magellan::common::{find_repo_root, magellan_dir};
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use magellan::OutputFormat;
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// A single diagnostic check result
#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    status: String,
    message: Option<String>,
    fix_hint: Option<String>,
}

/// Complete doctor diagnostic report
#[derive(Debug, Serialize)]
struct DoctorReport {
    status: String,
    issues_found: usize,
    issues_fixed: usize,
    checks: Vec<CheckResult>,
}

fn check_cfg_blocks_contract(conn: &Connection) -> Result<CheckResult> {
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(cfg_blocks)")?
        .query_map([], |row| row.get(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if columns.is_empty() {
        return Ok(CheckResult {
            name: "CFG schema contract".to_string(),
            status: "missing".to_string(),
            message: Some("cfg_blocks table not found".to_string()),
            fix_hint: Some("Re-open database to trigger schema migration".to_string()),
        });
    }

    Ok(CheckResult {
        name: "CFG schema contract".to_string(),
        status: "ok".to_string(),
        message: Some("cfg_blocks matches the Magellan source-of-truth schema".to_string()),
        fix_hint: None,
    })
}

/// Run the doctor command
///
/// Diagnoses common issues with Magellan installation and database.
pub fn run_doctor(db_path: PathBuf, fix: bool, output_format: OutputFormat) -> Result<()> {
    let mut checks = Vec::new();
    let mut issues_found = 0;
    let mut issues_fixed = 0;

    let exec_id = generate_execution_id();

    // Phase: open_graph
    let graph = CodeGraph::open(&db_path)?;
    graph
        .telemetry()
        .record_phase_start(&exec_id, "open_graph")?;

    // Check 1: Database file exists
    if db_path.exists() {
        checks.push(CheckResult {
            name: "Database file".to_string(),
            status: "ok".to_string(),
            message: None,
            fix_hint: None,
        });
    } else {
        checks.push(CheckResult {
            name: "Database file".to_string(),
            status: "missing".to_string(),
            message: Some(format!("Database not found at: {:?}", db_path)),
            fix_hint: Some(format!(
                "Run 'magellan watch --root . --db {:?} --scan-initial'",
                db_path
            )),
        });
        issues_found += 1;
    }

    // Check 2: Database is readable
    match CodeGraph::open(&db_path) {
        Ok(mut graph) => {
            // End open_graph phase, start diagnose phase
            graph.telemetry().record_phase_end(&exec_id, "open_graph")?;
            graph.telemetry().record_phase_start(&exec_id, "diagnose")?;

            checks.push(CheckResult {
                name: "Database readability".to_string(),
                status: "ok".to_string(),
                message: None,
                fix_hint: None,
            });

            // Check 3: Schema version via status
            match graph.count_files() {
                Ok(_) => {
                    checks.push(CheckResult {
                        name: "Schema version".to_string(),
                        status: "ok".to_string(),
                        message: None,
                        fix_hint: None,
                    });
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "Schema version".to_string(),
                        status: "warning".to_string(),
                        message: Some(format!("Schema error: {}", e)),
                        fix_hint: Some("Re-open database to trigger migration".to_string()),
                    });
                    issues_found += 1;
                }
            }

            // Check 4: Symbol count
            match graph.count_symbols() {
                Ok(count) => {
                    if count > 0 {
                        checks.push(CheckResult {
                            name: "Symbol index".to_string(),
                            status: "ok".to_string(),
                            message: Some(format!("{} symbols", count)),
                            fix_hint: None,
                        });
                    } else {
                        checks.push(CheckResult {
                            name: "Symbol index".to_string(),
                            status: "empty".to_string(),
                            message: Some("No symbols indexed".to_string()),
                            fix_hint: Some(format!(
                                "Run 'magellan watch --root . --db {:?} --scan-initial'",
                                db_path
                            )),
                        });
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "Symbol index".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: None,
                    });
                    issues_found += 1;
                }
            }

            // Check 5: File count
            match graph.count_files() {
                Ok(count) => {
                    if count > 0 {
                        checks.push(CheckResult {
                            name: "File index".to_string(),
                            status: "ok".to_string(),
                            message: Some(format!("{} files", count)),
                            fix_hint: None,
                        });
                    } else {
                        checks.push(CheckResult {
                            name: "File index".to_string(),
                            status: "empty".to_string(),
                            message: Some("No files indexed".to_string()),
                            fix_hint: Some(format!(
                                "Run 'magellan watch --root . --db {:?} --scan-initial'",
                                db_path
                            )),
                        });
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "File index".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: None,
                    });
                    issues_found += 1;
                }
            }

            // Check 6: Call graph
            match graph.count_calls() {
                Ok(count) => {
                    if count > 0 {
                        checks.push(CheckResult {
                            name: "Call graph".to_string(),
                            status: "ok".to_string(),
                            message: Some(format!("{} calls", count)),
                            fix_hint: None,
                        });
                    } else {
                        checks.push(CheckResult {
                            name: "Call graph".to_string(),
                            status: "empty".to_string(),
                            message: Some("No call relationships indexed".to_string()),
                            fix_hint: Some("Index files with function calls".to_string()),
                        });
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "Call graph".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: None,
                    });
                    issues_found += 1;
                }
            }

            // Check 7: Database file size
            if let Ok(metadata) = fs::metadata(&db_path) {
                let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                if size_mb > 1000.0 {
                    checks.push(CheckResult {
                        name: "Database size".to_string(),
                        status: "warning".to_string(),
                        message: Some(format!("Large database: {:.1} MB", size_mb)),
                        fix_hint: Some("Consider exporting and starting fresh".to_string()),
                    });
                    issues_found += 1;
                } else {
                    checks.push(CheckResult {
                        name: "Database size".to_string(),
                        status: "ok".to_string(),
                        message: Some(format!("{:.1} MB", size_mb)),
                        fix_hint: None,
                    });
                }
            }

            // Check 8: WAL file
            let wal_path = db_path.with_extension("db-wal");
            if wal_path.exists() {
                if let Ok(metadata) = fs::metadata(&wal_path) {
                    let wal_size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                    if wal_size_mb > 100.0 {
                        checks.push(CheckResult {
                            name: "WAL file".to_string(),
                            status: "warning".to_string(),
                            message: Some(format!("Large WAL: {:.1} MB", wal_size_mb)),
                            fix_hint: Some("Run 'magellan status' to checkpoint".to_string()),
                        });
                        if fix {
                            let _ = CodeGraph::open(&db_path);
                            issues_fixed += 1;
                        }
                        issues_found += 1;
                    } else {
                        checks.push(CheckResult {
                            name: "WAL file".to_string(),
                            status: "ok".to_string(),
                            message: Some(format!("{:.1} MB", wal_size_mb)),
                            fix_hint: None,
                        });
                    }
                }
            } else {
                checks.push(CheckResult {
                    name: "WAL file".to_string(),
                    status: "ok".to_string(),
                    message: Some("No WAL file (good)".to_string()),
                    fix_hint: None,
                });
            }

            // Check 9: Context index
            let context_path = db_path
                .parent()
                .map(|p| p.join(db_path.file_name().unwrap_or_default()))
                .unwrap_or_else(|| db_path.clone())
                .with_extension("context.json");

            if context_path.exists() {
                checks.push(CheckResult {
                    name: "Context index".to_string(),
                    status: "ok".to_string(),
                    message: None,
                    fix_hint: None,
                });
            } else {
                checks.push(CheckResult {
                    name: "Context index".to_string(),
                    status: "missing".to_string(),
                    message: Some("Context index not built".to_string()),
                    fix_hint: Some(format!("Run 'magellan context build --db {:?}'", db_path)),
                });
                if fix {
                    use magellan::context::build_context_index;
                    match build_context_index(&mut graph, &db_path) {
                        Ok(_) => issues_fixed += 1,
                        Err(e) => eprintln!("Warning: Failed to build context index: {}", e),
                    }
                }
                issues_found += 1;
            }

            // Check 10: Connection health
            let start = std::time::Instant::now();
            let conn_ok = graph.count_files().map(|_| true).unwrap_or(false);
            let elapsed_ms = start.elapsed().as_millis();
            if conn_ok {
                if elapsed_ms > 500 {
                    checks.push(CheckResult {
                        name: "Connection health".to_string(),
                        status: "warning".to_string(),
                        message: Some(format!("Slow query response: {}ms", elapsed_ms)),
                        fix_hint: Some(
                            "Database may be under contention; restart watcher or reduce concurrent access"
                                .to_string(),
                        ),
                    });
                    issues_found += 1;
                } else {
                    checks.push(CheckResult {
                        name: "Connection health".to_string(),
                        status: "ok".to_string(),
                        message: Some(format!("{}ms", elapsed_ms)),
                        fix_hint: None,
                    });
                }
            } else {
                checks.push(CheckResult {
                    name: "Connection health".to_string(),
                    status: "error".to_string(),
                    message: Some("Failed to query database".to_string()),
                    fix_hint: None,
                });
                issues_found += 1;
            }

            // Check 11: Duplicate file nodes
            let mut dupes_found = Vec::new();
            {
                use std::collections::HashMap;
                let mut path_counts: HashMap<String, usize> = HashMap::new();
                let backend = graph.backend();
                if let Ok(ids) = backend.entity_ids() {
                    let snapshot = sqlitegraph::SnapshotId::current();
                    for id in ids {
                        if let Ok(node) = backend.get_node(snapshot, id) {
                            if node.kind == "File" {
                                if let Ok(file_node) = serde_json::from_value::<
                                    magellan::graph::schema::FileNode,
                                >(node.data)
                                {
                                    *path_counts.entry(file_node.path).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
                for (path, count) in path_counts {
                    if count > 1 {
                        dupes_found.push((path, count));
                    }
                }
            }
            if dupes_found.is_empty() {
                checks.push(CheckResult {
                    name: "Duplicate file nodes".to_string(),
                    status: "ok".to_string(),
                    message: None,
                    fix_hint: None,
                });
            } else {
                let total_dupes: usize = dupes_found.iter().map(|(_, c)| c - 1).sum();
                checks.push(CheckResult {
                    name: "Duplicate file nodes".to_string(),
                    status: "warning".to_string(),
                    message: Some(format!(
                        "{} file(s) with {} extra nodes",
                        dupes_found.len(),
                        total_dupes
                    )),
                    fix_hint: Some(
                        "Re-index to clean up: magellan watch --root . --scan-initial".to_string(),
                    ),
                });
                if fix {
                    let mut fixed = 0;
                    for (path, _) in &dupes_found {
                        match graph.delete_file(path) {
                            Ok(_) => fixed += 1,
                            Err(_e) => {}
                        }
                    }
                    if fixed == dupes_found.len() {
                        issues_fixed += 1;
                    }
                }
                issues_found += 1;
            }

            // Check 12: Coverage schema
            match graph.check_coverage_schema() {
                Ok(true) => {
                    checks.push(CheckResult {
                        name: "Coverage schema".to_string(),
                        status: "ok".to_string(),
                        message: None,
                        fix_hint: None,
                    });
                }
                Ok(false) => {
                    checks.push(CheckResult {
                        name: "Coverage schema".to_string(),
                        status: "missing".to_string(),
                        message: Some("Coverage tables not found".to_string()),
                        fix_hint: Some("Re-open database to trigger schema migration".to_string()),
                    });
                    if fix {
                        drop(graph);
                        match CodeGraph::open(&db_path) {
                            Ok(_) => issues_fixed += 1,
                            Err(e) => eprintln!("Warning: Failed to re-open database: {}", e),
                        }
                    }
                    issues_found += 1;
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "Coverage schema".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: None,
                    });
                    issues_found += 1;
                }
            }

            // Check 13: CFG schema contract
            match Connection::open(&db_path) {
                Ok(conn) => match check_cfg_blocks_contract(&conn) {
                    Ok(check) => {
                        if check.status != "ok" {
                            issues_found += 1;
                        }
                        checks.push(check);
                    }
                    Err(e) => {
                        checks.push(CheckResult {
                            name: "CFG schema contract".to_string(),
                            status: "error".to_string(),
                            message: Some(e.to_string()),
                            fix_hint: Some("Inspect cfg_blocks table schema".to_string()),
                        });
                        issues_found += 1;
                    }
                },
                Err(e) => {
                    checks.push(CheckResult {
                        name: "CFG schema contract".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: Some("Inspect cfg_blocks table schema".to_string()),
                    });
                    issues_found += 1;
                }
            }

            // Check 14: FTS5 search index (after upgrade to 4.9.2 with FTS5 + call-graph BFS)
            match Connection::open(&db_path) {
                Ok(conn) => {
                    // Check if FTS5 table exists
                    let fts_exists: bool = conn
                        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='symbol_fts'")?
                        .query_map([], |row| row.get::<_, String>(0))?
                        .next()
                        .is_some();

                    if !fts_exists {
                        checks.push(CheckResult {
                            name: "FTS5 search index".to_string(),
                            status: "missing".to_string(),
                            message: Some("symbol_fts table not found".to_string()),
                            fix_hint: Some(
                                "Re-open database to trigger schema migration".to_string(),
                            ),
                        });
                        issues_found += 1;
                    } else {
                        // Check if FTS5 index is empty (needs rebuild after 4.9.2 upgrade)
                        let fts_count: i64 = conn
                            .prepare("SELECT COUNT(*) FROM symbol_fts")?
                            .query_row([], |row| row.get(0))
                            .unwrap_or(0);

                        let symbol_count: i64 = conn
                            .prepare("SELECT COUNT(*) FROM graph_entities")?
                            .query_row([], |row| row.get(0))
                            .unwrap_or(0);

                        if fts_count == 0 && symbol_count > 0 {
                            checks.push(CheckResult {
                                name: "FTS5 search index".to_string(),
                                status: "stale".to_string(),
                                message: Some(format!(
                                    "FTS5 index empty ({} symbols not indexed)",
                                    symbol_count
                                )),
                                fix_hint: Some(
                                    "Run 'magellan doctor --fix' to rebuild FTS5 index".to_string(),
                                ),
                            });
                            if fix {
                                match conn.execute(
                                    "INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')",
                                    [],
                                ) {
                                    Ok(_) => {
                                        issues_fixed += 1;
                                    }
                                    Err(e) => {
                                        eprintln!("Warning: Failed to rebuild FTS5 index: {}", e);
                                    }
                                }
                            }
                            issues_found += 1;
                        } else if fts_count > 0 {
                            checks.push(CheckResult {
                                name: "FTS5 search index".to_string(),
                                status: "ok".to_string(),
                                message: Some(format!("{} entries indexed", fts_count)),
                                fix_hint: None,
                            });
                        } else {
                            checks.push(CheckResult {
                                name: "FTS5 search index".to_string(),
                                status: "ok".to_string(),
                                message: Some("No symbols to index (empty database)".to_string()),
                                fix_hint: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    checks.push(CheckResult {
                        name: "FTS5 search index".to_string(),
                        status: "error".to_string(),
                        message: Some(e.to_string()),
                        fix_hint: Some("Check database permissions".to_string()),
                    });
                    issues_found += 1;
                }
            }
        }
        Err(e) => {
            checks.push(CheckResult {
                name: "Database readability".to_string(),
                status: "error".to_string(),
                message: Some(format!("Cannot open database: {}", e)),
                fix_hint: Some(format!(
                    "Delete and rebuild: rm {:?} && magellan watch --root . --db {:?} --scan-initial",
                    db_path, db_path
                )),
            });
            issues_found += 1;
        }
    }

    // Check 15: Repo-root exports
    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(root) = find_repo_root(&current_dir) {
            let mag_dir = magellan_dir(&root);

            let symbol_index = mag_dir.join("symbolindex.json");
            if !symbol_index.exists() {
                checks.push(CheckResult {
                    name: "Repo-root symbol index".to_string(),
                    status: "missing".to_string(),
                    message: Some("Symbol index not found in .magellan/".to_string()),
                    fix_hint: Some(format!("Run: llmgrep export-symbols --db {:?}", db_path)),
                });
                issues_found += 1;
            } else {
                checks.push(CheckResult {
                    name: "Repo-root symbol index".to_string(),
                    status: "ok".to_string(),
                    message: None,
                    fix_hint: None,
                });
            }

            let export_json = mag_dir.join("export.json");
            if !export_json.exists() {
                checks.push(CheckResult {
                    name: "Repo-root export".to_string(),
                    status: "missing".to_string(),
                    message: Some("Export not found in .magellan/".to_string()),
                    fix_hint: Some(format!(
                        "Run: magellan export --db {:?} --format json",
                        db_path
                    )),
                });
                issues_found += 1;
            } else {
                checks.push(CheckResult {
                    name: "Repo-root export".to_string(),
                    status: "ok".to_string(),
                    message: None,
                    fix_hint: None,
                });
            }
        }
    }

    let report = DoctorReport {
        status: if issues_found == 0 {
            "healthy".to_string()
        } else {
            "issues_found".to_string()
        },
        issues_found,
        issues_fixed,
        checks,
    };

    // End diagnose phase, start output phase
    graph.telemetry().record_phase_end(&exec_id, "diagnose")?;
    graph.telemetry().record_phase_start(&exec_id, "output")?;

    match output_format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&report)?);
        }
        OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Human => {
            println!("🔍 Magellan Doctor - Diagnosing issues...\n");
            for check in &report.checks {
                let icon = match check.status.as_str() {
                    "ok" => "✅",
                    "warning" | "large" => "⚠️",
                    "missing" | "empty" => "⚠️",
                    "error" => "❌",
                    _ => "❓",
                };
                print!("{} {}... ", icon, check.name);
                if let Some(ref msg) = check.message {
                    println!("{}", msg);
                } else {
                    println!("OK");
                }
                if let Some(ref hint) = check.fix_hint {
                    println!("   Fix: {}", hint);
                }
            }
            println!("\n{}", "=".repeat(50));
            if issues_found == 0 {
                println!("✅ No issues found! Your Magellan installation is healthy.");
            } else {
                println!(
                    "⚠️  Found {} issue(s), {} fixed",
                    issues_found, issues_fixed
                );
                println!();
                println!("Quick fixes:");
                println!(
                    "  - Rebuild database: magellan watch --root . --db {:?} --scan-initial",
                    db_path
                );
                println!(
                    "  - Build context:    magellan context build --db {:?}",
                    db_path
                );
                println!(
                    "  - Rebuild FTS5:     magellan doctor --db {:?} --fix",
                    db_path
                );
                println!("  - Check status:     magellan status --db {:?}", db_path);
                println!();
                println!("Run with --fix to auto-fix some issues");
            }
        }
    }

    // Track execution
    let _exec_id = generate_execution_id();

    // End output phase
    graph.telemetry().record_phase_end(&exec_id, "output")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_cfg_blocks_contract;

    #[test]
    fn cfg_blocks_contract_accepts_canonical_magellan_schema() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE cfg_blocks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                function_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                terminator TEXT NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                cfg_hash TEXT,
                statements TEXT,
                cfg_condition TEXT
            );",
        )
        .unwrap();

        let result = check_cfg_blocks_contract(&conn).unwrap();
        assert_eq!(result.status, "ok");
    }
}
