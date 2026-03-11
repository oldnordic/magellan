//! Web UI command implementation
//!
//! Starts the web server for code exploration.

use anyhow::Result;
use std::path::PathBuf;

/// Run the web UI server
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `host` - Host to bind to
/// * `port` - Port to listen on
pub fn run_web_ui(db_path: PathBuf, host: String, port: u16) -> Result<()> {
    #[cfg(feature = "web-ui")]
    {
        use crate::web_ui::run_web_server;

        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(run_web_server(db_path, host, port))?;
    }

    #[cfg(not(feature = "web-ui"))]
    {
        eprintln!("Error: Web UI feature not enabled");
        eprintln!("Rebuild with: cargo build --features web-ui");
        return Err(anyhow::anyhow!("Web UI feature not enabled"));
    }

    Ok(())
}
