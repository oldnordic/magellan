//! Watch command implementation

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use magellan::{detect_language, CodeGraph, EventType, FileSystemWatcher, WatcherConfig};

pub fn run_watch(
    root_path: PathBuf,
    db_path: PathBuf,
    config: WatcherConfig,
    scan_initial: bool,
) -> Result<()> {
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

    // Open graph
    let mut graph = CodeGraph::open(&db_path)?;

    // Phase 5.1: Initial full scan if requested
    if scan_initial {
        println!("Scanning {}...", root_path.display());
        let file_count = graph.scan_directory(
            &root_path,
            Some(&|current, total| {
                println!("Scanning... {}/{}", current, total);
            }),
        )?;
        println!("Scanned {} files", file_count);
    }

    // Create watcher
    let watcher = FileSystemWatcher::new(root_path.clone(), config)?;

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

                // Skip unsupported source files (only process known languages)
                if detect_language(&event.path).is_none() {
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
                            event.event_type, path_str, symbol_count, ref_count
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
