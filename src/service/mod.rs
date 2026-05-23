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

mod admin_socket;
mod meta_db;
pub mod registry;
mod types;

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
                    eprintln!("[daemon] meta.db upsert error for '{}': {}", entry.name, e);
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
                            open_graphs.get_mut(&batch.project_name).unwrap()
                        }
                        Err(err) => {
                            eprintln!(
                                "[daemon] Failed to open DB {} for '{}': {}",
                                db.display(),
                                batch.project_name,
                                err
                            );
                            continue;
                        }
                    },
                };

                for raw_path in &batch.paths {
                    let path = if raw_path.is_absolute() {
                        raw_path.clone()
                    } else {
                        root.join(raw_path)
                    };
                    let path_key = magellan::normalize_path(&path)
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());

                    if let Err(e) = graph.reconcile_file_path(&path, &path_key) {
                        eprintln!(
                            "[daemon] Reconcile error for {} in '{}': {}",
                            path.display(),
                            batch.project_name,
                            e
                        );
                    }
                }

                if let Err(e) = graph.checkpoint_wal() {
                    eprintln!(
                        "[daemon] WAL checkpoint failed for '{}': {}",
                        batch.project_name, e
                    );
                }

                // Update meta.db last_reindexed for this project
                if let Some(ref meta_path) = meta_db_path {
                    if let Ok(mut meta) = meta_db::MetaDb::open_at(meta_path) {
                        // Ensure project entry exists before updating
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
            eprintln!(
                "[daemon] WAL checkpoint failed on shutdown for '{}': {}",
                name, e
            );
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

    // Spawn the actual blocking filesystem watcher
    let _task = tokio::task::spawn_blocking(move || {
        let cfg = WatcherConfig {
            root_path: root.clone(),
            ..Default::default()
        };
        let fw = match FileSystemWatcher::new(root, cfg, flag.clone()) {
            Ok(w) => w,
            Err(e) => {
                eprintln!(
                    "[daemon] Failed to start watcher for {}: {}",
                    name_for_blocking, e
                );
                return;
            }
        };
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
}
