use magellan::{FileSystemWatcher, WatcherConfig};
use std::fs::File;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;

/// Helper: poll for event with timeout
fn poll_for_event(watcher: &FileSystemWatcher, timeout_ms: u64) -> Option<magellan::FileEvent> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        if let Some(event) = watcher.try_recv_event() {
            return Some(event);
        }

        if start.elapsed() >= timeout {
            return None;
        }

        sleep(Duration::from_millis(50));
    }
}

#[test]
fn test_file_create_event() {
    let temp_dir = TempDir::new().unwrap();
    let watcher =
        FileSystemWatcher::new(temp_dir.path().to_path_buf(), WatcherConfig::default()).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    let file_path = temp_dir.path().join("test.rs");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();

    // Poll for event with timeout
    let event = poll_for_event(&watcher, 2000);

    assert!(event.is_some(), "Should receive create event");
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    assert_eq!(event.event_type, magellan::EventType::Create);
}

#[test]
fn test_file_modify_event() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.rs");

    // Create file first
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn old() {{}}").unwrap();
    drop(file);

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let watcher =
        FileSystemWatcher::new(temp_dir.path().to_path_buf(), WatcherConfig::default()).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Modify file
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn new() {{}}").unwrap();

    // Poll for modify event
    let event = poll_for_event(&watcher, 2000);

    assert!(event.is_some(), "Should receive modify event");
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    assert_eq!(event.event_type, magellan::EventType::Modify);
}

#[test]
fn test_file_delete_event() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.rs");

    // Create file first
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();
    drop(file);

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let watcher =
        FileSystemWatcher::new(temp_dir.path().to_path_buf(), WatcherConfig::default()).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Delete file
    std::fs::remove_file(&file_path).unwrap();

    // Poll for delete event
    let event = poll_for_event(&watcher, 2000);

    assert!(event.is_some(), "Should receive delete event");
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    assert_eq!(event.event_type, magellan::EventType::Delete);
}

#[test]
fn test_debounce_rapid_changes() {
    let temp_dir = TempDir::new().unwrap();
    let watcher =
        FileSystemWatcher::new(temp_dir.path().to_path_buf(), WatcherConfig::default()).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    let file_path = temp_dir.path().join("test.rs");

    // Rapidly modify file 3 times
    for i in 0..3 {
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn v{}() {{}}", i).unwrap();
        drop(file);
        sleep(Duration::from_millis(50));
    }

    // Wait for debounce period + buffer
    sleep(Duration::from_millis(600));

    // Count events - rapid changes should produce multiple events
    let mut event_count = 0;
    while let Some(_) = watcher.try_recv_event() {
        event_count += 1;
        if event_count > 10 {
            break;
        }
    }

    // Should receive at least 1 event (OS-dependent debouncing)
    assert!(
        event_count >= 1,
        "Should receive at least 1 event, got {}",
        event_count
    );
}

#[test]
fn test_watch_temp_directory() {
    let temp_dir = TempDir::new().unwrap();
    let watcher =
        FileSystemWatcher::new(temp_dir.path().to_path_buf(), WatcherConfig::default()).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Create nested directory and file
    let subdir = temp_dir.path().join("nested");
    std::fs::create_dir(&subdir).unwrap();

    // Give time for directory creation to settle
    sleep(Duration::from_millis(100));

    let file_path = subdir.join("test.rs");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();

    // Poll for event - may get directory event first
    let mut found_file_event = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(2000);

    while start.elapsed() < timeout {
        if let Some(event) = watcher.try_recv_event() {
            if event.path == file_path {
                found_file_event = true;
                break;
            }
            // Directory events are filtered out in convert_notify_event
        }
        sleep(Duration::from_millis(50));
    }

    assert!(found_file_event, "Should receive event for nested file");
}
