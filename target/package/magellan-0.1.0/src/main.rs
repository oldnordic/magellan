//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]

use anyhow::Result;
use magellan::{CodeGraph, FileSystemWatcher, WatcherConfig, EventType};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn print_usage() {
    eprintln!("Usage: magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  --root <DIR>        Directory to watch recursively");
    eprintln!("  --db <FILE>         Path to sqlitegraph database");
    eprintln!("  --debounce-ms <N>   Debounce delay in milliseconds (default: 500)");
}

fn parse_args() -> Result<(PathBuf, PathBuf, WatcherConfig, bool)> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];
    if command != "watch" {
        return Err(anyhow::anyhow!("Unknown command: {}", command));
    }

    let mut root_path: Option<PathBuf> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut debounce_ms: u64 = 500; // Default from WatcherConfig::default()
    let mut status_only = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--db" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--db requires an argument"));
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--debounce-ms" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--debounce-ms requires an argument"));
                }
                debounce_ms = args[i + 1].parse()?;
                i += 2;
            }
            "--status" => {
                status_only = true;
                i += 1;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    let root_path = root_path.ok_or_else(|| anyhow::anyhow!("--root is required"))?;
    let db_path = db_path.ok_or_else(|| anyhow::anyhow!("--db is required"))?;

    let config = WatcherConfig { debounce_ms };

    Ok((root_path, db_path, config, status_only))
}

fn run_status(db_path: PathBuf) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;

    println!("files: {}", file_count);
    println!("symbols: {}", symbol_count);
    println!("references: {}", reference_count);

    Ok(())
}


fn run_watch(root_path: PathBuf, db_path: PathBuf, config: WatcherConfig) -> Result<()> {
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

    // Create watcher
    let watcher = FileSystemWatcher::new(root_path.clone(), config)?;

    // Open graph
    let mut graph = CodeGraph::open(&db_path)?;

    println!("Magellan watching: {}", root_path.display());
    println!("Database: {}", db_path.display());

    // Process events until shutdown flag is set
    loop {
        // Check shutdown flag
        if shutdown.load(Ordering::SeqCst) {
            println!("SHUTDOWN");
            break;
        }

        // Use try_recv to avoid blocking forever
        match watcher.try_recv_event() {
            Some(event) => {
                let path_str = event.path.to_string_lossy().to_string();

                // Skip non-Rust source files (e.g., .db, .db-journal)
                if !path_str.ends_with(".rs") {
                    continue;
                }

                match event.event_type {
                    EventType::Create | EventType::Modify => {
                        // Read file contents
                        let source = match std::fs::read(&event.path) {
                            Ok(s) => s,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                // File was deleted or doesn't exist yet, skip
                                continue;
                            }
                            Err(e) => {
                                // Log error and continue processing other events
                                println!("ERROR {} {}", path_str, e);
                                continue;
                            }
                        };

                        // Delete old data (idempotent)
                        let _ = graph.delete_file(&path_str);

                        // Index symbols
                        let symbol_count = match graph.index_file(&path_str, &source) {
                            Ok(n) => n,
                            Err(e) => {
                                println!("ERROR {} {}", path_str, e);
                                continue;
                            }
                        };

                        // Index references
                        let ref_count = match graph.index_references(&path_str, &source) {
                            Ok(n) => n,
                            Err(e) => {
                                println!("ERROR {} {}", path_str, e);
                                continue;
                            }
                        };

                        println!(
                            "{} {} symbols={} refs={}",
                            event.event_type,
                            path_str,
                            symbol_count,
                            ref_count
                        );
                    }
                    EventType::Delete => {
                        // Delete file and all derived data
                        let _ = graph.delete_file(&path_str);
                        println!("DELETE {}", path_str);
                    }
                }
            }
            None => {
                // No event available, sleep a bit then check shutdown flag
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match parse_args() {
        Ok((root_path, db_path, config, status_only)) => {
            if status_only {
                if let Err(e) = run_status(db_path) {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            } else {
                if let Err(e) = run_watch(root_path, db_path, config) {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            print_usage();
            ExitCode::from(1)
        }
    }
}
