use magellan::CodeGraph;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// Helper: Write bytes to file with full synchronization
///
/// Uses OpenOptions with create + truncate + write_all + sync_all to ensure
/// content is fully committed to disk before returning. This guarantees
/// that any subsequent MODIFY event will have stable, readable content.
fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Helper: Touch file by appending newline with synchronization
///
/// Appends a newline to the file and syncs to disk. This guarantees
/// a MODIFY event fires with stable content, ensuring the indexer
/// processes the file after it is fully written.
fn touch_and_sync(path: &Path) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new().append(true).open(path)?;
    std::io::Write::write_all(&mut file, b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[test]
fn test_create_event_indexes_file() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("test.rs");

    // Small delay to ensure directory is stable
    thread::sleep(Duration::from_millis(10));

    // Spawn file writer thread (writes file after watcher starts)
    let file_path_clone = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        // Write the file to trigger a CREATE/MODIFY event
        write_and_sync(&file_path_clone, b"fn foo() {}\nfn bar() { foo(); }").unwrap();
        touch_and_sync(&file_path_clone).unwrap();
    });

    // Run indexer with bounded events
    magellan::run_indexer_n(root_path.clone(), db_path.clone(), 3).unwrap();

    // Verify: File was indexed with symbols (use full path, as watcher reports full path)
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();

    // Should have 2 symbols (foo, bar) after the create/modify
    assert_eq!(
        symbols.len(),
        2,
        "Should index 2 symbols after MODIFY: foo, bar"
    );

    // Verify: References were indexed
    let foo_id = graph.symbol_id_by_name(&path_str, "foo").unwrap();
    let foo_id = foo_id.expect("foo symbol should exist");
    let references = graph.references_to_symbol(foo_id).unwrap();
    assert_eq!(references.len(), 1, "Should index 1 reference to foo");
}

#[test]
fn test_modify_event_reindexes_file() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("test.rs");
    let path_str = file_path.to_string_lossy().to_string();

    // Create and index initial file using indexer (not direct CodeGraph::open)
    // This avoids the "database is locked" issue from having multiple CodeGraph instances
    let file_path_init = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file_path_init, b"fn foo() {}").unwrap();
        touch_and_sync(&file_path_init).unwrap();
    });
    magellan::run_indexer_n(root_path.clone(), db_path.clone(), 3).unwrap();

    // Give SQLite time to release locks
    thread::sleep(Duration::from_millis(50));

    // Verify initial state - explicitly drop graph before next indexer run
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let symbols = graph.symbols_in_file(&path_str).unwrap();
        assert_eq!(symbols.len(), 1, "Initial state: 1 symbol");
    }

    // Modify file (add bar function and call) - spawn thread to do it after watcher starts
    let file_path_clone = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        let modified_source = b"fn foo() {}\nfn bar() { foo(); }";
        fs::write(&file_path_clone, modified_source).unwrap();
    });

    // Run indexer bounded to 1 event
    magellan::run_indexer_n(root_path.clone(), db_path.clone(), 1).unwrap();

    // Verify: File was re-indexed with new symbols
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 2, "After modify: should have 2 symbols");

    // Verify: Old references were deleted and new ones indexed
    let foo_id = graph.symbol_id_by_name(&path_str, "foo").unwrap();
    let foo_id = foo_id.expect("foo symbol should exist");
    let references = graph.references_to_symbol(foo_id).unwrap();
    assert_eq!(
        references.len(),
        1,
        "Should have 1 reference to foo after re-index"
    );
}

#[test]
fn test_delete_event_removes_file_data() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("test.rs");
    let path_str = file_path.to_string_lossy().to_string();

    // Create and index a file using indexer (avoids multiple CodeGraph instances)
    let source = b"fn foo() {}";
    let file_path_init = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file_path_init, source).unwrap();
        touch_and_sync(&file_path_init).unwrap();
    });
    magellan::run_indexer_n(root_path.clone(), db_path.clone(), 3).unwrap();

    // Give SQLite time to release locks
    thread::sleep(Duration::from_millis(50));

    // Verify initial state - explicitly drop graph before next indexer run
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let symbols = graph.symbols_in_file(&path_str).unwrap();
        assert_eq!(symbols.len(), 1, "Should have 1 symbol before delete");
    }

    // Delete the file immediately (before indexer starts)
    fs::remove_file(&file_path).unwrap();

    // Run indexer bounded to 1 event
    // The indexer will reconcile on startup and detect the deleted file
    magellan::run_indexer_n(root_path.clone(), db_path.clone(), 1).unwrap();

    // Verify: File and symbols were removed
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(
        symbols.len(),
        0,
        "Should have 0 symbols after file deletion"
    );
}

#[test]
fn test_multiple_sequential_events_produce_correct_final_state() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("test.rs");
    let path_str = file_path.to_string_lossy().to_string();

    // Small delay to ensure directory is stable
    thread::sleep(Duration::from_millis(10));

    // Event 1: Write initial file with synchronization
    let root_path1 = root_path.clone();
    let db_path1 = db_path.clone();
    let file_path1 = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file_path1, b"fn foo() {}").unwrap();
        touch_and_sync(&file_path1).unwrap();
    });

    // Process MODIFY events
    magellan::run_indexer_n(root_path1, db_path1, 3).unwrap();

    // Give SQLite time to release locks
    thread::sleep(Duration::from_millis(50));

    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let symbols = graph.symbols_in_file(&path_str).unwrap();
        assert_eq!(symbols.len(), 1, "After initial write: 1 symbol");
    }

    // Event 2: Modify file (add bar)
    let root_path2 = root_path.clone();
    let db_path2 = db_path.clone();
    let file_path2 = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file_path2, b"fn foo() {}\nfn bar() {}").unwrap();
        touch_and_sync(&file_path2).unwrap();
    });

    // Process MODIFY events
    magellan::run_indexer_n(root_path2, db_path2, 3).unwrap();

    // Give SQLite time to release locks
    thread::sleep(Duration::from_millis(50));

    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let symbols = graph.symbols_in_file(&path_str).unwrap();
        assert_eq!(symbols.len(), 2, "After adding bar: 2 symbols");
    }

    // Event 3: Modify again (add call from bar to foo)
    let root_path3 = root_path.clone();
    let db_path3 = db_path.clone();
    let file_path3 = file_path.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file_path3, b"fn foo() {}\nfn bar() { foo(); }").unwrap();
        touch_and_sync(&file_path3).unwrap();
    });

    // Process MODIFY events
    magellan::run_indexer_n(root_path3, db_path3, 2).unwrap();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 2, "After adding reference: still 2 symbols");

    // Verify final state has correct references
    let foo_id = graph.symbol_id_by_name(&path_str, "foo").unwrap();
    let foo_id = foo_id.expect("foo symbol should exist");
    let references = graph.references_to_symbol(foo_id).unwrap();
    assert_eq!(references.len(), 1, "Final state: 1 reference to foo");
}
