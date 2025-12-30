//! Verify command implementation

use anyhow::Result;
use std::path::PathBuf;

use magellan::CodeGraph;

pub fn run_verify(root_path: PathBuf, db_path: PathBuf) -> Result<u8> {
    let mut graph = CodeGraph::open(&db_path)?;
    let report = magellan::verify::verify_graph(&mut graph, &root_path)?;

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

    if report.is_clean() {
        println!("All files up to date.");
        Ok(0)
    } else {
        println!("Total: {} issues", report.total_issues());
        Ok(1)
    }
}
