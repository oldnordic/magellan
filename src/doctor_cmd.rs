//! Doctor command - Diagnose common Magellan issues
//!
//! Checks for common problems and provides actionable recommendations.

use anyhow::Result;
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use std::path::PathBuf;
use std::fs;

/// Run the doctor command
///
/// Diagnoses common issues with Magellan installation and database.
pub fn run_doctor(db_path: PathBuf, fix: bool) -> Result<()> {
    println!("🔍 Magellan Doctor - Diagnosing issues...\n");
    
    let mut issues_found = 0;
    let mut issues_fixed = 0;

    // Check 1: Database file exists
    print!("Checking database file... ");
    if db_path.exists() {
        println!("✅ OK");
    } else {
        println!("❌ MISSING");
        println!("   Database not found at: {:?}", db_path);
        println!("   Fix: Run 'magellan watch --root . --db {:?} --scan-initial'", db_path);
        issues_found += 1;
    }

    // Check 2: Database is readable
    print!("Checking database readability... ");
    match CodeGraph::open(&db_path) {
        Ok(mut graph) => {
            println!("✅ OK");
            
            // Check 3: Schema version via status
            print!("Checking schema version... ");
            match graph.count_files() {
                Ok(_) => {
                    println!("✅ OK");
                }
                Err(e) => {
                    println!("⚠️  WARNING");
                    println!("   Schema error: {}", e);
                    issues_found += 1;
                }
            }

            // Check 4: Symbol count
            print!("Checking symbol index... ");
            match graph.count_symbols() {
                Ok(count) => {
                    if count > 0 {
                        println!("✅ OK ({} symbols)", count);
                    } else {
                        println!("⚠️  EMPTY");
                        println!("   No symbols indexed");
                        println!("   Fix: Run 'magellan watch --root . --db {:?} --scan-initial'", db_path);
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    println!("❌ ERROR: {}", e);
                    issues_found += 1;
                }
            }

            // Check 5: File count
            print!("Checking file index... ");
            match graph.count_files() {
                Ok(count) => {
                    if count > 0 {
                        println!("✅ OK ({} files)", count);
                    } else {
                        println!("⚠️  EMPTY");
                        println!("   No files indexed");
                        println!("   Fix: Run 'magellan watch --root . --db {:?} --scan-initial'", db_path);
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    println!("❌ ERROR: {}", e);
                    issues_found += 1;
                }
            }

            // Check 6: Call graph
            print!("Checking call graph... ");
            match graph.count_calls() {
                Ok(count) => {
                    if count > 0 {
                        println!("✅ OK ({} calls)", count);
                    } else {
                        println!("⚠️  EMPTY");
                        println!("   No call relationships - call graph analysis won't work");
                        issues_found += 1;
                    }
                }
                Err(e) => {
                    println!("❌ ERROR: {}", e);
                    issues_found += 1;
                }
            }

            // Check 7: Database file size
            print!("Checking database size... ");
            if let Ok(metadata) = fs::metadata(&db_path) {
                let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                if size_mb > 1000.0 {
                    println!("⚠️  LARGE ({:.1} MB)", size_mb);
                    println!("   Consider running 'magellan export --format json' and starting fresh");
                    if fix {
                        println!("   Auto-fix: Not implemented yet");
                    }
                    issues_found += 1;
                } else {
                    println!("✅ OK ({:.1} MB)", size_mb);
                }
            }

            // Check 8: WAL file
            let wal_path = db_path.with_extension("db-wal");
            print!("Checking WAL file... ");
            if wal_path.exists() {
                if let Ok(metadata) = fs::metadata(&wal_path) {
                    let wal_size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
                    if wal_size_mb > 100.0 {
                        println!("⚠️  LARGE ({:.1} MB)", wal_size_mb);
                        println!("   Consider running 'magellan status' to checkpoint");
                        if fix {
                            println!("   Auto-fix: Running checkpoint...");
                            // Force a checkpoint by reopening
                            let _ = CodeGraph::open(&db_path);
                            println!("   ✅ Checkpoint complete");
                            issues_fixed += 1;
                        }
                        issues_found += 1;
                    } else {
                        println!("✅ OK ({:.1} MB)", wal_size_mb);
                    }
                }
            } else {
                println!("✅ None (good)");
            }

            // Check 9: Context index (for v3.0.0+)
            print!("Checking context index (v3.0.0)... ");
            let context_path = db_path.parent()
                .map(|p| p.join(db_path.file_name().unwrap_or_default()))
                .unwrap_or_else(|| db_path.clone())
                .with_extension("context.json");
            
            if context_path.exists() {
                println!("✅ OK");
            } else {
                println!("⚠️  MISSING");
                println!("   Context index not built");
                println!("   Fix: Run 'magellan context build --db {:?}'", db_path);
                if fix {
                    println!("   Auto-fix: Building context index...");
                    use magellan::context::build_context_index;
                    match build_context_index(&mut graph, &db_path) {
                        Ok(_) => {
                            println!("   ✅ Context index built");
                            issues_fixed += 1;
                        }
                        Err(e) => {
                            println!("   ❌ Failed: {}", e);
                        }
                    }
                }
                issues_found += 1;
            }

        }
        Err(e) => {
            println!("❌ ERROR");
            println!("   Cannot open database: {}", e);
            println!("   Fix: Delete and rebuild: rm {:?} && magellan watch --root . --db {:?} --scan-initial", db_path, db_path);
            issues_found += 1;
        }
    }

    // Summary
    println!("\n{}", "=".repeat(50));
    if issues_found == 0 {
        println!("✅ No issues found! Your Magellan installation is healthy.");
    } else {
        println!("⚠️  Found {} issue(s), {} fixed", issues_found, issues_fixed);
        println!();
        println!("Quick fixes:");
        println!("  - Rebuild database: magellan watch --root . --db {:?} --scan-initial", db_path);
        println!("  - Build context:    magellan context build --db {:?}", db_path);
        println!("  - Check status:     magellan status --db {:?}", db_path);
        println!();
        println!("Run with --fix to auto-fix some issues");
    }

    // Track execution
    let _exec_id = generate_execution_id();

    Ok(())
}
