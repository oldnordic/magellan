//! Watch command implementation

use crate::status_cmd::ExecutionTracker;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use walkdir::WalkDir;

use magellan::backend_router::MagellanBackend;
use magellan::graph::filter::FileFilter;
use magellan::graph::validation;
use magellan::WatchPipelineConfig;
use magellan::WatcherConfig;
use serde_json::json;

/// Synchronous helper: send JSON-RPC watch request to daemon
fn send_watch_request(req_line: &str, exec_id: &str) -> Result<()> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let socket_path = std::path::PathBuf::from(crate::service::socket_path());
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Daemon socket unreachable at {}", socket_path.display()))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let req_with_newline = format!("{}\n", req_line);
    if let Err(e) = stream.write_all(req_with_newline.as_bytes()) {
        return Err(anyhow::anyhow!(
            "Failed to write watch request to daemon socket: {}",
            e
        ));
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);

    let mut buf = [0u8; 4096];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => {
            let resp = String::from_utf8_lossy(&buf[..n]);
            if resp.contains(r#""error""#) {
                return Err(anyhow::anyhow!("Daemon refused watch request: {}", resp));
            }
            println!(
                "Watch request dispatched to daemon ({}) — response: {}",
                exec_id,
                resp.trim()
            );
            Ok(())
        }
        Ok(_) => Err(anyhow::anyhow!("Daemon socket closed before response")),
        Err(e) => Err(anyhow::anyhow!("Failed to read daemon response: {}", e)),
    }
}

fn collect_daemon_watch_paths(
    root_path: &std::path::Path,
    include: &[String],
    exclude: &[String],
) -> Result<Vec<String>> {
    let filter = FileFilter::new(root_path, include, exclude)?;
    let mut paths = Vec::new();

    for entry in WalkDir::new(root_path) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if filter.should_skip(path).is_none() {
            paths.push(path.to_string_lossy().to_string());
        }
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn daemon_watch_tag(root_path: &std::path::Path) -> String {
    crate::service::registry::Registry::load()
        .ok()
        .and_then(|registry| {
            registry
                .find_by_root(root_path)
                .map(|entry| entry.name.clone())
        })
        .unwrap_or_else(|| root_path.to_string_lossy().to_string())
}

fn build_daemon_watch_request(
    exec_id: &str,
    root_path: &std::path::Path,
    include: &[String],
    exclude: &[String],
) -> Result<String> {
    let paths = collect_daemon_watch_paths(root_path, include, exclude)?;
    Ok(json!({
        "id": exec_id,
        "method": "watch",
        "tag": daemon_watch_tag(root_path),
        "paths": paths,
    })
    .to_string())
}

pub fn run_watch(
    root_path: PathBuf,
    db_path: PathBuf,
    config: WatcherConfig,
    scan_initial: bool,
    validate: bool,
    validate_only: bool,
    compile_commands: Option<std::path::PathBuf>,
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

    let mut tracker = ExecutionTracker::new(
        args,
        Some(root_path.to_string_lossy().to_string()),
        db_path.to_string_lossy().to_string(),
    );
    let exec_id = tracker.exec_id().to_string();

    // svc-8: if daemon is running, signal it instead of local watch
    if crate::service::is_daemon_running() {
        let req_line = build_daemon_watch_request(&exec_id, &root_path, &[], &[])?;
        return send_watch_request(&req_line, &exec_id);
    }

    // Open the backend for execution logging
    let mut backend = Some(MagellanBackend::open_or_create(&db_path)?);

    if let Some(ref backend_ref) = backend {
        tracker.start_backend(backend_ref)?;
    }

    // Pre-run validation if enabled (SQLite only)
    if validate || validate_only {
        if let Some(MagellanBackend::SQLite(ref mut graph)) = &mut backend {
            // Phase: pre_validation
            graph
                .telemetry()
                .record_phase_start(&exec_id, "pre_validation")?;
            let input_paths = vec![root_path.clone()];
            match validation::pre_run_validate(&db_path, &root_path, &input_paths) {
                Ok(report) if !report.passed => {
                    let error_count = report.errors.len();
                    let error_msg = format!("Pre-validation failed: {} errors", error_count);
                    graph
                        .telemetry()
                        .record_phase_end(&exec_id, "pre_validation")?;
                    tracker.set_error(error_msg);
                    if let Some(ref backend_ref) = backend {
                        tracker.finish_backend(backend_ref)?;
                    }
                    return Err(anyhow::anyhow!("Pre-validation failed"));
                }
                Ok(_) => {}
                Err(e) => return Err(e),
            }
            graph
                .telemetry()
                .record_phase_end(&exec_id, "pre_validation")?;
            if validate_only {
                if let Some(ref backend_ref) = backend {
                    tracker.finish_backend(backend_ref)?;
                }
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
    let mut pipeline_config =
        WatchPipelineConfig::new(root_path, db_path.clone(), config, scan_initial);
    pipeline_config.compile_commands_path = compile_commands;

    // Run the deterministic watch pipeline
    let result = magellan::run_watch_pipeline(pipeline_config, shutdown);

    // Record execution completion (SQLite only)
    if let Some(ref backend_ref) = backend {
        if let Err(err) = &result {
            tracker.set_error(err.to_string());
        }
        let _ = tracker.finish_backend(backend_ref);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_watch_request_uses_registered_tag_and_files_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::write(root.join("target/ignored.rs"), "pub fn no() {}\n").unwrap();

        let home = temp.path().join("home");
        std::fs::create_dir_all(home.join(".config/magellan")).unwrap();
        let registry_path = home.join(".config/magellan/config.toml");
        std::fs::write(
            &registry_path,
            format!(
                r#"
version = "1"

[[project]]
name = "alpha"
root = "{}"
db = "{}"
source = "cargo"
enabled = true
"#,
                root.display(),
                temp.path().join("alpha.db").display()
            ),
        )
        .unwrap();

        let old_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", &home);
        }
        let req = build_daemon_watch_request("exec-1", &root, &[], &[]).unwrap();
        match old_home {
            Some(prev) => unsafe {
                std::env::set_var("HOME", prev);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }

        let parsed: serde_json::Value = serde_json::from_str(&req).unwrap();
        assert_eq!(parsed["tag"].as_str(), Some("alpha"));
        let paths = parsed["paths"].as_array().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].as_str(),
            Some(root.join("src/lib.rs").to_string_lossy().as_ref())
        );
    }
}
