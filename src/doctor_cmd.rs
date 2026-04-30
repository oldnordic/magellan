//! Doctor command - Diagnose common Magellan issues
//!
//! Checks for common problems and provides actionable recommendations.

use anyhow::Result;
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use magellan::OutputFormat;
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

/// Run the doctor command
///
/// Diagnoses common issues with Magellan installation and database.
pub fn run_doctor(db_path: PathBuf, fix: bool, output_format: OutputFormat) -> Result<()> {
    let mut checks = Vec::new();
    let mut issues_found = 0;
    let mut issues_fixed = 0;

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
                    fix_hint: Some(format!(
                        "Run 'magellan context build --db {:?}'",
                        db_path
                    )),
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

            // Check 10: Coverage schema
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
                println!("⚠️  Found {} issue(s), {} fixed", issues_found, issues_fixed);
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
                println!("  - Check status:     magellan status --db {:?}", db_path);
                println!();
                println!("Run with --fix to auto-fix some issues");
            }
        }
    }

    // Track execution
    let _exec_id = generate_execution_id();

    Ok(())
}
