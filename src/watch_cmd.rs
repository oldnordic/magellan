//! Watch command implementation

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{CodeGraph, WatcherConfig, generate_execution_id, OutputFormat, output_json};
use magellan::WatchPipelineConfig;
use magellan::graph::validation;

pub fn run_watch(
    root_path: PathBuf,
    db_path: PathBuf,
    config: WatcherConfig,
    scan_initial: bool,
    validate: bool,
    validate_only: bool,
    output_format: OutputFormat,
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

    let mut graph = CodeGraph::open(&db_path)?;
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

    // Pre-run validation if enabled
    if validate || validate_only {
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

                if output_format == OutputFormat::Json {
                    let response = magellan::output::command::ValidationResponse {
                        passed: false,
                        error_count,
                        errors: report.errors.into_iter().map(|e| magellan::output::command::ValidationError {
                            code: e.code,
                            message: e.message,
                            entity_id: e.entity_id,
                            details: e.details,
                        }).collect(),
                        warning_count: 0,
                        warnings: vec![],
                    };
                    let json_response = magellan::JsonResponse::new(response, &exec_id);
                    output_json(&json_response)?;
                }
                return Err(anyhow::anyhow!("Pre-validation failed"));
            }
            Ok(_) => {}
            Err(e) => {
                let error_msg = format!("Pre-validation error: {}", e);
                graph.execution_log().finish_execution(
                    &exec_id,
                    "error",
                    Some(&error_msg),
                    0,
                    0,
                    0,
                )?;
                return Err(e);
            }
        }
    }

    // If validate-only, run post-validation and exit
    if validate_only {
        let report = match validation::validate_graph(&mut graph) {
            Ok(r) => r,
            Err(e) => {
                let error_msg = format!("Validation error: {}", e);
                graph.execution_log().finish_execution(
                    &exec_id,
                    "error",
                    Some(&error_msg),
                    0,
                    0,
                    0,
                )?;
                return Err(e);
            }
        };

        if output_format == OutputFormat::Json {
            let response = magellan::output::command::ValidationResponse {
                passed: report.passed,
                error_count: report.errors.len(),
                errors: report.errors.into_iter().map(|e| magellan::output::command::ValidationError {
                    code: e.code,
                    message: e.message,
                    entity_id: e.entity_id,
                    details: e.details,
                }).collect(),
                warning_count: report.warnings.len(),
                warnings: report.warnings.into_iter().map(|w| magellan::output::command::ValidationWarning {
                    code: w.code,
                    message: w.message,
                    entity_id: w.entity_id,
                    details: w.details,
                }).collect(),
            };
            let json_response = magellan::JsonResponse::new(response, &exec_id);
            output_json(&json_response)?;
        } else {
            if report.passed {
                println!("Validation passed: no errors found");
            } else {
                eprintln!("Validation failed: {} errors", report.errors.len());
                for error in &report.errors {
                    eprintln!("  [{}] {}", error.code, error.message);
                }
            }
            if !report.warnings.is_empty() {
                eprintln!("Warnings: {}", report.warnings.len());
                for warning in &report.warnings {
                    eprintln!("  [{}] {}", warning.code, warning.message);
                }
            }
        }

        let outcome = if report.passed { "success" } else { "validation_failed" };
        graph.execution_log().finish_execution(&exec_id, outcome, None, 0, 0, 0)?;
        return Ok(());
    }

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
    let pipeline_config = WatchPipelineConfig::new(root_path, db_path.clone(), config, scan_initial);

    // Run the deterministic watch pipeline
    let result = match magellan::run_watch_pipeline(pipeline_config, shutdown) {
        Ok(_) => {
            // Post-run validation if enabled
            if validate {
                let report = match validation::validate_graph(&mut graph) {
                    Ok(r) => r,
                    Err(e) => {
                        let error_msg = format!("Post-validation error: {}", e);
                        graph.execution_log().finish_execution(
                            &exec_id,
                            "error",
                            Some(&error_msg),
                            0,
                            0,
                            0,
                        )?;
                        return Err(e);
                    }
                };

                if !report.passed {
                    let error_count = report.errors.len();
                    if output_format == OutputFormat::Json {
                        let response = magellan::output::command::ValidationResponse {
                            passed: report.passed,
                            error_count,
                            errors: report.errors.into_iter().map(|e| magellan::output::command::ValidationError {
                                code: e.code,
                                message: e.message,
                                entity_id: e.entity_id,
                                details: e.details,
                            }).collect(),
                            warning_count: report.warnings.len(),
                            warnings: report.warnings.into_iter().map(|w| magellan::output::command::ValidationWarning {
                                code: w.code,
                                message: w.message,
                                entity_id: w.entity_id,
                                details: w.details,
                            }).collect(),
                        };
                        let json_response = magellan::JsonResponse::new(response, &exec_id);
                        output_json(&json_response)?;
                    } else {
                        eprintln!("Validation failed: {} errors", error_count);
                        for error in &report.errors {
                            eprintln!("  [{}] {}", error.code, error.message);
                        }
                    }
                    let error_msg = format!("Post-validation failed: {} errors", error_count);
                    graph.execution_log().finish_execution(
                        &exec_id,
                        "validation_failed",
                        Some(&error_msg),
                        0,
                        0,
                        0,
                    )?;
                    return Err(anyhow::anyhow!("Post-validation failed"));
                }

                // Validation passed - optionally show warnings
                if !report.warnings.is_empty() && output_format == OutputFormat::Human {
                    eprintln!("Validation passed with {} warnings", report.warnings.len());
                    for warning in &report.warnings {
                        eprintln!("  [{}] {}", warning.code, warning.message);
                    }
                }
            }
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
