use magellan::graph::CodeGraph;
use tempfile::tempdir;
use rusqlite::OptionalExtension;

fn main() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    
    println!("Opening CodeGraph...");
    let _graph = CodeGraph::open(&db_path).unwrap();
    println!("CodeGraph opened successfully");
    
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    
    // Check magellan_meta
    let version: i64 = conn
        .query_row("SELECT magellan_schema_version FROM magellan_meta WHERE id=1", [], |r| r.get(0))
        .unwrap();
    println!("Magellan schema version: {}", version);
    assert_eq!(version, 7);
    
    // Check cfg_blocks table
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cfg_blocks'",
            [],
            |_| Ok(true)
        )
        .optional()
        .unwrap()
        .unwrap_or(false);
    println!("cfg_blocks table exists: {}", table_exists);
    assert!(table_exists);
    
    // Check indexes
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_cfg_blocks%'",
            [],
            |r| r.get(0)
        )
        .unwrap();
    println!("CFG indexes found: {}", count);
    assert_eq!(count, 3);
    
    println!("\nSUCCESS: All CFG schema elements verified!");
}
