//! Watch command implementation

// Debug macro - only compiles in when debug-prints feature is enabled
#[cfg(feature = "debug-prints")]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[cfg(not(feature = "debug-prints"))]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        ()
    };
}

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::{generate_execution_id, OutputFormat};
use magellan::backend_router::{BackendType, MagellanBackend};
use magellan::graph::validation;
use magellan::WatchPipelineConfig;
use magellan::WatcherConfig;

pub fn run_watch(
    root_path: PathBuf,
    db_path: PathBuf,
    config: WatcherConfig,
    scan_initial: bool,
    validate: bool,
    validate_only: bool,
    _output_format: OutputFormat,
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
    if validate {
        args.push("--validate".to_string());
    }
    if validate_only {
        args.push("--validate-only".to_string());
    }
    args.push("--debounce-ms".to_string());
    args.push(config.debounce_ms.to_string());

    let exec_id = generate_execution_id();
    let root_str = root_path.to_string_lossy().to_string();
    let db_path_str = db_path.to_string_lossy().to_string();

    // Detect backend type early to determine if we need a backend reference
    let backend_type = MagellanBackend::detect_type(&db_path);
    debug_print!(
        "[WATCH_DEBUG] Detected backend type: {:?} for db_path: {:?}",
        backend_type,
        db_path
    );

    // For Geometric backend, the pipeline manages its own backend instance.
    // For SQLite, we need the backend here for execution logging.
    let mut backend = if matches!(backend_type, BackendType::Geometric) {
        // For geometric, we don't create a backend here - the pipeline will manage it
        None
    } else {
        Some(MagellanBackend::open_or_create(&db_path)?)
    };

    // Start execution log if supported (SQLite only)
    if let Some(MagellanBackend::SQLite(ref mut graph)) = &mut backend {
        graph.execution_log().start_execution(
            &exec_id,
            env!("CARGO_PKG_VERSION"),
            &args,
            Some(&root_str),
            &db_path_str,
        )?;
    }

    // Pre-run validation if enabled (SQLite only)
    if validate || validate_only {
        if let Some(MagellanBackend::SQLite(ref mut graph)) = &mut backend {
            let input_paths = vec![root_path.clone()];
            match validation::pre_run_validate(&db_path, &root_path, &input_paths) {
                Ok(report) if !report.passed => {
                    let error_count = report.errors.len();
                    let error_msg = format!("Pre-validation failed: {} errors", error_count);
                    graph.execution_log().finish_execution(
                        &exec_id,
                        "error",
                        Some(&error_msg),
                        0,
                        0,
                        0,
                    )?;
                    return Err(anyhow::anyhow!("Pre-validation failed"));
                }
                Ok(_) => {}
                Err(e) => return Err(e),
            }
            if validate_only {
                graph
                    .execution_log()
                    .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
                return Ok(());
            }
        }
    }

    // Create shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Register signal handlers for SIGINT and SIGTERM
    #[cfg(unix)]
    {
        use signal_hook::consts::signal;
        use signal_hook::flag;
        let _ = flag::register(signal::SIGINT, shutdown_clone.clone())?;
        let _ = flag::register(signal::SIGTERM, shutdown_clone.clone())?;
    }

    // Warmup parsers
    let _ = magellan::ingest::pool::warmup_parsers();

    // Create pipeline configuration
    let pipeline_config =
        WatchPipelineConfig::new(root_path, db_path.clone(), config, scan_initial);

    // Run the deterministic watch pipeline based on backend type
    let result = match backend_type {
        BackendType::Geometric => {
            magellan::indexer::run_watch_pipeline_geometric(pipeline_config, shutdown)
        }
        _ => magellan::run_watch_pipeline(pipeline_config, shutdown),
    };

    // Record execution completion (SQLite only)
    if let Some(MagellanBackend::SQLite(ref mut graph)) = &mut backend {
        let outcome = if result.is_ok() { "success" } else { "error" };
        let error_msg = result.as_ref().err().map(|e| e.to_string());
        let _ = graph.execution_log().finish_execution(
            &exec_id,
            outcome,
            error_msg.as_deref(),
            0,
            0,
            0,
        );
    }

    match result {
        Ok(count) => {
            println!("SHUTDOWN");
            println!("Watch session complete. Processed {} events.", count);
            Ok(())
        }
        Err(e) => {
            println!("SHUTDOWN");
            Err(e)
        }
    }
}
