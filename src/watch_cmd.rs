//! Watch command implementation

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use magellan::{WatcherConfig, WatchPipelineConfig};

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

    // Create pipeline configuration
    let pipeline_config = WatchPipelineConfig::new(root_path, db_path, config, scan_initial);

    // Run the deterministic watch pipeline
    match magellan::run_watch_pipeline(pipeline_config, shutdown) {
        Ok(_) => {
            println!("SHUTDOWN");
            Ok(())
        }
        Err(e) => Err(e),
    }
}
