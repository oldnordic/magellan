//! Telemetry operations for tracking performance metrics
//!
//! Records fine-grained timing events (phase start/end), counters, and gauges
//! associated with execution IDs. Provides both historical querying (SQLite)
//! and real-time access (in-memory ring buffer).

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

/// Telemetry event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TelemetryEventType {
    PhaseStart,
    PhaseEnd,
    Counter,
    Gauge,
}

impl TelemetryEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TelemetryEventType::PhaseStart => "phase_start",
            TelemetryEventType::PhaseEnd => "phase_end",
            TelemetryEventType::Counter => "counter",
            TelemetryEventType::Gauge => "gauge",
        }
    }
}

impl std::str::FromStr for TelemetryEventType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "phase_start" => Ok(TelemetryEventType::PhaseStart),
            "phase_end" => Ok(TelemetryEventType::PhaseEnd),
            "counter" => Ok(TelemetryEventType::Counter),
            "gauge" => Ok(TelemetryEventType::Gauge),
            _ => Err(format!("Unknown telemetry event type: {}", s)),
        }
    }
}

/// A single telemetry event
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelemetryEvent {
    pub id: i64,
    pub execution_id: String,
    pub event_type: TelemetryEventType,
    pub event_name: String,
    pub timestamp_ns: i64,
    pub duration_ns: Option<i64>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Default ring buffer capacity for real-time telemetry
const DEFAULT_RING_BUFFER_CAPACITY: usize = 10_000;

/// Backend storage for TelemetryOps
enum TelemetryBackend {
    /// SQLite database path (opens new connection per operation)
    Sqlite(std::path::PathBuf),
    /// Shared connection from CodeGraph (avoids opening new connections)
    Shared(Arc<parking_lot::Mutex<rusqlite::Connection>>),
}

/// Telemetry operations for tracking performance metrics
///
/// Uses either:
/// - SQLite connection to database file
/// - Shared connection from CodeGraph
pub struct TelemetryOps {
    backend: TelemetryBackend,
    /// In-memory ring buffer for real-time access to recent events
    ring_buffer: Arc<parking_lot::Mutex<VecDeque<TelemetryEvent>>>,
    /// Maximum number of events to keep in the ring buffer
    ring_capacity: usize,
}

impl TelemetryOps {
    /// Create a new TelemetryOps with the given database path
    pub fn new(db_path: &Path) -> Self {
        Self {
            backend: TelemetryBackend::Sqlite(db_path.to_path_buf()),
            ring_buffer: Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(
                DEFAULT_RING_BUFFER_CAPACITY,
            ))),
            ring_capacity: DEFAULT_RING_BUFFER_CAPACITY,
        }
    }

    /// Create a TelemetryOps using a shared connection
    ///
    /// This eliminates redundant connection opens by reusing CodeGraph's side_conn
    pub fn with_connection(conn: Arc<parking_lot::Mutex<rusqlite::Connection>>) -> Self {
        let ops = Self {
            backend: TelemetryBackend::Shared(conn),
            ring_buffer: Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(
                DEFAULT_RING_BUFFER_CAPACITY,
            ))),
            ring_capacity: DEFAULT_RING_BUFFER_CAPACITY,
        };
        if let Err(e) = ops.ensure_schema() {
            eprintln!("Warning: Failed to ensure TelemetryOps schema: {}", e);
        }
        ops
    }

    /// Create an in-memory TelemetryOps for testing/stub usage
    pub fn in_memory() -> Self {
        let temp_dir = std::env::temp_dir();
        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("SystemTime before UNIX_EPOCH")
                .as_nanos()
        );
        let db_path = temp_dir.join(format!("magellan_telemetry_stub_{}.db", unique_id));

        let ops = Self {
            backend: TelemetryBackend::Sqlite(db_path),
            ring_buffer: Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(
                DEFAULT_RING_BUFFER_CAPACITY,
            ))),
            ring_capacity: DEFAULT_RING_BUFFER_CAPACITY,
        };

        if let Err(e) = ops.ensure_schema() {
            eprintln!("Warning: Failed to ensure TelemetryOps schema: {}", e);
        }

        ops
    }

    /// Get a connection to the database (SQLite backend only)
    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        match &self.backend {
            TelemetryBackend::Sqlite(path) => rusqlite::Connection::open(path),
            TelemetryBackend::Shared(_) => Err(rusqlite::Error::InvalidParameterName(
                "Direct SQLite connection not available for shared backend".to_string(),
            )),
        }
    }

    fn ensure_schema_sqlite(conn: &rusqlite::Connection) -> Result<(), anyhow::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS telemetry_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                execution_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_name TEXT NOT NULL,
                timestamp_ns INTEGER NOT NULL,
                duration_ns INTEGER,
                value REAL,
                unit TEXT,
                metadata TEXT
            )",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create telemetry_events table: {}", e))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_telemetry_events_execution
             ON telemetry_events(execution_id)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create execution index: {}", e))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_telemetry_events_type_name
             ON telemetry_events(event_type, event_name)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create type_name index: {}", e))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_telemetry_events_timestamp
             ON telemetry_events(timestamp_ns)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create timestamp index: {}", e))?;

        Ok(())
    }

    pub fn ensure_schema(&self) -> Result<()> {
        match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                Self::ensure_schema_sqlite(&conn)
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                Self::ensure_schema_sqlite(&conn)
            }
        }
    }

    fn now_ns() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64
    }

    fn push_to_ring(&self, event: TelemetryEvent) {
        let mut buf = self.ring_buffer.lock();
        if buf.len() >= self.ring_capacity {
            buf.pop_front();
        }
        buf.push_back(event);
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_event_sqlite(
        conn: &rusqlite::Connection,
        execution_id: &str,
        event_type: TelemetryEventType,
        event_name: &str,
        timestamp_ns: i64,
        duration_ns: Option<i64>,
        value: Option<f64>,
        unit: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<i64> {
        let metadata_str = metadata.map(|m| m.to_string());
        conn.execute(
            "INSERT INTO telemetry_events
                (execution_id, event_type, event_name, timestamp_ns, duration_ns, value, unit, metadata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                execution_id,
                event_type.as_str(),
                event_name,
                timestamp_ns,
                duration_ns,
                value,
                unit,
                metadata_str.as_deref(),
            ],
        )
        .map_err(|e| anyhow::anyhow!("Failed to insert telemetry event: {}", e))?;

        Ok(conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_event(
        &self,
        execution_id: &str,
        event_type: TelemetryEventType,
        event_name: &str,
        timestamp_ns: i64,
        duration_ns: Option<i64>,
        value: Option<f64>,
        unit: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<i64> {
        let row_id = match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                Self::insert_event_sqlite(
                    &conn,
                    execution_id,
                    event_type,
                    event_name,
                    timestamp_ns,
                    duration_ns,
                    value,
                    unit,
                    metadata,
                )?
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                Self::insert_event_sqlite(
                    &conn,
                    execution_id,
                    event_type,
                    event_name,
                    timestamp_ns,
                    duration_ns,
                    value,
                    unit,
                    metadata,
                )?
            }
        };

        // Push to ring buffer for real-time access
        self.push_to_ring(TelemetryEvent {
            id: row_id,
            execution_id: execution_id.to_string(),
            event_type,
            event_name: event_name.to_string(),
            timestamp_ns,
            duration_ns,
            value,
            unit: unit.map(|s| s.to_string()),
            metadata: metadata.cloned(),
        });

        Ok(row_id)
    }

    /// Record the start of a phase
    pub fn record_phase_start(&self, execution_id: &str, phase: &str) -> Result<i64> {
        self.insert_event(
            execution_id,
            TelemetryEventType::PhaseStart,
            phase,
            Self::now_ns(),
            None,
            None,
            None,
            None,
        )
    }

    /// Record the end of a phase
    ///
    /// Computes duration by looking up the matching phase_start event
    pub fn record_phase_end(&self, execution_id: &str, phase: &str) -> Result<i64> {
        let end_ns = Self::now_ns();

        // Find the matching phase_start to compute duration
        let start_ns = match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                conn.query_row(
                    "SELECT timestamp_ns FROM telemetry_events
                     WHERE execution_id = ?1 AND event_type = 'phase_start' AND event_name = ?2
                     ORDER BY timestamp_ns DESC LIMIT 1",
                    params![execution_id, phase],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| anyhow::anyhow!("Failed to query phase start: {}", e))?
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                conn.query_row(
                    "SELECT timestamp_ns FROM telemetry_events
                     WHERE execution_id = ?1 AND event_type = 'phase_start' AND event_name = ?2
                     ORDER BY timestamp_ns DESC LIMIT 1",
                    params![execution_id, phase],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| anyhow::anyhow!("Failed to query phase start: {}", e))?
            }
        };

        let duration_ns = start_ns.map(|s| end_ns - s);

        self.insert_event(
            execution_id,
            TelemetryEventType::PhaseEnd,
            phase,
            end_ns,
            duration_ns,
            None,
            duration_ns.map(|_| "ns"),
            None,
        )
    }

    /// Record a counter increment
    pub fn record_counter(
        &self,
        execution_id: &str,
        name: &str,
        value: f64,
        unit: &str,
    ) -> Result<i64> {
        self.insert_event(
            execution_id,
            TelemetryEventType::Counter,
            name,
            Self::now_ns(),
            None,
            Some(value),
            Some(unit),
            None,
        )
    }

    /// Record a gauge value
    pub fn record_gauge(
        &self,
        execution_id: &str,
        name: &str,
        value: f64,
        unit: &str,
    ) -> Result<i64> {
        self.insert_event(
            execution_id,
            TelemetryEventType::Gauge,
            name,
            Self::now_ns(),
            None,
            Some(value),
            Some(unit),
            None,
        )
    }

    /// Record a telemetry event with custom metadata
    pub fn record_event(
        &self,
        execution_id: &str,
        event_type: TelemetryEventType,
        event_name: &str,
        value: Option<f64>,
        unit: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<i64> {
        self.insert_event(
            execution_id,
            event_type,
            event_name,
            Self::now_ns(),
            None,
            value,
            unit,
            metadata,
        )
    }

    fn row_to_event(row: &rusqlite::Row) -> Result<TelemetryEvent, rusqlite::Error> {
        let event_type_str: String = row.get(2)?;
        let event_type = match event_type_str.as_str() {
            "phase_start" => TelemetryEventType::PhaseStart,
            "phase_end" => TelemetryEventType::PhaseEnd,
            "counter" => TelemetryEventType::Counter,
            "gauge" => TelemetryEventType::Gauge,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Unknown telemetry event type: {}", event_type_str),
                    )),
                ));
            }
        };

        let metadata_str: Option<String> = row.get(8)?;
        let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(TelemetryEvent {
            id: row.get(0)?,
            execution_id: row.get(1)?,
            event_type,
            event_name: row.get(3)?,
            timestamp_ns: row.get(4)?,
            duration_ns: row.get(5)?,
            value: row.get(6)?,
            unit: row.get(7)?,
            metadata,
        })
    }

    /// Get all telemetry events for an execution
    pub fn get_events_for_execution(&self, execution_id: &str) -> Result<Vec<TelemetryEvent>> {
        let sql = "SELECT id, execution_id, event_type, event_name, timestamp_ns,
                          duration_ns, value, unit, metadata
                   FROM telemetry_events
                   WHERE execution_id = ?1
                   ORDER BY timestamp_ns ASC";

        match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                let mut stmt = conn.prepare(sql)?;
                let events = stmt
                    .query_map(params![execution_id], Self::row_to_event)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect telemetry events: {}", e))?;
                Ok(events)
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                let mut stmt = conn.prepare(sql)?;
                let events = stmt
                    .query_map(params![execution_id], Self::row_to_event)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect telemetry events: {}", e))?;
                Ok(events)
            }
        }
    }

    /// Get phase durations for an execution
    ///
    /// Returns (phase_name, duration_ns) pairs from phase_end events
    pub fn get_phase_durations(&self, execution_id: &str) -> Result<Vec<(String, i64)>> {
        let sql = "SELECT event_name, duration_ns
                   FROM telemetry_events
                   WHERE execution_id = ?1 AND event_type = 'phase_end' AND duration_ns IS NOT NULL
                   ORDER BY timestamp_ns ASC";

        match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt
                    .query_map(params![execution_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect phase durations: {}", e))?;
                Ok(rows)
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt
                    .query_map(params![execution_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect phase durations: {}", e))?;
                Ok(rows)
            }
        }
    }

    /// Get recent events across all executions
    pub fn get_recent_events(&self, limit: usize) -> Result<Vec<TelemetryEvent>> {
        let sql = format!(
            "SELECT id, execution_id, event_type, event_name, timestamp_ns,
                    duration_ns, value, unit, metadata
             FROM telemetry_events
             ORDER BY timestamp_ns DESC
             LIMIT {}",
            limit
        );

        match &self.backend {
            TelemetryBackend::Sqlite(_) => {
                let conn = self.connect()?;
                let mut stmt = conn.prepare(&sql)?;
                let events = stmt
                    .query_map([], Self::row_to_event)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect recent events: {}", e))?;
                Ok(events)
            }
            TelemetryBackend::Shared(conn_arc) => {
                let conn = conn_arc.lock();
                let mut stmt = conn.prepare(&sql)?;
                let events = stmt
                    .query_map([], Self::row_to_event)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("Failed to collect recent events: {}", e))?;
                Ok(events)
            }
        }
    }

    /// Get a snapshot of the in-memory ring buffer
    ///
    /// This is the real-time API — no database query, just memory access
    pub fn snapshot_ring_buffer(&self) -> Vec<TelemetryEvent> {
        let buf = self.ring_buffer.lock();
        buf.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_telemetry_schema() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let telemetry = TelemetryOps::new(&db_path);

        telemetry.ensure_schema().unwrap();

        let conn = telemetry.connect().unwrap();
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='telemetry_events'",
                [],
                |_| Ok(true),
            )
            .optional()
            .unwrap()
            .unwrap_or(false);

        assert!(table_exists, "telemetry_events table should exist");
    }

    #[test]
    fn test_record_phase_timing() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let telemetry = TelemetryOps::new(&db_path);

        telemetry.ensure_schema().unwrap();

        let exec_id = "test-exec-001";

        // Record phase start
        let start_id = telemetry.record_phase_start(exec_id, "parse").unwrap();
        assert!(start_id > 0);

        // Small delay
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Record phase end
        let end_id = telemetry.record_phase_end(exec_id, "parse").unwrap();
        assert!(end_id > 0);

        // Query phase durations
        let durations = telemetry.get_phase_durations(exec_id).unwrap();
        assert_eq!(durations.len(), 1);
        assert_eq!(durations[0].0, "parse");
        assert!(
            durations[0].1 >= 10_000_000,
            "Duration should be at least 10ms in ns"
        );
    }

    #[test]
    fn test_record_counter_and_gauge() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let telemetry = TelemetryOps::new(&db_path);

        telemetry.ensure_schema().unwrap();

        let exec_id = "test-exec-002";

        telemetry
            .record_counter(exec_id, "files_indexed", 150.0, "count")
            .unwrap();
        telemetry
            .record_gauge(exec_id, "memory_mb", 512.0, "mb")
            .unwrap();

        let events = telemetry.get_events_for_execution(exec_id).unwrap();
        assert_eq!(events.len(), 2);

        let counter = &events[0];
        assert_eq!(counter.event_type, TelemetryEventType::Counter);
        assert_eq!(counter.event_name, "files_indexed");
        assert_eq!(counter.value, Some(150.0));
        assert_eq!(counter.unit.as_deref(), Some("count"));

        let gauge = &events[1];
        assert_eq!(gauge.event_type, TelemetryEventType::Gauge);
        assert_eq!(gauge.event_name, "memory_mb");
        assert_eq!(gauge.value, Some(512.0));
        assert_eq!(gauge.unit.as_deref(), Some("mb"));
    }

    #[test]
    fn test_ring_buffer() {
        let telemetry = TelemetryOps::in_memory();

        let exec_id = "test-exec-003";
        for i in 0..5 {
            telemetry
                .record_counter(exec_id, "iteration", i as f64, "count")
                .unwrap();
        }

        let snapshot = telemetry.snapshot_ring_buffer();
        assert_eq!(snapshot.len(), 5);

        // Verify order (oldest first in VecDeque)
        assert_eq!(snapshot[0].value, Some(0.0));
        assert_eq!(snapshot[4].value, Some(4.0));
    }

    #[test]
    fn test_get_recent_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let telemetry = TelemetryOps::new(&db_path);

        telemetry.ensure_schema().unwrap();

        for i in 0..10 {
            let exec_id = format!("test-exec-{}", i);
            telemetry
                .record_counter(&exec_id, "test", i as f64, "count")
                .unwrap();
        }

        let recent = telemetry.get_recent_events(5).unwrap();
        assert_eq!(recent.len(), 5);
        // Most recent first
        assert_eq!(recent[0].execution_id, "test-exec-9");
        assert_eq!(recent[4].execution_id, "test-exec-5");
    }
}
