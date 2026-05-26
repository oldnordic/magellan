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
mod tests;
