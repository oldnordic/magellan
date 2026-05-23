//! Service daemon: single-process indexer that manages N project watchers
//!
//! Architecture:
//! - Admin socket: unix domain socket at /tmp/magellan.sock (CLI control)
//! - Registry: ~/.config/magellan/registry.toml (persistent project list)
//! - Watcher: one FileSystemWatcher per enabled project root
//! - Dispatcher: tagged batch queue → worker pool
//! - Shutdown: signal_hook + tokio::sync::watch
//!
//! Phase 0: skeleton. Watcher/dispatcher wiring in Phase 1.
#![allow(dead_code)]

use anyhow::{Context, Result};
use signal_hook::{consts::SIGTERM, iterator::Signals};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};

use crate::service::admin_socket::AdminSocket;
use crate::service::registry::Registry;
use crate::service::types::TaggedBatch;

mod admin_socket;
pub mod registry;
mod types;

pub const SOCKET_PATH: &str = "/tmp/magellan.sock";

/// Service daemon state
pub struct Service {
    registry: Arc<tokio::sync::Mutex<Registry>>,
    shutdown: watch::Sender<bool>,
    batch_tx: mpsc::Sender<TaggedBatch>,
}

impl Service {
    /// Build daemon from registry; fail if no enabled projects
    pub async fn new() -> Result<(Self, watch::Receiver<bool>)> {
        let registry = Registry::load().context("Failed to load project registry")?;

        if registry.enabled_names().is_empty() {
            anyhow::bail!(
                "No enabled projects in registry. Add one with 'magellan service register --root <path>'"
            );
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (batch_tx, batch_rx) = mpsc::channel::<TaggedBatch>(1024);

        let reg = Arc::new(tokio::sync::Mutex::new(registry));

        // Spawn worker task
        let global_db = Registry::canonical_db_path("global");
        tokio::spawn(worker_loop(batch_rx, shutdown_rx.clone(), global_db));

        // Start per-project watchers (Phase 1)
        {
            let names = reg.lock().await.enabled_names();
            for name in names {
                let _ = name;
                // Phase 1: spawn watcher task here
            }
        }

        let svc = Self {
            registry: reg,
            shutdown: shutdown_tx,
            batch_tx,
        };

        Ok((svc, shutdown_rx))
    }

    /// Graceful shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Run the main event loop: signal handler + admin socket
    pub async fn run(self) -> Result<()> {
        let socket = Arc::new(self.setup_socket().await?);

        // Signal handler task
        let shutdown_tx = self.shutdown.clone();
        tokio::task::spawn_blocking(move || {
            let mut signals = Signals::new([signal_hook::consts::SIGINT, SIGTERM])
                .expect("Failed to register signal handler");
            if let Some(_sig) = signals.forever().next() {
                let _ = shutdown_tx.send(true);
            }
        });

        // Accept admin connections
        let mut shutdown_rx = self.shutdown.subscribe();
        let registry = self.registry.clone();

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                Ok((stream, _)) = socket.accept() => {
                    let reg = registry.clone();
                    let tx = self.batch_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = AdminSocket::handle_client(stream, reg, tx).await {
                            eprintln!("Admin socket handler error: {}", e);
                        }
                    });
                }
            }
        }

        self.cleanup().await;
        Ok(())
    }

    async fn setup_socket(&self) -> Result<UnixListener> {
        let path = PathBuf::from(SOCKET_PATH);
        let _ = tokio::fs::remove_file(&path).await;
        let listener = UnixListener::bind(&path)
            .with_context(|| format!("Failed to bind admin socket at {}", path.display()))?;
        Ok(listener)
    }

    async fn cleanup(&self) {
        let _ = tokio::fs::remove_file(SOCKET_PATH).await;
    }
}

/// Worker loop: receives TaggedBatch and dispatches to indexer
async fn worker_loop(
    mut rx: mpsc::Receiver<TaggedBatch>,
    mut shutdown: watch::Receiver<bool>,
    _global_db: PathBuf,
) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            Some(batch) = rx.recv() => {
                eprintln!(
                    "[daemon] {:?} files for project '{}'",
                    batch.paths.len(),
                    batch.project_name
                );
                // Phase 1: call run_watch_pipeline() for batch.paths in batch.project_name context
            }
        }
    }
}

/// Send a JSON-RPC request to the daemon via unix socket and return the response
pub async fn send_request(req: serde_json::Value) -> Result<serde_json::Value> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("Daemon does not appear to be running (socket not found)")?;

    let req_line = serde_json::to_string(&req)? + "\n";
    stream.write_all(req_line.as_bytes()).await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    let resp: serde_json::Value =
        serde_json::from_slice(&buf).context("Failed to parse daemon response")?;
    Ok(resp)
}

/// Synchronous probe: check if daemon socket exists and responds to ping.
/// Returns `true` only if the socket file exists AND a ping request receives
/// a response containing "pong" within 200 ms.
pub fn is_daemon_running() -> bool {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = std::path::PathBuf::from(SOCKET_PATH);
    if !path.exists() {
        return false;
    }

    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));

    let ping = r#"{"id":"probe-1","method":"ping","params":{}}"#;
    if stream.write_all(ping.as_bytes()).is_err() {
        return false;
    }

    let mut buf = [0u8; 1024];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => {
            let resp = String::from_utf8_lossy(&buf[..n]);
            resp.contains("\"pong\"") || resp.contains("pong")
        }
        _ => false,
    }
}

#[cfg(test)]
mod integration_tests {
    use std::path::PathBuf;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};

    // P0-12a: Registry roundtrip — write, read back, reload, verify contents
    #[tokio::test]
    async fn test_registry_write_readback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.toml");

        let mut reg = super::registry::Registry::load_from(path.clone()).unwrap();
        let e1 = super::types::ProjectEntry::new(
            "alpha".into(),
            PathBuf::from("/tmp/roots/alpha"),
            PathBuf::from("/tmp/alpha.db"),
            "cargo".into(),
        );
        let e2 = super::types::ProjectEntry::new(
            "beta".into(),
            PathBuf::from("/tmp/roots/beta"),
            PathBuf::from("/tmp/beta.db"),
            "cargo".into(),
        );
        reg.register(e1).unwrap();
        reg.register(e2).unwrap();

        let reg2 = super::registry::Registry::load_from(path).unwrap();
        let names = reg2.names();
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
        let alpha = reg2.find("alpha").expect("alpha missing after reload");
        assert_eq!(alpha.root, PathBuf::from("/tmp/roots/alpha"));
    }

    #[tokio::test]
    async fn test_registry_enabled_filtering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.toml");

        let mut reg = super::registry::Registry::load_from(path.clone()).unwrap();
        for name in &["on", "off"] {
            reg.register(super::types::ProjectEntry::new(
                name.to_string(),
                PathBuf::from(format!("/tmp/roots/{}", name)),
                PathBuf::from(format!("/tmp/{}.db", name)),
                "cargo".into(),
            ))
            .unwrap();
        }
        assert_eq!(reg.enabled_names().len(), 2);
        assert!(reg.pause("off").unwrap());
        assert_eq!(reg.enabled_names().len(), 1);
        assert!(reg.enabled_names().contains(&"on".to_string()));
        assert!(!reg.enabled_names().contains(&"off".to_string()));

        let reg2 = super::registry::Registry::load_from(path).unwrap();
        assert_eq!(reg2.enabled_names().len(), 1);
    }

    // P0-12b: Socket ping — spawn a minimal admin socket handler, ping via direct stream
    #[tokio::test]
    async fn test_admin_socket_ping_cycle() {
        let socket_path = "/tmp/magellan_test_ping.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_path = socket.with_extension("toml");
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(reg_path).unwrap(),
        ));

        // Spawn accept loop
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16); // dummy sender for ping test
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(stream, reg, tx).await;
                });
            }
        });

        // Give listener time to bind
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and ping — shutdown write after request so server sees EOF
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let (read_half, mut write_half) = stream.split();
        let req = r#"{"id":"test-ping-1","method":"ping","params":{}}"#;
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            resp.get("result")
                .and_then(|r| r.get("pong"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
    }

    // P0-12c: Multi-root dispatch — register multiple roots, list returns all
    #[tokio::test]
    async fn test_multi_root_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry_multi.toml");

        let mut reg = super::registry::Registry::load_from(path).unwrap();
        for name in &["root_a", "root_b", "root_c"] {
            let entry = super::types::ProjectEntry::new(
                name.to_string(),
                PathBuf::from(format!("/tmp/roots/{}", name)),
                PathBuf::from(format!("/tmp/dbs/{}.db", name)),
                "cargo".into(),
            );
            reg.register(entry).unwrap();
        }

        let names = reg.names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"root_a".to_string()));
        assert!(names.contains(&"root_b".to_string()));
        assert!(names.contains(&"root_c".to_string()));
    }

    // P0-14: Watch dispatch — admin socket "watch" request must queue a TaggedBatch
    #[tokio::test]
    async fn test_admin_socket_watch_dispatches_to_worker_queue() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_watch.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_path = socket.with_extension("toml");
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(reg_path).unwrap(),
        ));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);

        // Spawn accept loop
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(stream, reg, tx).await;
                });
            }
        });

        // Give listener time to bind
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and send watch request
        let mut stream = UnixStream::connect(socket_path).await.expect("connect to socket");
        let req = r#"{"id":"test-watch-1","method":"watch","tag":"alpha","paths":["/tmp/roots/alpha/src/main.rs"]}"#;
        let (read_half, mut write_half) = stream.split();
        write_half.write_all((req.to_string() + "\n").as_bytes()).await.unwrap();
        write_half.shutdown().await.unwrap();

        // Await acknowledgment line
        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.unwrap();
        assert!(n > 0, "expected acknowledgment");
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["result"]["queued"].as_str(), Some("alpha"));

        // Worker queue MUST receive the TaggedBatch
        let batch = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            rx.recv(),
        )
        .await
        .expect("timed out waiting for worker queue")
        .expect("worker channel closed with no batch");
        assert_eq!(batch.project_name, "alpha");
        assert_eq!(batch.paths.len(), 1);
        assert_eq!(
            batch.paths[0],
            std::path::PathBuf::from("/tmp/roots/alpha/src/main.rs")
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
    }
}
