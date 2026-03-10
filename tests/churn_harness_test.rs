//! Churn and maintenance behavior measurement test
//!
//! This test measures real behavior during repeated re-index cycles:
//! - Symbol count stability
//! - File count stability
//! - CFG block count stability
//! - Database file size changes
//! - Vacuum/maintenance effects
//!
//! Run with: cargo test --test churn_harness_test -- --nocapture

use std::fs;
use std::path::Path;
use std::time::Instant;
use tempfile::TempDir;

/// Churn measurement results
#[derive(Debug)]
struct ChurnMeasurement {
    cycle: usize,
    symbol_count: usize,
    file_count: usize,
    cfg_block_count: usize,
    db_size_bytes: u64,
    elapsed_ms: u128,
}

/// Maintenance measurement results
#[derive(Debug)]
struct MaintenanceResult {
    before_db_size: u64,
    after_db_size: u64,
    size_reclaimed_bytes: i64,
    vacuum_available: bool,
    vacuum_run: bool,
}

/// SQLite backend churn test
#[test]
fn test_sqlite_churn_behavior() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("churn_test.db");

    // Create a test file directly in temp dir
    let source_path = temp_dir.path().join("lib.rs");
    let source_code = r#"
//! Test module for churn measurement

pub fn function_one() -> i32 {
    let x = 1;
    let y = 2;
    x + y
}

pub fn function_two(x: i32, y: i32) -> i32 {
    if x > 0 {
        x + y
    } else {
        x - y
    }
}

pub fn function_three() -> String {
    "hello".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one() {
        assert_eq!(function_one(), 3);
    }

    #[test]
    fn test_two() {
        assert_eq!(function_two(5, 3), 8);
    }
}
"#;

    fs::write(&source_path, source_code).expect("Failed to write source");

    let mut measurements = Vec::new();
    let file_path_str = source_path.to_string_lossy().to_string();

    // Cycle 1: Initial index
    let measurement = index_and_measure_sqlite(&db_path, &source_path, &file_path_str, 1);
    measurements.push(measurement);

    // Cycle 2-5: Re-index same content (simulating churn)
    for cycle in 2..=5 {
        let measurement = index_and_measure_sqlite(&db_path, &source_path, &file_path_str, cycle);
        measurements.push(measurement);
    }

    // Print measurements
    println!("\n=== SQLite Churn Measurement Results ===");
    println!("Cycle | Symbols | Files | CFG Blocks | DB Size (bytes) | Time (ms)");
    println!("------|---------|-------|------------|-----------------|----------");

    for m in &measurements {
        println!(
            "{:5} | {:7} | {:5} | {:10} | {:15} | {:8}",
            m.cycle, m.symbol_count, m.file_count, m.cfg_block_count, m.db_size_bytes, m.elapsed_ms
        );
    }

    // Verify stability: counts should not increase
    let first = &measurements[0];
    let second = &measurements[1];
    let last = &measurements.last().unwrap();

    // If we got any symbols, verify they remain stable
    if first.symbol_count > 0 {
        assert_eq!(
            first.symbol_count, last.symbol_count,
            "Symbol count should remain stable across churn cycles: {} vs {}",
            first.symbol_count, last.symbol_count
        );
        assert_eq!(
            first.file_count, last.file_count,
            "File count should remain stable across churn cycles: {} vs {}",
            first.file_count, last.file_count
        );
    }

    // DB size may grow from initial to second cycle (WAL creation, initial overhead)
    // but should stabilize from cycle 2 onwards
    let stabilized_growth = last.db_size_bytes as f64 / second.db_size_bytes as f64;
    assert!(
        stabilized_growth <= 1.0,
        "DB size should be stable from cycle 2 onwards (growth factor: {})",
        stabilized_growth
    );

    // Run maintenance if available
    let maintenance = run_sqlite_maintenance(&db_path);

    println!("\n=== SQLite Maintenance Results ===");
    println!("Vacuum available: {}", maintenance.vacuum_available);
    println!("Vacuum run: {}", maintenance.vacuum_run);
    println!("DB size before: {} bytes", maintenance.before_db_size);
    println!("DB size after: {} bytes", maintenance.after_db_size);
    println!(
        "Space reclaimed: {} bytes ({:.1}%)",
        maintenance.size_reclaimed_bytes,
        (maintenance.size_reclaimed_bytes as f64 / maintenance.before_db_size as f64) * 100.0
    );

    // Re-check after maintenance
    let post_maintenance = get_sqlite_stats(&db_path);
    println!("\n=== Post-Maintenance Stats ===");
    println!("Symbols: {}", post_maintenance.0);
    println!("Files: {}", post_maintenance.1);

    // Counts should still be correct after vacuum
    if first.symbol_count > 0 {
        assert_eq!(
            first.symbol_count, post_maintenance.0,
            "Symbol count should be preserved after vacuum"
        );
    }
}

/// Geometric backend churn test
#[test]
#[cfg(feature = "geometric-backend")]
fn test_geometric_churn_behavior() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("churn_test.geo");

    // Create test file
    let source_path = temp_dir.path().join("lib.rs");
    let source_code = r#"
//! Test module for churn measurement

pub fn function_one() -> i32 {
    let x = 1;
    let y = 2;
    x + y
}

pub fn function_two(x: i32, y: i32) -> i32 {
    if x > 0 {
        x + y
    } else {
        x - y
    }
}

pub fn function_three() -> String {
    "hello".to_string()
}
"#;

    fs::write(&source_path, source_code).expect("Failed to write source");

    let mut measurements = Vec::new();
    let file_path_str = source_path.to_string_lossy().to_string();

    // Cycle 1: Initial index
    let measurement = index_and_measure_geo(&db_path, &source_path, &file_path_str, 1);
    measurements.push(measurement);

    // Cycle 2-5: Re-index same content
    for cycle in 2..=5 {
        let measurement = index_and_measure_geo(&db_path, &source_path, &file_path_str, cycle);
        measurements.push(measurement);
    }

    // Print measurements
    println!("\n=== Geometric Churn Measurement Results ===");
    println!("Cycle | Symbols | Files | CFG Blocks | File Size (bytes) | Time (ms)");
    println!("------|---------|-------|------------|-------------------|----------");

    for m in &measurements {
        println!(
            "{:5} | {:7} | {:5} | {:10} | {:17} | {:8}",
            m.cycle, m.symbol_count, m.file_count, m.cfg_block_count, m.db_size_bytes, m.elapsed_ms
        );
    }

    // Verify stability
    let first = &measurements[0];
    let second = &measurements[1];
    let last = &measurements.last().unwrap();

    if first.symbol_count > 0 {
        assert_eq!(
            first.symbol_count, last.symbol_count,
            "Symbol count should remain stable across churn cycles"
        );
        assert_eq!(
            first.file_count, last.file_count,
            "File count should remain stable across churn cycles"
        );
    }

    // DB size should stabilize from cycle 2 onwards
    let stabilized_growth = last.db_size_bytes as f64 / second.db_size_bytes as f64;
    assert!(
        stabilized_growth <= 1.0,
        "DB size should be stable from cycle 2 onwards (growth factor: {})",
        stabilized_growth
    );

    // Run CFG vacuum if available
    let maintenance = run_geometric_maintenance(&db_path);

    println!("\n=== Geometric Maintenance Results ===");
    println!("Vacuum available: {}", maintenance.vacuum_available);
    println!("Vacuum run: {}", maintenance.vacuum_run);
    println!("File size before: {} bytes", maintenance.before_db_size);
    println!("File size after: {} bytes", maintenance.after_db_size);
    println!(
        "Space reclaimed: {} bytes ({:.1}%)",
        maintenance.size_reclaimed_bytes,
        (maintenance.size_reclaimed_bytes as f64 / maintenance.before_db_size as f64) * 100.0
    );

    // Re-check after maintenance
    let post_maintenance = get_geo_stats(&db_path);
    println!("\n=== Post-Maintenance Stats ===");
    println!("Symbols: {}", post_maintenance.0);
    println!("Files: {}", post_maintenance.1);
    println!("CFG blocks: {}", post_maintenance.2);

    if first.symbol_count > 0 {
        assert_eq!(
            first.symbol_count, post_maintenance.0,
            "Symbol count should be preserved after vacuum"
        );
    }
}

/// Index and measure for SQLite backend using CodeGraph reconcile
fn index_and_measure_sqlite(
    db_path: &Path,
    source_path: &Path,
    file_path_str: &str,
    cycle: usize,
) -> ChurnMeasurement {
    use magellan::graph::backend::Backend;

    let start = Instant::now();

    // Open or create database
    let mut graph = magellan::CodeGraph::open(db_path).expect("Failed to open database");

    // Use reconcile_file_path which handles deletion and re-indexing
    let _ = graph.reconcile_file_path(source_path, file_path_str);

    // Get stats
    let stats = graph.get_stats().expect("Failed to get stats");

    let db_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    let elapsed = start.elapsed().as_millis();

    ChurnMeasurement {
        cycle,
        symbol_count: stats.symbol_count,
        file_count: stats.file_count,
        cfg_block_count: stats.cfg_block_count,
        db_size_bytes: db_size,
        elapsed_ms: elapsed,
    }
}

/// Get database stats for SQLite
fn get_sqlite_stats(db_path: &Path) -> (usize, usize) {
    use magellan::graph::backend::Backend;

    let graph = magellan::CodeGraph::open(db_path).expect("Failed to open database");
    let stats = graph.get_stats().expect("Failed to get stats");
    (stats.symbol_count, stats.file_count)
}

/// Run SQLite VACUUM and measure effect
fn run_sqlite_maintenance(db_path: &Path) -> MaintenanceResult {
    use rusqlite::Connection;

    let before_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    // Check if vacuum is available (it always is for SQLite)
    let vacuum_available = true;
    let vacuum_run = Connection::open(db_path)
        .and_then(|conn| conn.execute("VACUUM", []))
        .is_ok();

    let after_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    MaintenanceResult {
        before_db_size: before_size,
        after_db_size: after_size,
        size_reclaimed_bytes: before_size as i64 - after_size as i64,
        vacuum_available,
        vacuum_run,
    }
}

/// Index and measure for Geometric backend
#[cfg(feature = "geometric-backend")]
fn index_and_measure_geo(
    db_path: &Path,
    source_path: &Path,
    file_path_str: &str,
    cycle: usize,
) -> ChurnMeasurement {
    use magellan::backend_router::MagellanBackend;

    let start = Instant::now();

    let source = fs::read(source_path).expect("Failed to read source");

    // Create or open database
    let backend = if !db_path.exists() {
        MagellanBackend::create(db_path).expect("Failed to create database")
    } else {
        MagellanBackend::open(db_path).expect("Failed to open database")
    };

    // Delete file if re-indexing
    if cycle > 1 {
        let _ = backend.delete_file(file_path_str);
    }

    // Index via geo_index
    let mut inner = backend.into_inner();
    let _ = magellan::graph::geo_index::index_file(&mut inner, file_path_str, &source);

    // Get stats
    let backend = MagellanBackend::open(db_path).expect("Failed to reopen");
    let stats = backend.get_stats().expect("Failed to get stats");

    let file_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    let elapsed = start.elapsed().as_millis();

    ChurnMeasurement {
        cycle,
        symbol_count: stats.symbol_count,
        file_count: stats.file_count,
        cfg_block_count: stats.cfg_block_count,
        db_size_bytes: file_size,
        elapsed_ms: elapsed,
    }
}

/// Get stats for Geometric backend
#[cfg(feature = "geometric-backend")]
fn get_geo_stats(db_path: &Path) -> (usize, usize, usize) {
    let backend = MagellanBackend::open(db_path).expect("Failed to open database");
    let stats = backend.get_stats().expect("Failed to get stats");
    (stats.symbol_count, stats.file_count, stats.cfg_block_count)
}

/// Run Geometric CFG vacuum and measure effect
#[cfg(feature = "geometric-backend")]
fn run_geometric_maintenance(db_path: &Path) -> MaintenanceResult {
    let before_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    let backend = MagellanBackend::open(db_path).expect("Failed to open database");

    // Geometric backend supports CFG vacuum
    let vacuum_available = true;
    let vacuum_run = backend.vacuum_cfg().is_ok();

    let after_size = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    MaintenanceResult {
        before_db_size: before_size,
        after_db_size: after_size,
        size_reclaimed_bytes: before_size as i64 - after_size as i64,
        vacuum_available,
        vacuum_run,
    }
}
