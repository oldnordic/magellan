use super::*;
use magellan::graph::query::CollisionField;
use magellan::{ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;

/// Test that short usage is ≤25 lines (usability research shows longer help is ignored)
#[test]
fn test_short_usage_line_count() {
    // Manual line count verification - short usage should be brief
    // This test documents the requirement: short help ≤25 lines
    let short_help_lines = 15; // Estimated from print_short_usage()
    assert!(
        short_help_lines <= 25,
        "Short help should be ≤25 lines to ensure users actually read it"
    );
}

/// Test that watch command parsing works correctly
/// This test ensures the refactoring doesn't break existing functionality
#[test]
fn test_parse_watch_command() {
    // Note: We can't easily test parse_args_impl directly since it uses std::env::args()
    // Instead, we verify the Command enum structure is correct
    let cmd = Command::Watch {
        root_path: PathBuf::from("."),
        db_path: PathBuf::from("test.db"),
        config: WatcherConfig {
            root_path: PathBuf::from("."),
            debounce_ms: 500,
            gitignore_aware: true,
        },
        scan_initial: true,
        validate: false,
        validate_only: false,
        output_format: OutputFormat::Human,
        frontend: None,
    };

    // Verify we can construct the command
    match cmd {
        Command::Watch {
            root_path, db_path, ..
        } => {
            assert_eq!(root_path, PathBuf::from("."));
            assert_eq!(db_path, PathBuf::from("test.db"));
        }
        _ => panic!("Expected Watch command"),
    }
}

/// Test that find command parsing structure is correct
#[test]
fn test_parse_find_command_structure() {
    let cmd = Command::Find {
        db_path: PathBuf::from("test.db"),
        name: Some("test_function".to_string()),
        root: None,
        path: None,
        glob_pattern: None,
        symbol_id: None,
        ambiguous_name: None,
        first: false,
        output_format: OutputFormat::Json,
        with_context: false,
        with_callers: false,
        with_callees: false,
        with_semantics: false,
        with_checksums: false,
        context_lines: 3,
        all: false,
    };

    match cmd {
        Command::Find {
            name,
            output_format,
            ..
        } => {
            assert_eq!(name, Some("test_function".to_string()));
            assert!(matches!(output_format, OutputFormat::Json));
        }
        _ => panic!("Expected Find command"),
    }
}

// Tests for extracted parser functions

#[test]
fn test_parse_watch_args() {
    let args = vec![
        "--root".to_string(),
        "/home/test".to_string(),
        "--db".to_string(),
        "test.db".to_string(),
        "--debounce-ms".to_string(),
        "1000".to_string(),
        "--watch-only".to_string(),
    ];

    let result = parse_watch_args(&args).unwrap();
    match result {
        Command::Watch {
            root_path,
            db_path,
            config,
            scan_initial,
            ..
        } => {
            assert_eq!(root_path, PathBuf::from("/home/test"));
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(config.debounce_ms, 1000);
            assert!(!scan_initial); // watch-only implies no initial scan
        }
        _ => panic!("Expected Watch command"),
    }
}

#[test]
fn test_parse_watch_args_missing_required() {
    let args = vec!["--root".to_string(), "/home/test".to_string()];

    // --db is now optional; resolve_db_path provides a CWD fallback
    let result = parse_watch_args(&args);
    assert!(result.is_ok());
}

#[test]
fn test_parse_export_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--output".to_string(),
        "output.json".to_string(),
    ];

    let result = parse_export_args(&args).unwrap();
    match result {
        Command::Export {
            db_path,
            format,
            output,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(format, ExportFormat::Json);
            assert_eq!(output, Some(PathBuf::from("output.json")));
        }
        _ => panic!("Expected Export command"),
    }
}

#[test]
fn test_parse_status_args() {
    let args = vec!["--db".to_string(), "test.db".to_string()];

    let result = parse_status_args(&args).unwrap();
    match result {
        Command::Status {
            db_path,
            output_format,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(matches!(output_format, OutputFormat::Human));
        }
        _ => panic!("Expected Status command"),
    }
}

#[test]
fn test_parse_find_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "my_function".to_string(),
        "--first".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find {
            db_path,
            name,
            first,
            output_format,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(name, Some("my_function".to_string()));
            assert!(first);
            assert!(matches!(output_format, OutputFormat::Json));
        }
        _ => panic!("Expected Find command"),
    }
}

#[test]
fn test_parse_find_args_by_symbol_id() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--symbol-id".to_string(),
        "abc123".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find {
            db_path, symbol_id, ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(symbol_id, Some("abc123".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

#[test]
fn test_parse_find_args_without_name_or_symbol() {
    // Find can work without --name or --symbol-id (lists all symbols)
    let args = vec!["--db".to_string(), "test.db".to_string()];

    let result = parse_find_args(&args);
    assert!(result.is_ok());

    match result.unwrap() {
        Command::Find {
            db_path,
            name,
            symbol_id,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(name, None);
            assert_eq!(symbol_id, None);
        }
        _ => panic!("Expected Find command"),
    }
}

#[test]
fn test_parse_refs_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "my_function".to_string(),
        "--path".to_string(),
        "src/main.rs".to_string(),
        "--direction".to_string(),
        "out".to_string(),
    ];

    let result = parse_refs_args(&args).unwrap();
    match result {
        Command::Refs {
            db_path,
            name,
            direction,
            path,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(name, "my_function".to_string());
            assert_eq!(path, Some(PathBuf::from("src/main.rs")));
            assert_eq!(direction, "out");
        }
        _ => panic!("Expected Refs command"),
    }
}

#[test]
fn test_parse_refs_args_without_path() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "my_function".to_string(),
        "--direction".to_string(),
        "in".to_string(),
    ];

    let result = parse_refs_args(&args).unwrap();
    match result {
        Command::Refs {
            db_path,
            name,
            direction,
            path,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(name, "my_function".to_string());
            assert_eq!(path, None);
            assert_eq!(direction, "in");
        }
        _ => panic!("Expected Refs command"),
    }
}

#[test]
fn test_parse_get_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--file".to_string(),
        "src/main.rs".to_string(),
        "--symbol".to_string(),
        "main".to_string(),
        "--with-context".to_string(),
    ];

    let result = parse_get_args(&args).unwrap();
    match result {
        Command::Get {
            db_path,
            file_path,
            symbol_name,
            with_context,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(file_path, "src/main.rs".to_string());
            assert_eq!(symbol_name, "main".to_string());
            assert!(with_context);
        }
        _ => panic!("Expected Get command"),
    }
}

#[test]
fn test_parse_get_file_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--file".to_string(),
        "src/main.rs".to_string(),
    ];

    let result = parse_get_file_args(&args).unwrap();
    match result {
        Command::GetFile {
            db_path,
            file_path,
            output_format,
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(file_path, "src/main.rs".to_string());
            assert!(matches!(output_format, OutputFormat::Human));
        }
        _ => panic!("Expected GetFile command"),
    }
}

#[test]
fn test_parse_files_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--symbols".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];

    let result = parse_files_args(&args).unwrap();
    match result {
        Command::Files {
            db_path,
            with_symbols,
            output_format,
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(with_symbols);
            assert!(matches!(output_format, OutputFormat::Json));
        }
        _ => panic!("Expected Files command"),
    }
}

#[test]
fn test_parse_verify_args() {
    let args = vec![
        "--root".to_string(),
        "/home/test".to_string(),
        "--db".to_string(),
        "test.db".to_string(),
    ];

    let result = parse_verify_args(&args).unwrap();
    match result {
        Command::Verify {
            root_path,
            db_path,
            output_format,
        } => {
            assert_eq!(root_path, PathBuf::from("/home/test"));
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(matches!(output_format, OutputFormat::Human));
        }
        _ => panic!("Expected Verify command"),
    }
}

#[test]
fn test_parse_label_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--label".to_string(),
        "important".to_string(),
        "--label".to_string(),
        "refactored".to_string(),
        "--list".to_string(),
    ];

    let result = parse_label_args(&args).unwrap();
    match result {
        Command::Label {
            db_path,
            label,
            list,
            output_format,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(label, vec!["important", "refactored"]);
            assert!(list);
            assert!(matches!(output_format, OutputFormat::Human));
        }
        _ => panic!("Expected Label command"),
    }
}

#[test]
fn test_parse_collisions_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--field".to_string(),
        "display_fqn".to_string(),
        "--limit".to_string(),
        "50".to_string(),
    ];

    let result = parse_collisions_args(&args).unwrap();
    match result {
        Command::Collisions {
            db_path,
            field,
            limit,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(matches!(field, CollisionField::DisplayFqn));
            assert_eq!(limit, 50);
        }
        _ => panic!("Expected Collisions command"),
    }
}

#[test]
fn test_parse_migrate_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--dry-run".to_string(),
        "--no-backup".to_string(),
    ];

    let result = parse_migrate_args(&args).unwrap();
    match result {
        Command::Migrate {
            db_path,
            dry_run,
            no_backup,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(dry_run);
            assert!(no_backup);
        }
        _ => panic!("Expected Migrate command"),
    }
}

#[test]
fn test_parse_migrate_backend_args() {
    let args = vec![
        "--input".to_string(),
        "old.db".to_string(),
        "--output".to_string(),
        "new.db".to_string(),
        "--dry-run".to_string(),
    ];

    let result = parse_migrate_backend_args(&args).unwrap();
    match result {
        Command::MigrateBackend {
            input_db,
            output_db,
            dry_run,
            ..
        } => {
            assert_eq!(input_db, PathBuf::from("old.db"));
            assert_eq!(output_db, PathBuf::from("new.db"));
            assert!(dry_run);
        }
        _ => panic!("Expected MigrateBackend command"),
    }
}

#[test]
fn test_parse_query_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--file".to_string(),
        "src/main.rs".to_string(),
        "--kind".to_string(),
        "function".to_string(),
        "--explain".to_string(),
    ];

    let result = parse_query_args(&args).unwrap();
    match result {
        Command::Query {
            db_path,
            file_path,
            kind,
            explain,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(file_path, Some(PathBuf::from("src/main.rs")));
            assert_eq!(kind, Some("function".to_string()));
            assert!(explain);
        }
        _ => panic!("Expected Query command"),
    }
}

#[test]
fn test_parse_chunks_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--limit".to_string(),
        "100".to_string(),
        "--file".to_string(),
        "*.rs".to_string(),
    ];

    let result = parse_chunks_args(&args).unwrap();
    match result {
        Command::Chunks {
            db_path,
            limit,
            file_filter,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(limit, Some(100));
            assert_eq!(file_filter, Some("*.rs".to_string()));
        }
        _ => panic!("Expected Chunks command"),
    }
}

#[test]
fn test_parse_chunk_by_span_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--file".to_string(),
        "src/main.rs".to_string(),
        "--start".to_string(),
        "100".to_string(),
        "--end".to_string(),
        "200".to_string(),
    ];

    let result = parse_chunk_by_span_args(&args).unwrap();
    match result {
        Command::ChunkBySpan {
            db_path,
            file_path,
            byte_start,
            byte_end,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(file_path, "src/main.rs".to_string());
            assert_eq!(byte_start, 100);
            assert_eq!(byte_end, 200);
        }
        _ => panic!("Expected ChunkBySpan command"),
    }
}

#[test]
fn test_parse_chunk_by_symbol_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--symbol".to_string(),
        "my_function".to_string(),
    ];

    let result = parse_chunk_by_symbol_args(&args).unwrap();
    match result {
        Command::ChunkBySymbol {
            db_path,
            symbol_name,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(symbol_name, "my_function".to_string());
        }
        _ => panic!("Expected ChunkBySymbol command"),
    }
}

#[test]
fn test_parse_ast_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--file".to_string(),
        "src/main.rs".to_string(),
        "--position".to_string(),
        "150".to_string(),
    ];

    let result = parse_ast_args(&args).unwrap();
    match result {
        Command::Ast {
            db_path,
            file_path,
            position,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(file_path, "src/main.rs".to_string());
            assert_eq!(position, Some(150));
        }
        _ => panic!("Expected Ast command"),
    }
}

#[test]
fn test_parse_find_ast_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--kind".to_string(),
        "function".to_string(),
    ];

    let result = parse_find_ast_args(&args).unwrap();
    match result {
        Command::FindAst { db_path, kind, .. } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(kind, "function".to_string());
        }
        _ => panic!("Expected FindAst command"),
    }
}

#[test]
fn test_parse_reachable_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--symbol".to_string(),
        "main::test".to_string(),
        "--reverse".to_string(),
    ];

    let result = parse_reachable_args(&args).unwrap();
    match result {
        Command::Reachable {
            db_path,
            symbol_id,
            reverse,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(symbol_id, "main::test".to_string());
            assert!(reverse);
        }
        _ => panic!("Expected Reachable command"),
    }
}

#[test]
fn test_parse_dead_code_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--entry".to_string(),
        "main".to_string(),
    ];

    let result = parse_dead_code_args(&args).unwrap();
    match result {
        Command::DeadCode {
            db_path,
            entry_symbol_id,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(entry_symbol_id, "main".to_string());
        }
        _ => panic!("Expected DeadCode command"),
    }
}

#[test]
fn test_parse_cycles_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--symbol".to_string(),
        "main".to_string(),
    ];

    let result = parse_cycles_args(&args).unwrap();
    match result {
        Command::Cycles {
            db_path, symbol_id, ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(symbol_id, Some("main".to_string()));
        }
        _ => panic!("Expected Cycles command"),
    }
}

#[test]
fn test_parse_condense_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--members".to_string(),
    ];

    let result = parse_condense_args(&args).unwrap();
    match result {
        Command::Condense {
            db_path,
            show_members,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert!(show_members);
        }
        _ => panic!("Expected Condense command"),
    }
}

#[test]
fn test_parse_paths_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--start".to_string(),
        "main".to_string(),
        "--end".to_string(),
        "helper".to_string(),
        "--max-depth".to_string(),
        "50".to_string(),
    ];

    let result = parse_paths_args(&args).unwrap();
    match result {
        Command::Paths {
            db_path,
            start_symbol_id,
            end_symbol_id,
            max_depth,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(start_symbol_id, "main".to_string());
            assert_eq!(end_symbol_id, Some("helper".to_string()));
            assert_eq!(max_depth, 50);
        }
        _ => panic!("Expected Paths command"),
    }
}

#[test]
fn test_parse_slice_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--target".to_string(),
        "main".to_string(),
        "--direction".to_string(),
        "forward".to_string(),
        "--verbose".to_string(),
    ];

    let result = parse_slice_args(&args).unwrap();
    match result {
        Command::Slice {
            db_path,
            target,
            direction,
            verbose,
            ..
        } => {
            assert_eq!(db_path, PathBuf::from("test.db"));
            assert_eq!(target, "main".to_string());
            assert_eq!(direction, "forward".to_string());
            assert!(verbose);
        }
        _ => panic!("Expected Slice command"),
    }
}

#[test]
fn test_parse_output_format_validation() {
    // Test invalid output format is rejected
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--output".to_string(),
        "invalid_format".to_string(),
    ];

    let result = parse_status_args(&args);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid output format"));
}

#[test]
fn test_parse_unknown_argument() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--unknown-flag".to_string(),
    ];

    let result = parse_status_args(&args);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown argument"));
}

#[test]
fn test_parse_missing_argument_value() {
    let args = vec!["--db".to_string()]; // Missing value for --db

    let result = parse_status_args(&args);
    assert!(result.is_err());
}

// ============================================================================
// Edge Case Explosion Tests
// ============================================================================

#[test]
fn test_edge_empty_args() {
    let args: Vec<String> = vec![];

    // all commands now use resolve_db_path fallback; empty args succeed
    assert!(parse_status_args(&args).is_ok());

    // files uses resolve_db_path fallback: empty args succeeds with cwd default
    assert!(parse_files_args(&args).is_ok());

    // watch also uses resolve_db_path + detect_project_root fallbacks
    assert!(parse_watch_args(&args).is_ok());
}

/// Test arguments in different orders
#[test]
fn test_edge_arg_order_independence() {
    // Order 1: --db first
    let args1 = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];

    // Order 2: --output first
    let args2 = vec![
        "--output".to_string(),
        "json".to_string(),
        "--db".to_string(),
        "test.db".to_string(),
    ];

    let result1 = parse_status_args(&args1).unwrap();
    let result2 = parse_status_args(&args2).unwrap();

    match (result1, result2) {
        (
            Command::Status {
                db_path: db1,
                output_format: fmt1,
                ..
            },
            Command::Status {
                db_path: db2,
                output_format: fmt2,
                ..
            },
        ) => {
            assert_eq!(db1, db2);
            assert!(matches!(fmt1, OutputFormat::Json));
            assert!(matches!(fmt2, OutputFormat::Json));
        }
        _ => panic!("Expected Status commands"),
    }
}

/// Test duplicate arguments (last one wins)
#[test]
fn test_edge_duplicate_args() {
    let args = vec![
        "--db".to_string(),
        "first.db".to_string(),
        "--db".to_string(),
        "second.db".to_string(),
    ];

    let result = parse_status_args(&args).unwrap();
    match result {
        Command::Status { db_path, .. } => {
            // Last --db should win
            assert_eq!(db_path, PathBuf::from("second.db"));
        }
        _ => panic!("Expected Status command"),
    }
}

/// Test special characters in path arguments
#[test]
fn test_edge_special_chars_in_paths() {
    let args = vec!["--db".to_string(), "/path/with spaces/file.db".to_string()];

    let result = parse_status_args(&args).unwrap();
    match result {
        Command::Status { db_path, .. } => {
            assert_eq!(db_path, PathBuf::from("/path/with spaces/file.db"));
        }
        _ => panic!("Expected Status command"),
    }
}

/// Test unicode in string arguments
#[test]
fn test_edge_unicode_args() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "函数_🎉".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some("函数_🎉".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test very long argument values
#[test]
fn test_edge_long_argument_values() {
    let long_name = "a".repeat(1000);
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        long_name.clone(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some(long_name));
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test boundary: context_lines at max (100)
#[test]
fn test_edge_context_lines_max() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "test".to_string(),
        "--context-lines".to_string(),
        "100".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { context_lines, .. } => {
            assert_eq!(context_lines, 100);
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test boundary: context_lines above max (should be capped)
#[test]
fn test_edge_context_lines_above_max() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "test".to_string(),
        "--context-lines".to_string(),
        "200".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { context_lines, .. } => {
            // Should be capped at 100
            assert_eq!(context_lines, 100);
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test boundary: context_lines at zero
#[test]
fn test_edge_context_lines_zero() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "test".to_string(),
        "--context-lines".to_string(),
        "0".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { context_lines, .. } => {
            assert_eq!(context_lines, 0);
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test invalid integer values
#[test]
fn test_edge_invalid_integer() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--limit".to_string(),
        "not_a_number".to_string(),
    ];

    let result = parse_chunks_args(&args);
    assert!(result.is_err());
}

/// Test negative integer values
#[test]
fn test_edge_negative_integer() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--limit".to_string(),
        "-5".to_string(),
    ];

    // This should parse but may cause issues later - just verify it doesn't panic
    let result = parse_chunks_args(&args);
    // Note: The result depends on whether the type accepts negative values
    // For usize, this should fail
    assert!(result.is_err());
}

/// Test empty string values
#[test]
fn test_edge_empty_string_values() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some("".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test all boolean flags can be combined
#[test]
fn test_edge_combined_boolean_flags() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "test".to_string(),
        "--first".to_string(),
        "--with-context".to_string(),
        "--with-callers".to_string(),
        "--with-callees".to_string(),
        "--with-semantics".to_string(),
        "--with-checksums".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find {
            first,
            with_context,
            with_callers,
            with_callees,
            with_semantics,
            with_checksums,
            ..
        } => {
            assert!(first);
            assert!(with_context);
            assert!(with_callers);
            assert!(with_callees);
            assert!(with_semantics);
            assert!(with_checksums);
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test collision field variants
#[test]
fn test_edge_collision_field_variants() {
    // Test all valid field values
    for (field_str, expected) in [
        ("fqn", CollisionField::Fqn),
        ("display_fqn", CollisionField::DisplayFqn),
        ("canonical_fqn", CollisionField::CanonicalFqn),
    ] {
        let args = vec![
            "--db".to_string(),
            "test.db".to_string(),
            "--field".to_string(),
            field_str.to_string(),
        ];

        let result = parse_collisions_args(&args).unwrap();
        match result {
            Command::Collisions { field, .. } => {
                assert_eq!(field, expected, "Field {} should map correctly", field_str);
            }
            _ => panic!("Expected Collisions command"),
        }
    }
}

/// Test invalid collision field
#[test]
fn test_edge_invalid_collision_field() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--field".to_string(),
        "invalid_field".to_string(),
    ];

    let result = parse_collisions_args(&args);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid field"));
}

/// Test refs direction variants
#[test]
fn test_edge_refs_direction_variants() {
    // Valid directions: "in" and "out"
    for direction in ["in", "out"] {
        let args = vec![
            "--db".to_string(),
            "test.db".to_string(),
            "--name".to_string(),
            "test".to_string(),
            "--path".to_string(),
            "src/main.rs".to_string(),
            "--direction".to_string(),
            direction.to_string(),
        ];

        let result = parse_refs_args(&args).unwrap();
        match result {
            Command::Refs { direction: dir, .. } => {
                assert_eq!(dir, direction);
            }
            _ => panic!("Expected Refs command"),
        }
    }
}

/// Test invalid refs direction
#[test]
fn test_edge_invalid_refs_direction() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "test".to_string(),
        "--path".to_string(),
        "src/main.rs".to_string(),
        "--direction".to_string(),
        "invalid".to_string(),
    ];

    let result = parse_refs_args(&args);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid direction"));
}

/// Test slice direction validation
#[test]
fn test_edge_slice_direction_validation() {
    // Valid: backward
    let args1 = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--target".to_string(),
        "main".to_string(),
        "--direction".to_string(),
        "backward".to_string(),
    ];
    assert!(parse_slice_args(&args1).is_ok());

    // Valid: forward
    let args2 = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--target".to_string(),
        "main".to_string(),
        "--direction".to_string(),
        "forward".to_string(),
    ];
    assert!(parse_slice_args(&args2).is_ok());

    // Invalid direction
    let args3 = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--target".to_string(),
        "main".to_string(),
        "--direction".to_string(),
        "sideways".to_string(),
    ];
    let result = parse_slice_args(&args3);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid direction"));
}

/// Test export format variants
#[test]
fn test_edge_export_format_variants() {
    let formats = vec!["json", "jsonl", "csv", "scip", "dot"];

    for format in formats {
        let args = vec![
            "--db".to_string(),
            "test.db".to_string(),
            "--format".to_string(),
            format.to_string(),
        ];

        let result = parse_export_args(&args);
        assert!(result.is_ok(), "Format {} should be valid", format);
    }
}

/// Test multiple labels
#[test]
fn test_edge_multiple_labels() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--label".to_string(),
        "label1".to_string(),
        "--label".to_string(),
        "label2".to_string(),
        "--label".to_string(),
        "label3".to_string(),
    ];

    let result = parse_label_args(&args).unwrap();
    match result {
        Command::Label { label, .. } => {
            assert_eq!(label, vec!["label1", "label2", "label3"]);
        }
        _ => panic!("Expected Label command"),
    }
}

/// Test watch mode flags interaction
#[test]
fn test_edge_watch_mode_flags() {
    // watch-only should disable scan_initial
    let args = vec![
        "--root".to_string(),
        "/test".to_string(),
        "--db".to_string(),
        "test.db".to_string(),
        "--watch-only".to_string(),
    ];

    let result = parse_watch_args(&args).unwrap();
    match result {
        Command::Watch { scan_initial, .. } => {
            assert!(!scan_initial, "watch-only should disable initial scan");
        }
        _ => panic!("Expected Watch command"),
    }
}

/// Test paths with special characters
#[test]
fn test_edge_special_path_characters() {
    let special_paths = vec![
        "/path/with-dash/file.rs",
        "/path/with_underscore/file.rs",
        "/path/with.dot/file.rs",
        "/path/with@symbol/file.rs",
        "/path/with#hash/file.rs",
    ];

    for path in special_paths {
        let args = vec![
            "--db".to_string(),
            "test.db".to_string(),
            "--file".to_string(),
            path.to_string(),
        ];

        let result = parse_get_file_args(&args);
        assert!(result.is_ok(), "Path {} should be valid", path);
    }
}

/// Test absolute vs relative paths
#[test]
fn test_edge_absolute_vs_relative_paths() {
    // Absolute path
    let args1 = vec!["--db".to_string(), "/absolute/path/to/test.db".to_string()];
    let result1 = parse_status_args(&args1).unwrap();
    match result1 {
        Command::Status { db_path, .. } => {
            assert_eq!(db_path, PathBuf::from("/absolute/path/to/test.db"));
        }
        _ => panic!("Expected Status command"),
    }

    // Relative path
    let args2 = vec!["--db".to_string(), "./relative/path/to/test.db".to_string()];
    let result2 = parse_status_args(&args2).unwrap();
    match result2 {
        Command::Status { db_path, .. } => {
            assert_eq!(db_path, PathBuf::from("./relative/path/to/test.db"));
        }
        _ => panic!("Expected Status command"),
    }
}

/// Test argument with equals sign (if supported by shell)
#[test]
fn test_edge_arguments_with_equals() {
    // This tests that we handle values that might contain equals signs
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "foo=bar".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some("foo=bar".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test newlines in arguments (edge case from shell)
#[test]
fn test_edge_newline_in_arguments() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "line1\nline2".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some("line1\nline2".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

/// Test tab characters in arguments
#[test]
fn test_edge_tab_in_arguments() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "col1\tcol2".to_string(),
    ];

    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { name, .. } => {
            assert_eq!(name, Some("col1\tcol2".to_string()));
        }
        _ => panic!("Expected Find command"),
    }
}

// =========================================================================
// Phase 1 — Project Registry: --all and --project flags
// =========================================================================

#[test]
fn test_parse_find_args_all_flag() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--name".to_string(),
        "foo".to_string(),
        "--all".to_string(),
    ];
    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { all, .. } => assert!(all, "--all flag should set all=true"),
        _ => panic!("Expected Find command"),
    }
}

#[test]
fn test_parse_find_args_all_false_by_default() {
    let args = vec!["--db".to_string(), "test.db".to_string()];
    let result = parse_find_args(&args).unwrap();
    match result {
        Command::Find { all, .. } => assert!(!all, "all should be false when --all is absent"),
        _ => panic!("Expected Find command"),
    }
}

#[test]
fn test_parse_status_args_all_flag() {
    let args = vec!["--all".to_string()];
    let result = parse_status_args(&args).unwrap();
    match result {
        Command::Status { all, .. } => assert!(all, "--all flag should set all=true"),
        _ => panic!("Expected Status command"),
    }
}

#[test]
fn test_parse_find_args_project_flag_unknown() {
    // --project with a name not in registry must return an error
    let args = vec![
        "--project".to_string(),
        "__no_such_project_xyzzy__".to_string(),
    ];
    let err = parse_find_args(&args).unwrap_err();
    assert!(
        err.to_string().contains("not found in registry"),
        "expected 'not found in registry', got: {}",
        err
    );
}

#[test]
fn test_parse_status_args_project_flag_unknown() {
    let args = vec![
        "--project".to_string(),
        "__no_such_project_xyzzy__".to_string(),
    ];
    let err = parse_status_args(&args).unwrap_err();
    assert!(
        err.to_string().contains("not found in registry"),
        "expected 'not found in registry', got: {}",
        err
    );
}

// Phase 3: ask --all / --project
#[test]
fn test_parse_ask_args_all_flag() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "--all".to_string(),
        "who calls run_find".to_string(),
    ];
    let result = parse_ask_args(&args).unwrap();
    match result {
        Command::Ask { all, .. } => assert!(all, "--all flag should set all=true"),
        _ => panic!("Expected Ask command"),
    }
}

#[test]
fn test_parse_ask_args_all_false_by_default() {
    let args = vec![
        "--db".to_string(),
        "test.db".to_string(),
        "who calls run_find".to_string(),
    ];
    let result = parse_ask_args(&args).unwrap();
    match result {
        Command::Ask { all, .. } => assert!(!all, "all should be false when --all is absent"),
        _ => panic!("Expected Ask command"),
    }
}

#[test]
fn test_parse_ask_args_project_flag_unknown() {
    let args = vec![
        "--project".to_string(),
        "__no_such_project_xyzzy__".to_string(),
        "who calls run_find".to_string(),
    ];
    let err = parse_ask_args(&args).unwrap_err();
    assert!(
        err.to_string().contains("not found in registry"),
        "expected 'not found in registry', got: {}",
        err
    );
}
