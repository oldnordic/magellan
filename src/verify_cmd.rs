//! Verify command implementation

use anyhow::Result;
use std::path::PathBuf;

use crate::{generate_execution_id, CodeGraph};

pub fn run_verify(root_path: PathBuf, db_path: PathBuf) -> Result<u8> {
    // Build args for execution tracking
    let args = vec![
        "verify".to_string(),
        "--root".to_string(),
        root_path.to_string_lossy().to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();
    let root_str = root_path.to_string_lossy().to_string();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        Some(&root_str),
        &db_path_str,
    )?;

    let mut graph_mut = CodeGraph::open(&db_path)?;
    let report = magellan::verify::verify_graph(&mut graph_mut, &root_path)?;

    let root_str = root_path.to_string_lossy();
    println!("Database verification: {}", root_str);

    if !report.missing.is_empty() {
        println!("Missing files ({}):", report.missing.len());
        for path in &report.missing {
            println!("  - {}", path);
        }
    }

    if !report.new.is_empty() {
        println!("New files ({}):", report.new.len());
        for path in &report.new {
            println!("  + {}", path);
        }
    }

    if !report.modified.is_empty() {
        println!("Modified files ({}):", report.modified.len());
        for path in &report.modified {
            println!("  ~ {}", path);
        }
    }

    if !report.stale.is_empty() {
        println!("Stale files ({}):", report.stale.len());
        for path in &report.stale {
            println!("  ? {}", path);
        }
    }

    let exit_code = if report.is_clean() {
        println!("All files up to date.");
        0u8
    } else {
        println!("Total: {} issues", report.total_issues());
        1u8
    };

    let outcome = if exit_code == 0 { "success" } else { "success" }; // Verification success even if issues found
    graph
        .execution_log()
        .finish_execution(&exec_id, outcome, None, 0, 0, 0)?;

    Ok(exit_code)
}
