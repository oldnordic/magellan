//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod ask_cmd;
mod ast_cmd;
mod backfill_cmd;
mod blast_score_cmd;
mod candidate_fact_cmd;
mod catalog_cmd;
mod cli;
mod collisions_cmd;
mod command_dispatch;
mod command_special;
mod condense_cmd;
mod config_cmd;
mod context_cmd;
mod context_output;
mod cross_file_refs_cmd;
mod cycles_cmd;
mod cypher_cmd;
mod db_resolver;
mod dead_code_cmd;
mod delete_cmd;
mod doctor_cmd;
mod embed_cmd;
mod enrich_cmd;
mod explore_cmd;
mod export_cmd;
mod features_cmd;
mod files_cmd;
mod find_cmd;
mod get_cmd;
mod hnsw_cmd;
mod hook_cmd;
mod hopgraph_cmd;
mod import_lsif_cmd;
mod index_cmd;
mod ingest_coverage;
mod ingest_coverage_cmd;
mod init_cmd;
mod label_cmd;
mod migrate_cmd;
mod navigate_cmd;
mod orient_cmd;
mod path_enumeration_cmd;
mod project_metadata_cmd;
mod query_cmd;
mod reachable_cmd;
mod refresh_cmd;
mod refs_cmd;
mod repair_edges_cmd;
mod score_cmd;
mod search_cmd;
mod service;
mod service_cmd;
mod slice_cmd;
mod source_inventory_cmd;
mod status_cmd;
mod telemetry_cmd;
mod temporal_query_cmd;
mod temporal_sweep_cmd;
mod verify_cmd;
mod version;
mod watch_cmd;

use std::process::ExitCode;

use cli::parse_args;

// Re-export for other command modules that use crate::generate_execution_id
pub use magellan::output::generate_execution_id;
pub use magellan::output::OutputFormat;
pub use magellan::CodeGraph;

fn print_short_usage() {
    cli::print_short_usage();
}

fn print_full_usage() {
    cli::print_full_usage();
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Handle help flags before parsing
    if args.len() >= 2 {
        match args[1].as_str() {
            "--help" | "-h" => {
                print_short_usage();
                return ExitCode::SUCCESS;
            }
            "--help-full" | "-H" => {
                print_full_usage();
                return ExitCode::SUCCESS;
            }
            "--backends" => {
                cli::print_backend_info();
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
    }

    // Handle --detect-backend before command dispatch
    if args.contains(&"--detect-backend".to_string()) {
        let db_idx = args.iter().position(|a| a == "--db");
        let db_path = match db_idx {
            Some(idx) if idx + 1 < args.len() => std::path::PathBuf::from(&args[idx + 1]),
            _ => {
                eprintln!("Error: --db required for --detect-backend");
                return ExitCode::from(1);
            }
        };
        match magellan::migrate_backend_cmd::detect_backend_format(&db_path) {
            Ok(format) => {
                println!("{}", format.as_str());
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
        }
    }

    if args.len() < 2 {
        print_short_usage();
        return ExitCode::from(1);
    }

    match parse_args() {
        Ok(command) => command_dispatch::run(command),
        Err(e) => {
            eprintln!("Error: {}", e);
            print_short_usage();
            ExitCode::from(1)
        }
    }
}
