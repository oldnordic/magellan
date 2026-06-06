use crate::cli::Command;
use anyhow::Result;
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::cli::parsers::*;
use crate::db_resolver::resolve_db_path;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// System Parsers
// ============================================================================

/// Parse CLI arguments into a Command
///
/// This function handles all CLI argument parsing for Magellan.
/// For the --version and -V flags, it prints the version and exits.
/// For the --help and -h flags, it prints usage and exits.
///
/// The version display is handled via a closure passed in to avoid
/// circular dependencies with the version module.
pub fn parse_args_impl<F>(print_version: F) -> Result<Command>
where
    F: FnOnce(),
{
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(anyhow::anyhow!("Missing command"));
    }

    let command = &args[1];

    // Handle --version and -V flags
    if command == "--version" || command == "-V" {
        print_version();
        std::process::exit(0);
    }

    // Handle --help and -h flags
    if command == "--help" || command == "-h" {
        crate::cli::print_short_usage();
        std::process::exit(0);
    }

    // Handle --help-full and -H flags
    if command == "--help-full" || command == "-H" {
        crate::cli::print_full_usage();
        std::process::exit(0);
    }

    match command.as_str() {
        "watch" => parse_watch_args(&args[2..]),
        "backfill" => parse_backfill_args(&args[2..]),
        "cross-file-refs" => parse_cross_file_refs_args(&args[2..]),
        "delete" => parse_delete_args(&args[2..]),
        "export" => parse_export_args(&args[2..]),
        "index" => parse_index_args(&args[2..]),
        "import-lsif" => parse_import_lsif_args(&args[2..]),
        "ingest-coverage" => parse_ingest_coverage_args(&args[2..]),
        "enrich" => parse_enrich_args(&args[2..]),
        "status" => parse_status_args(&args[2..]),
        "project-metadata" => parse_project_metadata_args(&args[2..]),
        "context" => parse_context_args(&args[2..]),
        "doctor" => parse_doctor_args(&args[2..]),
        "query" => parse_query_args(&args[2..]),
        "find" => parse_find_args(&args[2..]),
        "refs" => parse_refs_args(&args[2..]),
        "get" => parse_get_args(&args[2..]),
        "get-file" => parse_get_file_args(&args[2..]),
        "files" => parse_files_args(&args[2..]),
        "verify" => parse_verify_args(&args[2..]),
        "refresh" => parse_refresh_args(&args[2..]),
        "label" => parse_label_args(&args[2..]),
        "collisions" => parse_collisions_args(&args[2..]),
        "migrate" => parse_migrate_args(&args[2..]),
        "migrate-backend" => parse_migrate_backend_args(&args[2..]),
        "chunks" => parse_chunks_args(&args[2..]),
        "chunk-by-span" => parse_chunk_by_span_args(&args[2..]),
        "chunk-by-symbol" => parse_chunk_by_symbol_args(&args[2..]),
        "ast" => parse_ast_args(&args[2..]),
        "find-ast" => parse_find_ast_args(&args[2..]),
        "reachable" => parse_reachable_args(&args[2..]),
        "dead-code" => parse_dead_code_args(&args[2..]),
        "cycles" => parse_cycles_args(&args[2..]),
        "registry" => {
            // Registry has subcommands
            if args.len() < 3 {
                return Err(anyhow::anyhow!("registry subcommand required: scan, list"));
            }
            match args[2].as_str() {
                "scan" => parse_registry_scan_args(&args[3..]),
                "list" => parse_registry_list_args(&args[3..]),
                _ => Err(anyhow::anyhow!("Unknown registry subcommand: {}", args[2])),
            }
        }
        "config" => {
            // Config has subcommands
            if args.len() < 3 {
                return Err(anyhow::anyhow!("config subcommand required: show, init"));
            }
            match args[2].as_str() {
                "show" => parse_config_show_args(&args[3..]),
                "init" => parse_config_init_args(&args[3..]),
                _ => Err(anyhow::anyhow!("Unknown config subcommand: {}", args[2])),
            }
        }
        "condense" => parse_condense_args(&args[2..]),
        "init" => parse_project_init_args(&args[2..]),
        "paths" => parse_paths_args(&args[2..]),
        "slice" => parse_slice_args(&args[2..]),
        "source-inventory" => parse_source_inventory_args(&args[2..]),
        "service" => {
            if args.len() < 3 {
                return Err(anyhow::anyhow!("service subcommand required: start, stop, list, register, unregister, pause, resume, status"));
            }
            let mut output_format = OutputFormat::Human;
            let mut name: Option<String> = None;
            let mut root: Option<PathBuf> = None;
            let mut event_type: Option<String> = None;
            let mut event_project: Option<String> = None;
            let mut since_hours: Option<u64> = None;
            let mut event_limit: usize = 50;
            let mut json_output = false;
            let mut include: Vec<String> = Vec::new();
            let mut exclude: Vec<String> = Vec::new();
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--output" | "-o" => {
                        let value = parse_required_arg(&args[..], &mut i, "--output")?;
                        output_format = parse_output_format(&value)?;
                    }
                    "--name" | "-n" => {
                        name = Some(parse_required_arg(&args[..], &mut i, "--name")?);
                    }
                    "--root" | "-r" => {
                        root = Some(parse_path_arg(&args[..], &mut i, "--root")?);
                    }
                    "--include" | "-I" => {
                        include.push(parse_required_arg(&args[..], &mut i, "--include")?);
                    }
                    "--exclude" | "-E" => {
                        exclude.push(parse_required_arg(&args[..], &mut i, "--exclude")?);
                    }
                    "--type" | "-t" => {
                        event_type = Some(parse_required_arg(&args[..], &mut i, "--type")?);
                    }
                    "--project" | "-p" => {
                        event_project = Some(parse_required_arg(&args[..], &mut i, "--project")?);
                    }
                    "--since" => {
                        since_hours = Some(
                            parse_required_arg(&args[..], &mut i, "--since")?
                                .parse()
                                .map_err(|_| anyhow::anyhow!("--since must be a number"))?,
                        );
                    }
                    "--limit" | "-l" => {
                        event_limit = parse_required_arg(&args[..], &mut i, "--limit")?
                            .parse()
                            .map_err(|_| anyhow::anyhow!("--limit must be a number"))?;
                    }
                    "--json" | "-j" => {
                        json_output = true;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            let positional = if args.len() > 3 {
                Some(args[3].clone())
            } else {
                None
            };
            let action = match args[2].as_str() {
                "start" => crate::service_cmd::ServiceAction::Start,
                "stop" => crate::service_cmd::ServiceAction::Stop,
                "list" => crate::service_cmd::ServiceAction::List,
                "register" => crate::service_cmd::ServiceAction::Register {
                    root: root.unwrap_or_else(|| PathBuf::from(".")),
                    name: name.or(positional),
                    include,
                    exclude,
                },
                "unregister" => crate::service_cmd::ServiceAction::Unregister {
                    name: name.or(positional).unwrap_or_default(),
                },
                "pause" => crate::service_cmd::ServiceAction::Pause {
                    name: name.or(positional).unwrap_or_default(),
                },
                "resume" => crate::service_cmd::ServiceAction::Resume {
                    name: name.or(positional).unwrap_or_default(),
                },
                "status" => crate::service_cmd::ServiceAction::Status,
                "stats" => crate::service_cmd::ServiceAction::Stats,
                "events" => crate::service_cmd::ServiceAction::Events {
                    project: event_project,
                    event_type,
                    since_hours,
                    limit: event_limit,
                    json_output,
                },
                _ => return Err(anyhow::anyhow!("Unknown service subcommand: {}", args[2])),
            };
            Ok(Command::Service {
                action,
                output_format,
            })
        }
        "candidate-fact" => parse_candidate_fact_args(&args[2..]),
        "cypher" => parse_cypher_args(&args[2..]),
        "hnsw-create" => parse_hnsw_create_args(&args[2..]),
        "hnsw-query" => parse_hnsw_query_args(&args[2..]),
        "ask" => parse_ask_args(&args[2..]),
        "navigate" => parse_navigate_args(&args[2..]),
        "explore" => parse_explore_args(&args[2..]),
        "telemetry" => parse_telemetry_args(&args[2..]),
        "hopgraph" => parse_hopgraph_args(&args[2..]),
        "embed" => parse_embed_args(&args[2..]),
        "features" => parse_features_args(&args[2..]),
        "service-daemon" => Ok(Command::ServiceDaemon),
        _ => Err(anyhow::anyhow!("Unknown command: {}", command)),
    }
}

/// Convenience wrapper around `parse_args_impl` that uses the version module
pub fn parse_args() -> Result<Command> {
    parse_args_impl(|| {
        println!("{}", crate::version::version());
    })
}

/// Parse the `files` command arguments
pub fn parse_files_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;
    let mut with_symbols = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db_path = Some(parse_path_arg(args, &mut i, "--db")?),
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            "--symbols" => {
                with_symbols = true;
                i += 1;
            }
            _ => return Err(anyhow::anyhow!("Unknown argument: {}", args[i])),
        }
    }

    let db_path = resolve_db_path(db_path)?;

    Ok(Command::Files {
        db_path,
        output_format,
        with_symbols,
    })
}
