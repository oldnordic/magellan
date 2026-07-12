//! Verify command implementation

use anyhow::Result;
use std::path::PathBuf;

use crate::{status_cmd::ExecutionTracker, CodeGraph};
use magellan::output::command::OutputFormat;

pub fn run_verify(root_path: PathBuf, db_path: PathBuf, output_format: OutputFormat) -> Result<u8> {
    let graph = CodeGraph::open(&db_path)?;
    let mut tracker = ExecutionTracker::new(
        vec![
            "verify".to_string(),
            "--root".to_string(),
            root_path.to_string_lossy().to_string(),
            "--db".to_string(),
            db_path.to_string_lossy().to_string(),
        ],
        Some(root_path.to_string_lossy().to_string()),
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;
    let exec_id = tracker.exec_id().to_string();

    let result = (|| -> Result<u8> {
        graph
            .telemetry()
            .record_phase_start(&exec_id, "verify_graph")?;
        let mut graph_mut = CodeGraph::open(&db_path)?;
        let report = magellan::verify::verify_graph(&mut graph_mut, &root_path)?;
        graph
            .telemetry()
            .record_phase_end(&exec_id, "verify_graph")?;

        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = serde_json::json!({
                    "schema_version": "1.0.0",
                    "execution_id": &exec_id,
                    "data": {
                        "root_path": root_path.to_string_lossy(),
                        "db_path": db_path.to_string_lossy(),
                        "missing": report.missing,
                        "new": report.new,
                        "modified": report.modified,
                        "stale": report.stale,
                        "is_clean": report.is_clean(),
                        "total_issues": report.total_issues(),
                    },
                    "tool": "magellan",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let json_str = match output_format {
                    OutputFormat::Pretty => serde_json::to_string_pretty(&response)?,
                    _ => serde_json::to_string(&response)?,
                };
                println!("{json_str}");
                Ok(if report.is_clean() { 0 } else { 1 })
            }
            OutputFormat::Human => {
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

                Ok(exit_code)
            }
        }
    })();

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}
