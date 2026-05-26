//! Service daemon: single-process indexer that manages N project watchers
//!
//! Architecture:
//! - Admin socket: unix domain socket at /tmp/magellan.sock (CLI control)
//! - Registry: ~/.config/magellan/registry.toml (persistent project list)
//! - Watcher: one FileSystemWatcher per enabled project root
//! - Dispatcher: tagged batch queue -> worker pool
//! - Shutdown: signal_hook + tokio::sync::watch
//!
//! Phase 1: worker_loop -> CodeGraph reconcile. TODO: remove allow(dead_code) when stable.
#![allow(dead_code)]

use anyhow::{Context, Result};
use signal_hook::{consts::SIGTERM, iterator::Signals};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};

use crate::service::admin_socket::AdminSocket;
use crate::service::registry::Registry;
use crate::service::types::TaggedBatch;
use magellan::{FileSystemWatcher, WatcherConfig};

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn make_event(event_type: &str, project: Option<&str>) -> meta_db::DaemonEvent {
    meta_db::DaemonEvent {
        id: None,
        event_type: event_type.to_string(),
        project_name: project.map(|s| s.to_string()),
        file_path: None,
        details: None,
        created_at: now_secs(),
        execution_id: None,
    }
}

mod admin_socket;
mod candidates;
mod meta_db;
pub mod registry;
pub mod structural;
mod types;
mod verify;

/// Return the socket path, respecting `XDG_RUNTIME_DIR` when present
/// (systemd user-level services) with fallback to `/tmp/magellan.sock`.
pub fn socket_path() -> &'static str {
    std::env::var_os("XDG_RUNTIME_DIR")
        .and_then(|p| p.into_string().ok())
        .map(|d| format!("{}/magellan.sock", d))
        .map(|s: String| -> &'static str { Box::leak(s.into_boxed_str()) })
        .unwrap_or("/tmp/magellan.sock")
}
/// Deprecated constant — use [`socket_path()`] instead.
pub const SOCKET_PATH: &str = "/tmp/magellan.sock";

/// Service daemon state
pub struct Service {
    registry: Arc<tokio::sync::Mutex<Registry>>,
    shutdown: watch::Sender<bool>,
    batch_tx: mpsc::Sender<TaggedBatch>,
    worker_shutdown: Arc<AtomicBool>,
    meta_db: Arc<tokio::sync::Mutex<meta_db::MetaDb>>,
}

impl Service {
    /// Build daemon from default registry file
    pub async fn new() -> Result<(Self, watch::Receiver<bool>)> {
        let registry = Registry::load().context("Failed to load project registry")?;
        Self::from_registry(registry).await
    }

    /// Build daemon from an in-memory registry (test entry point)
    pub async fn from_registry(registry: Registry) -> Result<(Self, watch::Receiver<bool>)> {
        if registry.enabled_names().is_empty() {
            anyhow::bail!(
                "No enabled projects in registry. Add one with 'magellan service register --root <path>'"
            );
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (batch_tx, batch_rx) = mpsc::channel::<TaggedBatch>(1024);
        let worker_shutdown = Arc::new(AtomicBool::new(false));

        // Build immutable project map for worker to avoid async Mutex in blocking thread
        let mut project_map: HashMap<String, (PathBuf, PathBuf)> = HashMap::new();
        for entry in registry.list().iter().filter(|e| e.enabled) {
            project_map.insert(entry.name.clone(), (entry.root.clone(), entry.db.clone()));
        }
        let project_map = Arc::new(project_map);

        let reg = Arc::new(tokio::sync::Mutex::new(registry));
        let meta_db = meta_db::MetaDb::open().context("Failed to open meta.db")?;
        let meta_db = Arc::new(tokio::sync::Mutex::new(meta_db));

        // Populate meta.db from registry entries
        {
            let lock = reg.lock().await;
            let mut meta = meta_db.lock().await;
            for entry in lock.list().iter() {
                if let Err(e) = meta.upsert_project(
                    &entry.name,
                    &entry.root.to_string_lossy(),
                    &entry.db.to_string_lossy(),
                    entry.enabled,
                ) {
                    tracing::warn!(project = %entry.name, error = %e, "meta.db upsert error");
                }
            }
        }

        // Spawn worker in blocking task (CodeGraph is not Send)
        let worker_shutdown_clone = worker_shutdown.clone();
        let global_db = Registry::canonical_db_path("global");
        let meta_db_path = Some(meta_db::MetaDb::default_path());
        tokio::task::spawn_blocking(move || {
            worker_loop(
                batch_rx,
                worker_shutdown_clone,
                global_db,
                project_map,
                meta_db_path,
            );
        });

        // Start per-project watchers
        {
            let lock = reg.lock().await;
            let entries: Vec<types::ProjectEntry> =
                lock.list().iter().filter(|e| e.enabled).cloned().collect();
            drop(lock);

            for entry in entries {
                let tx = batch_tx.clone();
                let shutdown = shutdown_rx.clone();
                let root = entry.root;
                let name = entry.name;
                let _handle = tokio::spawn(watcher_task(root, name, shutdown, tx));
            }
        }

        let svc = Self {
            registry: reg,
            shutdown: shutdown_tx,
            batch_tx,
            worker_shutdown,
            meta_db,
        };

        Ok((svc, shutdown_rx))
    }

    /// Graceful shutdown
    pub fn shutdown(&self) {
        self.worker_shutdown.store(true, Ordering::SeqCst);
        let _ = self.shutdown.send(true);
    }

    /// Run the main event loop: signal handler + admin socket
    pub async fn run(self) -> Result<()> {
        let socket = Arc::new(self.setup_socket().await?);

        // Signal handler task
        let shutdown_tx = self.shutdown.clone();
        let worker_shutdown = self.worker_shutdown.clone();
        tokio::task::spawn_blocking(move || {
            let mut signals = Signals::new([signal_hook::consts::SIGINT, SIGTERM])
                .expect("Failed to register signal handler");
            if let Some(_sig) = signals.forever().next() {
                worker_shutdown.store(true, Ordering::SeqCst);
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
                    let meta = self.meta_db.clone();
                    let tx = self.batch_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = AdminSocket::handle_client(
                            stream, reg, meta, tx, None, None,
                        ).await {
                            tracing::error!(error = %e, "Admin socket handler error");
                        }
                    });
                }
            }
        }

        self.cleanup().await;
        Ok(())
    }

    async fn setup_socket(&self) -> Result<UnixListener> {
        let path = PathBuf::from(socket_path());
        let _ = tokio::fs::remove_file(&path).await;
        let listener = UnixListener::bind(&path)
            .with_context(|| format!("Failed to bind admin socket at {}", path.display()))?;
        Ok(listener)
    }

    async fn cleanup(&self) {
        let _ = tokio::fs::remove_file(socket_path()).await;
    }
}

/// Worker loop: receives TaggedBatch and dispatches to indexer via CodeGraph
fn worker_loop(
    mut rx: mpsc::Receiver<TaggedBatch>,
    shutdown: Arc<AtomicBool>,
    _global_db: PathBuf,
    project_map: Arc<HashMap<String, (PathBuf, PathBuf)>>,
    meta_db_path: Option<PathBuf>,
) {
    let mut open_graphs: HashMap<String, magellan::CodeGraph> = HashMap::new();

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match rx.blocking_recv() {
            Some(batch) => {
                let path_count = batch.paths.len();
                tracing::info!(
                    project = %batch.project_name,
                    paths = path_count,
                    "Batch received"
                );

                let (root, db) = project_map
                    .get(&batch.project_name)
                    .cloned()
                    .unwrap_or_else(|| {
                        let db = Registry::canonical_db_path(&batch.project_name);
                        (PathBuf::new(), db)
                    });

                let graph = match open_graphs.get_mut(&batch.project_name) {
                    Some(g) => g,
                    None => match magellan::CodeGraph::open(&db) {
                        Ok(g) => {
                            open_graphs.insert(batch.project_name.clone(), g);
                            open_graphs
                                .get_mut(&batch.project_name)
                                .expect("invariant: just inserted project into open_graphs")
                        }
                        Err(err) => {
                            tracing::error!(
                                db = %db.display(),
                                project = %batch.project_name,
                                error = %err,
                                "Failed to open DB for project"
                            );
                            continue;
                        }
                    },
                };

                let mut reconcile_errors: Vec<String> = Vec::new();
                for raw_path in &batch.paths {
                    let path = if raw_path.is_absolute() {
                        raw_path.clone()
                    } else {
                        root.join(raw_path)
                    };
                    let path_key = magellan::normalize_path(&path)
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());

                    if let Err(e) = graph.reconcile_file_path(&path, &path_key) {
                        tracing::warn!(
                            path = %path.display(),
                            project = %batch.project_name,
                            error = %e,
                            "Reconcile error"
                        );
                        reconcile_errors.push(format!("{}: {}", path.display(), e));
                        if let Some(ref meta_path) = meta_db_path {
                            if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                                let mut ev = make_event("reconcile_err", Some(&batch.project_name));
                                ev.file_path = Some(path.to_string_lossy().to_string());
                                ev.details = Some(serde_json::json!({ "error": e.to_string() }));
                                let _ = meta.log_event(&ev);
                                let _ = meta.close();
                            }
                        }
                    }
                }

                if reconcile_errors.is_empty() {
                    if let Some(ref meta_path) = meta_db_path {
                        if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                            let mut ev = make_event("reconcile_ok", Some(&batch.project_name));
                            ev.details = Some(serde_json::json!({"paths": path_count}));
                            let _ = meta.log_event(&ev);
                            let _ = meta.close();
                        }
                    }
                }

                if let Err(e) = graph.checkpoint_wal() {
                    tracing::error!(
                        project = %batch.project_name,
                        error = %e,
                        "WAL checkpoint failed"
                    );
                    if let Some(ref meta_path) = meta_db_path {
                        if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                            let ev = make_event("checkpoint_err", Some(&batch.project_name));
                            let mut ev = ev;
                            ev.details = Some(serde_json::json!({"error": e.to_string()}));
                            let _ = meta.log_event(&ev);
                            let _ = meta.close();
                        }
                    }
                } else {
                    if let Some(ref meta_path) = meta_db_path {
                        if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                            let ev = make_event("checkpoint_ok", Some(&batch.project_name));
                            let _ = meta.log_event(&ev);
                            let _ = meta.close();
                        }
                    }
                }

                // Update meta.db last_reindexed + log events
                if let Some(ref meta_path) = meta_db_path {
                    if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                        if meta
                            .get_project(&batch.project_name)
                            .unwrap_or(None)
                            .is_none()
                        {
                            if let Some((root, db)) = project_map.get(&batch.project_name) {
                                let _ = meta.upsert_project(
                                    &batch.project_name,
                                    &root.to_string_lossy(),
                                    &db.to_string_lossy(),
                                    true,
                                );
                            }
                        }
                        let _ = meta.update_last_reindexed(&batch.project_name);
                        let mut ev = make_event("batch_received", Some(&batch.project_name));
                        ev.details = Some(serde_json::json!({ "paths": path_count }));
                        let _ = meta.log_event(&ev);
                        let _ = meta.close();
                    }
                }
            }
            None => break,
        }
    }

    // Flush remaining open graphs
    for (name, graph) in open_graphs {
        if let Err(e) = graph.checkpoint_wal() {
            tracing::error!(project = %name, error = %e, "WAL checkpoint failed on shutdown");
        }
    }
}

/// Per-project watcher task: bridges synchronous FileSystemWatcher to async batch_tx
async fn watcher_task(
    root: PathBuf,
    project_name: String,
    mut shutdown: watch::Receiver<bool>,
    batch_tx: mpsc::Sender<TaggedBatch>,
) {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let flag = shutdown_flag.clone();

    // Bridge from blocking watcher thread to async task
    let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::channel::<magellan::WatcherBatch>(16);

    let name_for_blocking = project_name.clone();
    let root_display = root.display().to_string();

    // Spawn the actual blocking filesystem watcher
    let _task = tokio::task::spawn_blocking(move || {
        let cfg = WatcherConfig {
            root_path: root.clone(),
            ..Default::default()
        };
        let fw = match FileSystemWatcher::new(root, cfg, flag.clone()) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(project = %name_for_blocking, error = %e, "Failed to start watcher");
                return;
            }
        };
        tracing::info!(project = %name_for_blocking, root = %root_display, "Watcher started");
        loop {
            match fw.recv_batch_timeout(Duration::from_millis(500)) {
                Ok(Some(batch)) => {
                    if bridge_tx.blocking_send(batch).is_err() {
                        break; // Bridge receiver dropped
                    }
                }
                Ok(None) => break, // All senders dropped
                Err(_) => {
                    if flag.load(Ordering::SeqCst) {
                        break;
                    }
                    // Otherwise just continue polling
                }
            }
        }
    });

    // Forward from bridge to batch_tx while watching shutdown
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                shutdown_flag.store(true, Ordering::SeqCst);
                break;
            }
            Some(batch) = bridge_rx.recv() => {
                let paths = batch.paths;
                let _ = batch_tx
                    .send(TaggedBatch {
                        project_name: project_name.clone(),
                        paths,
                    })
                    .await;
            }
        }
    }
}

/// Send a JSON-RPC request to the daemon via unix socket and return the response
pub async fn send_request(req: serde_json::Value) -> Result<serde_json::Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path())
        .await
        .context("Daemon does not appear to be running (socket not found)")?;

    let req_line = serde_json::to_string(&req)? + "\n";
    stream.write_all(req_line.as_bytes()).await?;
    stream
        .shutdown()
        .await
        .context("Failed to shutdown write half")?;

    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .context("Failed to read daemon response")?;

    let resp: serde_json::Value =
        serde_json::from_str(&line).context("Failed to parse daemon response")?;
    Ok(resp)
}

/// Synchronous probe: check if daemon socket exists and responds to ping.
/// Returns `true` only if the socket file exists AND a ping request receives
/// a response containing "pong" within 200 ms.
pub fn is_daemon_running() -> bool {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = std::path::PathBuf::from(socket_path());
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
    use super::*;
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

        let meta_ping = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at("/tmp/magellan_test_ping_meta.db").unwrap(),
        ));

        // Spawn accept loop
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_ping.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16); // dummy sender for ping test
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
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
        let _ = tokio::fs::remove_file("/tmp/magellan_test_ping_meta.db").await;
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
        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(socket.with_extension("meta_watch.db")).unwrap(),
        ));

        // Spawn accept loop
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_db.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        // Give listener time to bind
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and send watch request
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-watch-1","method":"watch","tag":"alpha","paths":["/tmp/roots/alpha/src/main.rs"]}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        // Await acknowledgment line
        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.unwrap();
        assert!(n > 0, "expected acknowledgment");
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["result"]["queued"].as_str(), Some("alpha"));

        // Worker queue MUST receive the TaggedBatch
        let batch = tokio::time::timeout(tokio::time::Duration::from_secs(2), rx.recv())
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

    // P0-15: WatcherReactor — watcher_task must emit TaggedBatch on file changes
    #[tokio::test]
    async fn test_watcher_task_emits_batch_on_file_change() {
        use std::fs::write;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let file1 = root.join("test.rs");
        write(&file1, "// initial").unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Spawn watcher task
        let _join = tokio::spawn(super::watcher_task(
            root.clone(),
            "testproj".to_string(),
            shutdown_rx,
            tx,
        ));

        // Give watcher time to start
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Create another file to trigger a watch event
        let file2 = root.join("new.rs");
        write(&file2, "// new file").unwrap();

        // Wait for batch (debounce + poll interval = up to ~1.5s normally)
        let batch = tokio::time::timeout(tokio::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out waiting for watcher batch")
            .expect("batch channel closed");

        assert_eq!(batch.project_name, "testproj");
        // Should contain at least one of the files from temp dir
        assert!(!batch.paths.is_empty(), "batch should contain dirty paths");
    }

    // P1.5: Worker Loop -> Pipeline Dispatch — indexing via reconcile_file_path
    #[tokio::test]
    async fn test_worker_loop_indexes_file_and_reconciles() {
        use std::fs::write;

        let dir = tempfile::tempdir().unwrap();
        let db_path = Registry::ensure_db_dir("test_p15").unwrap();
        let root = dir.path().to_path_buf();

        // Create a minimal Rust file for indexing
        let src_file = root.join("main.rs");
        write(&src_file, r#"pub fn hello() -> &'static str { "world" }"#).unwrap();

        // Build a batch pointing at the file
        let batch = TaggedBatch {
            project_name: "test_p15".to_string(),
            paths: vec![src_file.clone()],
        };

        // Set up the project map for the worker
        let mut project_map = std::collections::HashMap::new();
        project_map.insert("test_p15".to_string(), (root.clone(), db_path.clone()));
        let project_map = std::sync::Arc::new(project_map);

        // tokio::sync::mpsc required for worker_loop's blocking_recv()
        let (batch_tx, batch_rx) = tokio::sync::mpsc::channel::<TaggedBatch>(4);
        let _ = batch_tx.send(batch).await;
        drop(batch_tx); // signal end

        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Run worker_loop synchronously in blocking task (CodeGraph is notSend)
        let db_for_worker = db_path.clone();
        tokio::task::spawn_blocking(move || {
            worker_loop(batch_rx, shutdown, db_for_worker, project_map, None);
        })
        .await
        .unwrap();

        // Verify DB exists and the file was indexed (count_files > 0)
        let graph = magellan::CodeGraph::open(&db_path).unwrap();
        let file_count = graph.count_files().unwrap();
        assert!(
            file_count >= 1,
            "Expected at least 1 file in DB after reconcile, got {}",
            file_count
        );

        // Cleanup
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }
        let parent = db_path.parent().unwrap();
        if parent.exists() && parent != std::path::Path::new(".") {
            let _ = std::fs::remove_dir(parent);
        }
    }

    // Phase 2: meta.db update
    #[tokio::test]
    async fn test_worker_loop_updates_meta_db_last_reindexed() {
        use std::fs::write;

        let dir = tempfile::tempdir().unwrap();
        let db_path = Registry::ensure_db_dir("test_p15_meta").unwrap();
        let meta_db_path = dir.path().join("meta.db");
        let root = dir.path().to_path_buf();

        let src_file = root.join("main.rs");
        write(&src_file, r#"pub fn hello() -> &'static str { "world" }"#).unwrap();

        let batch = TaggedBatch {
            project_name: "test_p15_meta".to_string(),
            paths: vec![src_file.clone()],
        };

        let mut project_map = std::collections::HashMap::new();
        project_map.insert("test_p15_meta".to_string(), (root.clone(), db_path.clone()));
        let project_map = std::sync::Arc::new(project_map);

        let (batch_tx, batch_rx) = tokio::sync::mpsc::channel::<TaggedBatch>(4);
        let _ = batch_tx.send(batch).await;
        drop(batch_tx);

        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let db_for_worker = db_path.clone();
        let meta_for_worker = meta_db_path.clone();
        tokio::task::spawn_blocking(move || {
            worker_loop(
                batch_rx,
                shutdown,
                db_for_worker,
                project_map,
                Some(meta_for_worker),
            );
        })
        .await
        .unwrap();

        // Verify meta.db was created and last_reindexed updated
        let meta = meta_db::MetaDb::open_at(&meta_db_path).unwrap();
        let stats = meta.get_project("test_p15_meta").unwrap();
        assert!(
            stats.is_some(),
            "meta.db should contain project entry after reconcile"
        );
        assert!(
            stats.unwrap().last_reindexed.is_some(),
            "last_reindexed should be set"
        );

        // Cleanup
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }
        let parent = db_path.parent().unwrap();
        if parent.exists() && parent != std::path::Path::new(".") {
            let _ = std::fs::remove_dir(parent);
        }
    }

    // P2-5: service.stats JSON-RPC over admin socket
    #[tokio::test]
    async fn test_admin_socket_stats_returns_meta_db_projects() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_stats.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(PathBuf::from("/tmp/magellan_test_stats.toml"))
                .unwrap(),
        ));

        // Pre-seed meta.db with a test project
        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at("/tmp/magellan_test_stats_meta.db").unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project("omega", "/tmp/roots/omega", "/tmp/dbs/omega.db", true)
                .unwrap();
        }

        // Spawn accept loop
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16); // dummy for stats
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and request stats
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-stats-1","method":"stats","params":{}}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        // Assert top-level structure
        let projects = resp
            .get("result")
            .and_then(|r| r.get("projects"))
            .and_then(|p| p.as_array());
        assert!(
            projects.is_some(),
            "response should contain stats.projects array, got: {}",
            line
        );
        let projects = projects.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].get("name").and_then(|v| v.as_str()),
            Some("omega")
        );
        assert_eq!(
            projects[0].get("enabled").and_then(|v| v.as_bool()),
            Some(true)
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
        let _ = tokio::fs::remove_file("/tmp/magellan_test_stats_meta.db").await;
    }

    // Phase 3 RED: query.find — must succeed (empty result) instead of returning not_implemented
    #[tokio::test]
    async fn test_admin_socket_query_find_returns_empty_on_no_projects() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_query_find.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(PathBuf::from(
                "/tmp/magellan_test_query_find.toml",
            ))
            .unwrap(),
        ));

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at("/tmp/magellan_test_query_find_meta.db").unwrap(),
        ));

        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-qf-1","method":"query.find","name":"hello"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        // Must NOT be not_implemented
        let error = resp.get("error");
        assert!(
            error.is_none(),
            "query.find should be implemented, got error: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let matches = result.get("matches").and_then(|v| v.as_array());
        assert!(
            matches.is_some(),
            "expected result.matches array, got: {}",
            line
        );
        assert!(
            matches.unwrap().is_empty(),
            "expected empty matches with no projects"
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
        let _ = tokio::fs::remove_file("/tmp/magellan_test_query_find_meta.db").await;
    }

    // Phase 3 RED: query.find returns actual symbol from meta.db-registered project
    #[tokio::test]
    async fn test_admin_socket_query_find_returns_symbol_from_meta_db() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qf_real.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create a real indexed DB in blocking task (CodeGraph is !Send)
        let db_path_clone = temp_path.clone();
        let db_path = tokio::task::spawn_blocking(move || {
            let db_path = db_path_clone.join("real_proj.db");
            std::fs::create_dir_all(db_path_clone.join("src")).unwrap();
            let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
            graph
                .index_file(
                    "src/lib.rs",
                    b"fn greet() { hello() }\nfn hello() { println!(\"hi\") }\n",
                )
                .unwrap();
            graph.checkpoint_wal().unwrap();
            db_path
        })
        .await
        .unwrap();

        // Verify DB was created and the graph has symbols before admin socket query
        let verify_path = db_path.clone();
        let symbols = tokio::task::spawn_blocking(move || {
            let graph = magellan::CodeGraph::open(&verify_path).unwrap();
            graph.count_symbols().unwrap_or(0)
        })
        .await
        .unwrap();
        assert!(symbols > 0, "expected indexed symbols in DB");

        // Pre-seed registry + meta.db with this project
        let reg_path = temp_dir.path().join("registry.toml");
        let mut reg = super::registry::Registry::load_from(reg_path.clone()).unwrap();
        reg.register(super::types::ProjectEntry::new(
            "real_proj".to_string(),
            temp_path.clone(),
            db_path.clone(),
            "test".to_string(),
        ))
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "real_proj",
                &temp_path.to_string_lossy(),
                &db_path.to_string_lossy(),
                true,
            )
            .unwrap();
            let projects = meta.list_projects().unwrap();
            assert!(
                !projects.is_empty(),
                "meta_db should have real_proj after upsert"
            );
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(reg));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-qf-real","method":"query.find","name":"greet"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        let error = resp.get("error");
        assert!(
            error.is_none(),
            "query.find should succeed, got error: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let matches = result
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches array missing");
        assert!(
            !matches.is_empty(),
            "expected at least 1 match for 'greet', got empty array"
        );
        let first = &matches[0];
        assert_eq!(
            first.get("name").and_then(|v| v.as_str()),
            Some("greet"),
            "first match should be name=greet"
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
    }

    // P3-CTX RED: query.context returns symbol with callers/callees
    #[tokio::test]
    async fn test_admin_socket_query_context_returns_symbol() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qctx.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let db_path_clone = temp_path.clone();
        let db_path = tokio::task::spawn_blocking(move || {
            let db_path = db_path_clone.join("ctx_proj.db");
            let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
            graph
                .index_file(
                    "src/lib.rs",
                    b"fn greet() { hello() }\nfn hello() { println!(\"hi\") }\n",
                )
                .unwrap();
            graph.checkpoint_wal().unwrap();
            db_path
        })
        .await
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("ctx_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "ctx_proj",
                &temp_path.to_string_lossy(),
                &db_path.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_dir.path().join("ctx_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-qctx-1","method":"query.context","name":"greet","callers":true,"callees":true}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        // Must NOT return not_implemented
        let error = resp.get("error");
        assert!(
            error.is_none(),
            "query.context should be implemented, got error: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let matches = result
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches array missing");
        assert!(
            !matches.is_empty(),
            "expected at least 1 match for 'greet', got empty array"
        );
        let first = &matches[0];
        assert_eq!(
            first.get("name").and_then(|v| v.as_str()),
            Some("greet"),
            "first match name should be greet"
        );
        // callers field must be present (may be empty list)
        assert!(
            first.get("callers").is_some(),
            "callers field must be present in context response"
        );
        // callees field must be present
        assert!(
            first.get("callees").is_some(),
            "callees field must be present in context response"
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    // P3-CMP RED: query.compare returns per-project symbol side-by-side
    #[tokio::test]
    async fn test_admin_socket_query_compare_returns_per_project() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qcmp.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create two DBs — both have "greet", each in a different file
        let tp = temp_path.clone();
        let (db_a, db_b) = tokio::task::spawn_blocking(move || {
            let db_a = tp.join("cmp_a.db");
            let db_b = tp.join("cmp_b.db");
            let mut g = magellan::CodeGraph::open(&db_a).unwrap();
            g.index_file("src/a.rs", b"fn greet() { println!(\"a\") }\n")
                .unwrap();
            g.checkpoint_wal().unwrap();
            let mut g = magellan::CodeGraph::open(&db_b).unwrap();
            g.index_file("src/b.rs", b"fn greet() { println!(\"b\") }\n")
                .unwrap();
            g.checkpoint_wal().unwrap();
            (db_a, db_b)
        })
        .await
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("cmp_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "cmp_a",
                &temp_path.to_string_lossy(),
                &db_a.to_string_lossy(),
                true,
            )
            .unwrap();
            meta.upsert_project(
                "cmp_b",
                &temp_path.to_string_lossy(),
                &db_b.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_dir.path().join("cmp_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        // Compare "greet" across cmp_a and cmp_b
        let req = r#"{"id":"test-qcmp-1","method":"query.compare","name":"greet","projects":["cmp_a","cmp_b"]}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        // Must NOT return not_implemented
        assert!(
            resp.get("error").is_none(),
            "query.compare should be implemented, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let comparisons = result
            .get("comparisons")
            .and_then(|v| v.as_array())
            .expect("comparisons array missing");
        assert_eq!(
            comparisons.len(),
            2,
            "expected 2 per-project entries, got: {}",
            line
        );
        // Each entry has project + name + file_path
        for entry in comparisons {
            assert!(entry.get("project").is_some(), "project field missing");
            assert!(entry.get("name").is_some(), "name field missing");
            assert!(entry.get("file_path").is_some(), "file_path field missing");
        }

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_query_build_index_returns_pairs_inserted() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qbuild.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Two projects with identical Rust structure so cosine similarity = 1.0 ≥ 0.70
        let tp = temp_path.clone();
        let (db_a, db_b) = tokio::task::spawn_blocking(move || {
            let src_a = tp.join("bi_a.rs");
            let src_b = tp.join("bi_b.rs");
            let src = b"fn greet() { if true { println!(\"hi\"); } }\n";
            std::fs::write(&src_a, src).unwrap();
            std::fs::write(&src_b, src).unwrap();

            let db_a = tp.join("bi_a.db");
            let db_b = tp.join("bi_b.db");
            let mut g = magellan::CodeGraph::open(&db_a).unwrap();
            g.index_file(src_a.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            let mut g = magellan::CodeGraph::open(&db_b).unwrap();
            g.index_file(src_b.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            (db_a, db_b)
        })
        .await
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("bi_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "bi_a",
                &temp_path.to_string_lossy(),
                &db_a.to_string_lossy(),
                true,
            )
            .unwrap();
            meta.upsert_project(
                "bi_b",
                &temp_path.to_string_lossy(),
                &db_b.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_dir.path().join("bi_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"test-qbuild-1","method":"query.build-index"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "query.build-index should succeed, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let pairs = result
            .get("pairs_inserted")
            .and_then(|v| v.as_u64())
            .expect("pairs_inserted field missing");
        assert!(pairs > 0, "expected ≥1 pair inserted, got {pairs}");

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_query_compare_includes_similarity_score() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qcmp_score.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Two projects with same structural shape — build-index should produce cross-ref pair
        let tp = temp_path.clone();
        let (db_a, db_b) = tokio::task::spawn_blocking(move || {
            let src = b"fn greet() { if true { println!(\"hi\"); } }\n";
            let src_a = tp.join("score_a.rs");
            let src_b = tp.join("score_b.rs");
            std::fs::write(&src_a, src).unwrap();
            std::fs::write(&src_b, src).unwrap();

            let db_a = tp.join("score_a.db");
            let db_b = tp.join("score_b.db");
            let mut g = magellan::CodeGraph::open(&db_a).unwrap();
            g.index_file(src_a.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            let mut g = magellan::CodeGraph::open(&db_b).unwrap();
            g.index_file(src_b.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            (db_a, db_b)
        })
        .await
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("score_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "score_a",
                &temp_path.to_string_lossy(),
                &db_a.to_string_lossy(),
                true,
            )
            .unwrap();
            meta.upsert_project(
                "score_b",
                &temp_path.to_string_lossy(),
                &db_b.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_dir.path().join("score_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Step 1: build the cross-ref index first
        let mut stream = UnixStream::connect(socket_path).await.unwrap();
        let req_build = r#"{"id":"score-build","method":"query.build-index"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req_build.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();
        let mut reader = tokio::io::BufReader::new(read_half);
        let mut _line = String::new();
        reader.read_line(&mut _line).await.unwrap(); // consume build response

        // Step 2: query.compare — should now include similarity_score per entry
        let mut stream = UnixStream::connect(socket_path).await.unwrap();
        let req = r#"{"id":"score-cmp","method":"query.compare","name":"greet","projects":["score_a","score_b"]}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "query.compare should succeed, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let comparisons = result
            .get("comparisons")
            .and_then(|v| v.as_array())
            .expect("comparisons array missing");
        assert_eq!(comparisons.len(), 2, "expected 2 comparison entries");

        // At least one entry should carry a similarity_score
        let has_score = comparisons
            .iter()
            .any(|e| e.get("similarity_score").is_some());
        assert!(
            has_score,
            "expected at least one entry with similarity_score"
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_query_suggest_returns_similar_symbols() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_qsuggest.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let tp = temp_path.clone();
        let (db_a, db_b) = tokio::task::spawn_blocking(move || {
            let src = b"fn greet() { if true { println!(\"hi\"); } }\n";
            let src_a = tp.join("sug_a.rs");
            let src_b = tp.join("sug_b.rs");
            std::fs::write(&src_a, src).unwrap();
            std::fs::write(&src_b, src).unwrap();
            let db_a = tp.join("sug_a.db");
            let db_b = tp.join("sug_b.db");
            let mut g = magellan::CodeGraph::open(&db_a).unwrap();
            g.index_file(src_a.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            let mut g = magellan::CodeGraph::open(&db_b).unwrap();
            g.index_file(src_b.to_str().unwrap(), src).unwrap();
            g.checkpoint_wal().unwrap();
            (db_a, db_b)
        })
        .await
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_dir.path().join("sug_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "sug_a",
                &temp_path.to_string_lossy(),
                &db_a.to_string_lossy(),
                true,
            )
            .unwrap();
            meta.upsert_project(
                "sug_b",
                &temp_path.to_string_lossy(),
                &db_b.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_dir.path().join("sug_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Step 1: build the cross-ref index
        let mut stream = UnixStream::connect(socket_path).await.unwrap();
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all(b"{\"id\":\"sug-build\",\"method\":\"query.build-index\"}\n")
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();
        let mut reader = tokio::io::BufReader::new(read_half);
        let mut _line = String::new();
        reader.read_line(&mut _line).await.unwrap();

        // Step 2: query.suggest for "greet" from sug_a
        let mut stream = UnixStream::connect(socket_path).await.unwrap();
        let req =
            r#"{"id":"sug-1","method":"query.suggest","from_project":"sug_a","name":"greet"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "query.suggest should be implemented, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let suggestions = result
            .get("suggestions")
            .and_then(|v| v.as_array())
            .expect("suggestions array missing");
        assert!(
            !suggestions.is_empty(),
            "expected ≥1 suggestion after build-index"
        );

        let first = &suggestions[0];
        assert!(first.get("project").is_some(), "project field missing");
        assert!(first.get("symbol").is_some(), "symbol field missing");
        assert!(
            first.get("similarity_score").is_some(),
            "score field missing"
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_analyze_returns_hotspots() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_evol.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create a shard with symbol_metrics
        let db_a = temp_path.join("evol_shard.db");
        {
            let conn = rusqlite::Connection::open(&db_a).unwrap();
            conn.execute(
                "CREATE TABLE symbol_metrics (
                    symbol_id INTEGER PRIMARY KEY,
                    symbol_name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    loc INTEGER DEFAULT 0,
                    estimated_loc REAL DEFAULT 0,
                    fan_in INTEGER DEFAULT 0,
                    fan_out INTEGER DEFAULT 0,
                    cyclomatic_complexity INTEGER DEFAULT 0,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO symbol_metrics
                 (symbol_name, kind, file_path, loc, fan_in, cyclomatic_complexity, last_updated)
                 VALUES ('heavy', 'fn', 'src/big.rs', 200, 30, 8, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO symbol_metrics
                 (symbol_name, kind, file_path, loc, fan_in, cyclomatic_complexity, last_updated)
                 VALUES ('light', 'fn', 'src/small.rs', 10, 2, 1, 0)",
                [],
            )
            .unwrap();
        }

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("evol_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "evol_a",
                &temp_path.to_string_lossy(),
                &db_a.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("evol_reg.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req =
            r#"{"id":"evol-1","method":"evolve.analyze","params":{"project":"evol_a","limit":2}}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "evolve.analyze should succeed, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let candidates = result
            .get("candidates")
            .and_then(|v| v.as_array())
            .expect("candidates array missing");
        assert_eq!(candidates.len(), 2, "expected 2 hotspot candidates");

        let first = &candidates[0];
        assert_eq!(first.get("symbol").and_then(|v| v.as_str()), Some("heavy"));
        assert_eq!(
            first.get("project").and_then(|v| v.as_str()),
            Some("evol_a")
        );
        assert!(first.get("rank_score").and_then(|v| v.as_f64()).unwrap() > 0.0);

        let second = &candidates[1];
        assert_eq!(second.get("symbol").and_then(|v| v.as_str()), Some("light"));

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_retrieve_returns_analogues() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_retrieve.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("meta_r.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.insert_cross_ref("proj_a", "sym_a", "a.rs", "proj_b", "sym_b", "b.rs", 0.91)
                .unwrap();
            meta.insert_cross_ref("proj_a", "sym_a", "a.rs", "proj_c", "sym_c", "c.rs", 0.82)
                .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("reg_r.toml")).unwrap(),
        ));

        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-r-1","method":"evolve.retrieve","project":"proj_a","symbol":"sym_a","limit":1}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "evolve.retrieve should succeed, got: {}",
            line
        );

        let result = resp.get("result").expect("result missing");
        let analogues = result
            .get("analogues")
            .and_then(|v| v.as_array())
            .expect("analogues array missing");
        assert_eq!(analogues.len(), 1, "expected 1 analogue due to limit=1");

        let first = &analogues[0];
        assert_eq!(first.get("symbol").and_then(|v| v.as_str()), Some("sym_b"));
        assert_eq!(
            first.get("similarity_score").and_then(|v| v.as_f64()),
            Some(0.91)
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_propose_persists_candidate() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_propose.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("meta_p.db")).unwrap(),
        ));

        let shard_db = temp_path.join("proj_alpha.db");
        {
            let conn = rusqlite::Connection::open(&shard_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS candidate_facts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    candidate_id TEXT UNIQUE NOT NULL,
                    source_document_id INTEGER NOT NULL DEFAULT 0,
                    subject_type TEXT NOT NULL,
                    subject_key TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object_type TEXT,
                    object_key TEXT,
                    properties_json TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    rejection_reason TEXT,
                    created_at INTEGER,
                    reviewed_at INTEGER
                );",
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("reg_p.toml")).unwrap(),
        ));
        {
            let mut r = reg.lock().await;
            let entry = super::types::ProjectEntry::new(
                "proj_alpha".to_string(),
                temp_path.clone(),
                shard_db.clone(),
                "manual".to_string(),
            );
            r.register(entry).unwrap();
        }

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_clone = reg.clone();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg_clone.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-p-1","method":"evolve.propose","project":"proj_alpha","symbol":"sym_x","candidate_id":"c-42","patch_diff":"@@ -1 +1 @@\n-foo\n+bar\n","analogue":{"project":"proj_beta","symbol":"sym_y"}}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "evolve.propose should succeed, got: {}",
            line
        );
        let result = resp.get("result").expect("result missing");
        assert_eq!(
            result.get("candidate_id").and_then(|v| v.as_str()),
            Some("c-42")
        );
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("pending")
        );

        // Verify persisted in project DB
        let recs = super::candidates::list_candidates(&shard_db, None, None).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].candidate_id, "c-42");
        assert!(recs[0].properties_json.contains("patch_diff"));

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_candidates_list_returns_items() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_candidates.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("meta_c.db")).unwrap(),
        ));

        let shard_db = temp_path.join("proj_gamma.db");
        {
            let conn = rusqlite::Connection::open(&shard_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS candidate_facts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    candidate_id TEXT UNIQUE NOT NULL,
                    source_document_id INTEGER NOT NULL DEFAULT 0,
                    subject_type TEXT NOT NULL,
                    subject_key TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object_type TEXT,
                    object_key TEXT,
                    properties_json TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    rejection_reason TEXT,
                    created_at INTEGER,
                    reviewed_at INTEGER
                );",
            )
            .unwrap();
        }

        // Seed one record directly
        super::candidates::insert_candidate_fact(
            &shard_db,
            "seed-1",
            "Symbol",
            "sym_seed",
            "proposes-improvement",
            r#"{"patch_diff":"dummy"}"#,
            "pending",
        )
        .unwrap();

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("reg_c.toml")).unwrap(),
        ));
        {
            let mut r = reg.lock().await;
            let entry = super::types::ProjectEntry::new(
                "proj_gamma".to_string(),
                temp_path.clone(),
                shard_db.clone(),
                "manual".to_string(),
            );
            r.register(entry).unwrap();
        }

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_clone = reg.clone();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg_clone.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-c-1","method":"evolve.candidates","project":"proj_gamma","status":"pending","limit":5}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "evolve.candidates should succeed, got: {}",
            line
        );
        let result = resp.get("result").expect("result missing");
        let items = result
            .get("candidates")
            .and_then(|v| v.as_array())
            .expect("candidates array missing");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("candidate_id").and_then(|v| v.as_str()),
            Some("seed-1")
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_promote_and_reject() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_promote.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("meta_pr.db")).unwrap(),
        ));

        let shard_db = temp_path.join("proj_epsilon.db");
        {
            let conn = rusqlite::Connection::open(&shard_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS candidate_facts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    candidate_id TEXT UNIQUE NOT NULL,
                    source_document_id INTEGER NOT NULL DEFAULT 0,
                    subject_type TEXT NOT NULL,
                    subject_key TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object_type TEXT,
                    object_key TEXT,
                    properties_json TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    rejection_reason TEXT,
                    created_at INTEGER,
                    reviewed_at INTEGER
                );",
            )
            .unwrap();
        }

        super::candidates::insert_candidate_fact(
            &shard_db,
            "promo-1",
            "Symbol",
            "sym_e",
            "proposes-improvement",
            r#"{"patch_diff":"test"}"#,
            "pending",
        )
        .unwrap();

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("reg_pr.toml")).unwrap(),
        ));
        {
            let mut r = reg.lock().await;
            let entry = super::types::ProjectEntry::new(
                "proj_epsilon".to_string(),
                temp_path.clone(),
                shard_db.clone(),
                "manual".to_string(),
            );
            r.register(entry).unwrap();
        }

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_clone = reg.clone();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg_clone.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Promote
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-pr-1","method":"evolve.promote","project":"proj_epsilon","candidate_id":"promo-1"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(
            resp.get("error").is_none(),
            "evolve.promote should succeed, got: {}",
            line
        );
        let result = resp.get("result").expect("result missing");
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("promoted")
        );

        // Reject (with reason)
        // seed another first because promo-1 already promoted
        super::candidates::insert_candidate_fact(
            &shard_db,
            "promo-2",
            "Symbol",
            "sym_f",
            "proposes-improvement",
            r#"{"patch_diff":"bad"}"#,
            "pending",
        )
        .unwrap();

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-rj-1","method":"evolve.reject","project":"proj_epsilon","candidate_id":"promo-2","rejection_reason":"test fails"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(
            resp.get("error").is_none(),
            "evolve.reject should succeed, got: {}",
            line
        );
        let result = resp.get("result").expect("result missing");
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("rejected")
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    #[tokio::test]
    async fn test_admin_socket_evolve_verify_applies_patch_and_runs_tests() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_verify.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create a minimal Rust project
        std::fs::create_dir(temp_path.join("src")).unwrap();
        std::fs::write(
            temp_path.join("Cargo.toml"),
            r#"[package]
name = "dummy_proj"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(
            temp_path.join("src/lib.rs"),
            "pub fn hello() -> &'static str { \"world\" }\n",
        )
        .unwrap();

        let shard_db = temp_path.join("proj_verify.db");
        {
            let conn = rusqlite::Connection::open(&shard_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS candidate_facts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    candidate_id TEXT UNIQUE NOT NULL,
                    source_document_id INTEGER NOT NULL DEFAULT 0,
                    subject_type TEXT NOT NULL,
                    subject_key TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object_type TEXT,
                    object_key TEXT,
                    properties_json TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    rejection_reason TEXT,
                    created_at INTEGER,
                    reviewed_at INTEGER
                );",
            )
            .unwrap();
        }

        // Patch using actual relative paths (no a/ b/ prefixes)
        let patch = r#"--- src/lib.rs
+++ src/lib.rs
@@ -1 +1 @@
-pub fn hello() -> &'static str { "world" }
+pub fn hello() -> &'static str { "world" } // patched
"#;
        let properties = serde_json::json!({"patch_diff": patch}).to_string();
        super::candidates::insert_candidate_fact(
            &shard_db,
            "verify-1",
            "Symbol",
            "hello",
            "proposes-improvement",
            &properties,
            "pending",
        )
        .unwrap();

        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("meta_v.db")).unwrap(),
        ));
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("reg_v.toml")).unwrap(),
        ));
        {
            let mut r = reg.lock().await;
            let entry = super::types::ProjectEntry::new(
                "proj_verify".to_string(),
                temp_path.clone(),
                shard_db.clone(),
                "manual".to_string(),
            );
            r.register(entry).unwrap();
        }

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_clone = reg.clone();
        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg_clone.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"evol-v-1","method":"evolve.verify","project":"proj_verify","candidate_id":"verify-1"}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "evolve.verify should succeed, got: {}",
            line
        );
        let result = resp.get("result").expect("result missing");
        let status = result.get("status").and_then(|v| v.as_str());
        let stdout = result.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
        let stderr = result.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            status == Some("verified"),
            "Expected verified, got {:?}. stdout: {}\nstderr: {}",
            status,
            stdout,
            stderr
        );
        assert_eq!(result.get("passed").and_then(|v| v.as_bool()), Some(true));

        // Check candidate status updated in DB
        let rec = super::candidates::get_candidate_by_id(&shard_db, "verify-1")
            .unwrap()
            .expect("candidate should exist");
        assert_eq!(rec.status, "verified");

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    // ------------------------------------------------------------------
    // End-to-end P5 evolution chain: analyze -> propose -> verify -> promote
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_admin_socket_evolve_end_to_end_chain() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_e2e_chain.sock";
        let _ = tokio::fs::remove_file(socket_path).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // --- 1. Create a minimal Rust project for verify() ---
        std::fs::create_dir(temp_path.join("src")).unwrap();
        std::fs::write(
            temp_path.join("Cargo.toml"),
            r#"[package]
name = "e2e_proj"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(
            temp_path.join("src/lib.rs"),
            "pub fn greet() -> &'static str { \"hello\" }\n",
        )
        .unwrap();

        // --- 2. Shard DB with symbol_metrics (for analyze) + candidate_facts (for chain) ---
        let shard_db = temp_path.join("e2e_shard.db");
        {
            let conn = rusqlite::Connection::open(&shard_db).unwrap();
            conn.execute(
                "CREATE TABLE symbol_metrics (
                    symbol_id INTEGER PRIMARY KEY,
                    symbol_name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    loc INTEGER DEFAULT 0,
                    estimated_loc REAL DEFAULT 0,
                    fan_in INTEGER DEFAULT 0,
                    fan_out INTEGER DEFAULT 0,
                    cyclomatic_complexity INTEGER DEFAULT 0,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO symbol_metrics
                 (symbol_name, kind, file_path, loc, fan_in, cyclomatic_complexity, last_updated)
                 VALUES ('greet', 'fn', 'src/lib.rs', 1, 0, 1, 0)",
                [],
            )
            .unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS candidate_facts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    candidate_id TEXT UNIQUE NOT NULL,
                    source_document_id INTEGER NOT NULL DEFAULT 0,
                    subject_type TEXT NOT NULL,
                    subject_key TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object_type TEXT,
                    object_key TEXT,
                    properties_json TEXT,
                    status TEXT NOT NULL DEFAULT 'pending',
                    rejection_reason TEXT,
                    created_at INTEGER,
                    reviewed_at INTEGER
                );",
            )
            .unwrap();
        }

        // --- 3. Meta DB + Registry ---
        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(temp_path.join("e2e_meta.db")).unwrap(),
        ));
        {
            let mut meta = meta_db.lock().await;
            meta.upsert_project(
                "e2e_proj",
                &temp_path.to_string_lossy(),
                &shard_db.to_string_lossy(),
                true,
            )
            .unwrap();
        }

        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(temp_path.join("e2e_reg.toml")).unwrap(),
        ));
        {
            let mut r = reg.lock().await;
            let entry = super::types::ProjectEntry::new(
                "e2e_proj".to_string(),
                temp_path.clone(),
                shard_db.clone(),
                "manual".to_string(),
            );
            r.register(entry).unwrap();
        }

        // --- 4. Spawn socket ---
        let listener = UnixListener::bind(socket_path).unwrap();
        let meta_clone = meta_db.clone();
        let reg_clone = reg.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let meta = meta_clone.clone();
                let reg = reg_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Helper: send one-shot JSON-RPC and return response
        async fn rpc_call(socket_path: &str, req_json: &str) -> serde_json::Value {
            let mut stream = UnixStream::connect(socket_path).await.expect("connect");
            let (read_half, mut write_half) = stream.split();
            write_half
                .write_all((req_json.to_string() + "\n").as_bytes())
                .await
                .unwrap();
            write_half.shutdown().await.unwrap();

            let mut reader = tokio::io::BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            serde_json::from_str(&line).unwrap()
        }

        // --- 5. Step A: analyze -> get hotspot candidates ---
        let resp_a = rpc_call(
            socket_path,
            r#"{"id":"e2e-1","method":"evolve.analyze","project":"e2e_proj","limit":5}"#,
        )
        .await;
        assert!(
            resp_a.get("error").is_none(),
            "analyze failed: {}",
            serde_json::to_string(&resp_a).unwrap()
        );
        let result_a = resp_a.get("result").expect("result missing");
        let candidates = result_a
            .get("candidates")
            .and_then(|v| v.as_array())
            .expect("candidates array missing");
        assert!(
            !candidates.is_empty(),
            "analyze should return at least 1 hotspot"
        );
        let top_symbol = candidates[0]
            .get("symbol")
            .and_then(|v| v.as_str())
            .expect("symbol missing");
        assert_eq!(top_symbol, "greet");

        // --- 6. Step B: propose -> create candidate from the hotspot ---
        let patch = r#"--- src/lib.rs
+++ src/lib.rs
@@ -1 +1 @@
-pub fn greet() -> &'static str { "hello" }
+pub fn greet() -> &'static str { "hello" } // evolved
"#;
        let req_propose_val = serde_json::json!({
            "id": "e2e-2",
            "method": "evolve.propose",
            "project": "e2e_proj",
            "symbol": top_symbol,
            "patch_diff": patch,
            "analogue": { "project": "other", "symbol": "other_sym" }
        });
        let resp_p = rpc_call(socket_path, &req_propose_val.to_string()).await;
        assert!(
            resp_p.get("error").is_none(),
            "propose failed: {}",
            serde_json::to_string(&resp_p).unwrap()
        );
        let result_p = resp_p.get("result").expect("result missing");
        let candidate_id = result_p
            .get("candidate_id")
            .and_then(|v| v.as_str())
            .expect("candidate_id missing");
        assert_eq!(
            result_p.get("status").and_then(|v| v.as_str()),
            Some("pending")
        );

        // --- 7. Step C: verify -> apply patch and run tests ---
        let req_verify = format!(
            r#"{{"id":"e2e-3","method":"evolve.verify","project":"e2e_proj","candidate_id":"{}"}}"#,
            candidate_id
        );
        let resp_v = rpc_call(socket_path, &req_verify).await;
        assert!(
            resp_v.get("error").is_none(),
            "verify failed: {}",
            serde_json::to_string(&resp_v).unwrap()
        );
        let result_v = resp_v.get("result").expect("result missing");
        assert_eq!(
            result_v.get("status").and_then(|v| v.as_str()),
            Some("verified")
        );
        assert_eq!(result_v.get("passed").and_then(|v| v.as_bool()), Some(true));

        // DB should reflect verified
        let rec_v = super::candidates::get_candidate_by_id(&shard_db, candidate_id)
            .unwrap()
            .expect("candidate should exist after verify");
        assert_eq!(rec_v.status, "verified");

        // --- 8. Step D: promote -> transition verified -> promoted ---
        let req_promote = format!(
            r#"{{"id":"e2e-4","method":"evolve.promote","project":"e2e_proj","candidate_id":"{}"}}"#,
            candidate_id
        );
        let resp_m = rpc_call(socket_path, &req_promote).await;
        assert!(
            resp_m.get("error").is_none(),
            "promote failed: {}",
            serde_json::to_string(&resp_m).unwrap()
        );
        let result_m = resp_m.get("result").expect("result missing");
        assert_eq!(
            result_m.get("status").and_then(|v| v.as_str()),
            Some("promoted")
        );
        assert_eq!(
            result_m.get("candidate_id").and_then(|v| v.as_str()),
            Some(candidate_id)
        );

        // Final DB check
        let rec_m = super::candidates::get_candidate_by_id(&shard_db, candidate_id)
            .unwrap()
            .expect("candidate should exist after promote");
        assert_eq!(rec_m.status, "promoted");

        accept_task.abort();
        let _ = tokio::fs::remove_file(socket_path).await;
    }

    // Phase 6: Runtime watcher spawn — register via socket must start FileSystemWatcher
    #[tokio::test]
    async fn test_register_spawns_watcher_on_running_daemon() {
        use std::fs::write;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        write(root.join("init.rs"), "// init").unwrap();

        let socket_path = "/tmp/magellan_test_reg_watch.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_path = socket.with_extension("reg_watch.toml");
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(reg_path.clone()).unwrap(),
        ));
        let (batch_tx, mut batch_rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(socket.with_extension("meta_reg.db")).unwrap(),
        ));

        // Watcher map + shutdown channel (new API)
        let watcher_map: std::sync::Arc<
            tokio::sync::Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>,
        > = Default::default();
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Spawn accept loop with NEW signature (watcher_map, shutdown_rx)
        let wm = watcher_map.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_db.clone();
                let tx = batch_tx.clone();
                let wm_inner = wm.clone();
                let sr = shutdown_rx.clone();
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream,
                        reg,
                        meta,
                        tx,
                        Some(wm_inner),
                        Some(sr),
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Send "register" request via socket
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let root_str = root.to_string_lossy();
        let req = format!(
            r#"{{"id":"reg1","method":"register","name":"testreg","root":"{}"}}"#,
            root_str.clone().escape_default()
        );
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.clone() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.unwrap();
        assert!(n > 0, "expected register acknowledgment");
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(
            resp.get("error").is_none(),
            "register failed: {}",
            serde_json::to_string(&resp).unwrap()
        );
        assert_eq!(resp["result"]["registered"].as_str(), Some("testreg"));

        // Write a new file to trigger the watcher that should have been spawned
        // Allow watcher ~2s to set up inotify before creating the file
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        write(root.join("trigger.rs"), "// trigger file").unwrap();

        // MUST receive a TaggedBatch from the watcher within 10s
        let batch = tokio::time::timeout(tokio::time::Duration::from_secs(10), batch_rx.recv())
            .await
            .expect("timed out waiting for watcher batch — watcher was NOT spawned on register")
            .expect("batch channel closed");

        assert_eq!(batch.project_name, "testreg");
        assert!(!batch.paths.is_empty(), "batch should contain dirty paths");

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
        let _ = tokio::fs::remove_file(reg_path).await;
        let _ = tokio::fs::remove_file(socket.with_extension("meta_reg.db")).await;
    }

    // P7: Admin socket "events" method returns logged daemon events
    #[tokio::test]
    async fn test_admin_socket_events_returns_logged_events() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        use tokio::net::{UnixListener, UnixStream};

        let socket_path = "/tmp/magellan_test_events.sock";
        let socket = std::path::PathBuf::from(socket_path);
        let _ = tokio::fs::remove_file(&socket).await;

        let listener = UnixListener::bind(socket_path).unwrap();
        let reg_path = socket.with_extension("toml");
        let reg = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::registry::Registry::load_from(reg_path).unwrap(),
        ));

        let meta_path = socket.with_extension("meta_events.db");
        let meta_db = std::sync::Arc::new(tokio::sync::Mutex::new(
            super::meta_db::MetaDb::open_at(&meta_path).unwrap(),
        ));

        // Pre-seed some events directly
        {
            let mut meta = meta_db.lock().await;
            meta.log_event(&super::meta_db::DaemonEvent {
                id: None,
                event_type: "batch_received".to_string(),
                project_name: Some("testproj".to_string()),
                file_path: None,
                details: Some(serde_json::json!({ "paths": 2 })),
                created_at: super::now_secs(),
                execution_id: None,
            })
            .unwrap();
            meta.log_event(&super::meta_db::DaemonEvent {
                id: None,
                event_type: "admin_request".to_string(),
                project_name: None,
                file_path: None,
                details: Some(serde_json::json!({ "method": "ping" })),
                created_at: super::now_secs(),
                execution_id: None,
            })
            .unwrap();
        }

        let meta_clone = meta_db.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let reg = reg.clone();
                let meta = meta_clone.clone();
                let (tx, _rx) = tokio::sync::mpsc::channel::<super::types::TaggedBatch>(16);
                tokio::spawn(async move {
                    let _ = super::admin_socket::AdminSocket::handle_client(
                        stream, reg, meta, tx, None, None,
                    )
                    .await;
                });
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to socket");
        let req = r#"{"id":"ev1","method":"events","params":{"limit":10}}"#;
        let (read_half, mut write_half) = stream.split();
        write_half
            .write_all((req.to_string() + "\n").as_bytes())
            .await
            .unwrap();
        write_half.shutdown().await.unwrap();

        let mut reader = tokio::io::BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert!(
            resp.get("error").is_none(),
            "events should succeed, got: {}",
            line
        );
        let events = resp
            .get("result")
            .and_then(|r| r.get("events"))
            .and_then(|v| v.as_array())
            .expect("events array missing");
        // Should have at least 2 pre-seeded + 1 admin_request from this connection
        assert!(
            events.len() >= 2,
            "expected at least 2 events, got {}: {}",
            events.len(),
            line
        );

        accept_task.abort();
        let _ = tokio::fs::remove_file(&socket).await;
        let _ = tokio::fs::remove_file(&meta_path).await;
    }
}
