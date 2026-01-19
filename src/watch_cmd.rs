//! Watch command implementation

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{CodeGraph, WatcherConfig, generate_execution_id};
use magellan::WatchPipelineConfig;

pub fn run_watch(
    root_path: PathBuf,
    db_path: PathBuf,
    config: WatcherConfig,
    scan_initial: bool,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec![
        "watch".to_string(),
        "--root".to_string(),
        root_path.to_string_lossy().to_string(),
        "--db".to_string(),
        db_path.to_string_lossy().to_string(),
    ];
    if !scan_initial {
        args.push("--watch-only".to_string());
    }
    args.push("--debounce-ms".to_string());
    args.push(config.debounce_ms.to_string());

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

    // Create shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Register signal handlers for SIGINT and SIGTERM
    #[cfg(unix)]
    {
        use signal_hook::consts::signal;
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([signal::SIGTERM, signal::SIGINT])?;

        std::thread::spawn(move || {
            for _ in &mut signals {
                shutdown_clone.store(true, Ordering::SeqCst);
                break;
            }
        });
    }

    // Create pipeline configuration
    let pipeline_config = WatchPipelineConfig::new(root_path, db_path, config, scan_initial);

    // Run the deterministic watch pipeline
    let result = match magellan::run_watch_pipeline(pipeline_config, shutdown) {
        Ok(_) => {
            println!("SHUTDOWN");
            Ok(())
        }
        Err(e) => Err(e),
    };

    // Record execution completion
    let outcome = if result.is_ok() { "success" } else { "error" };
    let error_msg = result.as_ref().err().map(|e| e.to_string());
    graph.execution_log().finish_execution(&exec_id, outcome, error_msg.as_deref(), 0, 0, 0)?;

    result
}
