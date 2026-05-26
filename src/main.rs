//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod ask_cmd;
mod ast_cmd;
mod backfill_cmd;
mod candidate_fact_cmd;
mod cli;
mod collisions_cmd;
mod condense_cmd;
mod config_cmd;
mod context_cmd;
mod cross_file_refs_cmd;
mod cycles_cmd;
mod cypher_cmd;
mod db_resolver;
mod dead_code_cmd;
mod delete_cmd;
mod doctor_cmd;
mod enrich_cmd;
mod export_cmd;
mod features_cmd;
mod files_cmd;
mod find_cmd;
mod get_cmd;
mod hnsw_cmd;
mod import_lsif_cmd;
mod index_cmd;
mod ingest_coverage_cmd;
mod ingest_coverage;
mod init_cmd;
mod label_cmd;
mod migrate_cmd;
mod navigate_cmd;
mod path_enumeration_cmd;
mod project_metadata_cmd;
mod query_cmd;
mod reachable_cmd;
mod refresh_cmd;
mod refs_cmd;
mod registry_cmd;
mod service;
mod service_cmd;
mod slice_cmd;
mod source_inventory_cmd;
mod status_cmd;
mod verify_cmd;
mod version;
mod watch_cmd;

#[cfg(feature = "web-ui")]
mod web_ui_cmd;

use magellan::output::{output_json, JsonResponse, MigrateResponse, OutputFormat};
use magellan::CodeGraph;
use std::process::ExitCode;

use cli::{parse_args, Command};
use status_cmd::run_status;

// Re-export for other command modules that use crate::generate_execution_id
pub use magellan::output::generate_execution_id;

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
        Ok(Command::Backfill { db_path }) => {
            if let Err(e) = backfill_cmd::run_backfill(db_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::CrossFileRefs {
            db_path,
            fqn,
            output_format,
        }) => {
            if let Err(e) = cross_file_refs_cmd::run_cross_file_refs(db_path, fqn, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::RegistryScan {
            root,
            output_format,
        }) => {
            if let Err(e) = registry_cmd::run_registry_scan(root, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::RegistryList {
            root,
            output_format,
        }) => {
            if let Err(e) = registry_cmd::run_registry_list(root, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ConfigShow { output_format }) => {
            if let Err(e) = config_cmd::run_config_show(output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ConfigInit { force }) => {
            if let Err(e) = config_cmd::run_config_init(force) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ProjectInit { path }) => {
            if let Err(e) = init_cmd::run_project_init(path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Delete {
            db_path,
            file_path,
            root,
        }) => {
            if let Err(e) = delete_cmd::run_delete(db_path, file_path, root) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Index {
            db_path,
            file_path,
            root,
        }) => {
            if let Err(e) = index_cmd::run_index(db_path, file_path, root) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Status {
            output_format,
            db_path,
            all,
            ..
        }) => {
            if let Err(e) = run_status(db_path, output_format, all) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ProjectMetadata {
            db_path,
            query,
            output_format,
        }) => {
            if let Err(e) =
                project_metadata_cmd::run_project_metadata(db_path, query, output_format)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Export {
            db_path,
            format,
            output,
            include_symbols,
            include_references,
            include_calls,
            minify,
            include_collisions,
            collisions_field,
            filters,
        }) => {
            if let Err(e) = export_cmd::run_export(
                db_path,
                format,
                output,
                include_symbols,
                include_references,
                include_calls,
                minify,
                include_collisions,
                collisions_field,
                filters,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ImportLsif {
            db_path,
            lsif_paths,
        }) => {
            if let Err(e) = import_lsif_cmd::run_import_lsif(db_path, lsif_paths) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::IngestCoverage { db_path, lcov_path }) => {
            if let Err(e) = ingest_coverage_cmd::run_ingest_coverage(db_path, lcov_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Enrich {
            db_path,
            files,
            timeout_secs,
        }) => {
            if let Err(e) = enrich_cmd::run_enrich(db_path, files, timeout_secs) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Context {
            subcommand,
            db_paths,
        }) => {
            use cli::ContextSubcommand;
            let result = match subcommand {
                ContextSubcommand::Build => context_cmd::run_context_build(db_paths),
                ContextSubcommand::Summary => context_cmd::run_context_summary(db_paths),
                ContextSubcommand::List {
                    kind,
                    page,
                    page_size,
                    cursor,
                    project,
                    output_format,
                } => context_cmd::run_context_list(
                    db_paths,
                    kind,
                    page,
                    page_size,
                    cursor,
                    project,
                    output_format,
                ),
                ContextSubcommand::Symbol {
                    name,
                    file,
                    callers,
                    callees,
                    output_format,
                    with_source,
                    depth,
                    project,
                } => context_cmd::run_context_symbol(
                    db_paths,
                    name,
                    file,
                    callers,
                    callees,
                    output_format,
                    with_source,
                    depth,
                    project,
                ),
                ContextSubcommand::File { path } => context_cmd::run_context_file(db_paths, path),
                ContextSubcommand::Impact {
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                } => context_cmd::run_context_impact(
                    db_paths,
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                ),
                ContextSubcommand::Affected {
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                } => context_cmd::run_context_affected(
                    db_paths,
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                ),
            };
            if let Err(e) = result {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Doctor {
            db_path,
            fix,
            output_format,
        }) => {
            if let Err(e) = doctor_cmd::run_doctor(db_path, fix, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        #[cfg(feature = "web-ui")]
        Ok(Command::WebUi {
            db_path,
            host,
            port,
        }) => {
            if let Err(e) = web_ui_cmd::run_web_ui(db_path, host, port) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Query {
            db_path,
            file_path,
            root,
            kind,
            explain,
            symbol,
            show_extent,
            output_format,
            with_context,
            with_callers,
            with_callees,
            with_semantics,
            with_checksums,
            context_lines,
        }) => {
            if let Err(e) = query_cmd::run_query(
                db_path,
                file_path,
                root,
                kind,
                explain,
                symbol,
                show_extent,
                output_format,
                with_context,
                with_callers,
                with_callees,
                with_semantics,
                with_checksums,
                context_lines,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Find {
            db_path,
            name,
            root,
            path,
            glob_pattern,
            symbol_id,
            ambiguous_name,
            first,
            output_format,
            with_context,
            with_callers,
            with_callees,
            with_semantics,
            with_checksums,
            context_lines,
            all,
        }) => {
            if let Err(e) = find_cmd::run_find(
                db_path,
                name,
                root,
                path,
                glob_pattern,
                symbol_id,
                ambiguous_name,
                first,
                output_format,
                with_context,
                with_callers,
                with_callees,
                with_semantics,
                with_checksums,
                context_lines,
                all,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Refs {
            db_path,
            name,
            root,
            path,
            symbol_id,
            direction,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
            all,
        }) => {
            if let Err(e) = refs_cmd::run_refs(
                db_path,
                name,
                root,
                path,
                symbol_id,
                direction,
                output_format,
                with_context,
                with_semantics,
                with_checksums,
                context_lines,
                all,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Files {
            db_path,
            output_format,
            with_symbols,
        }) => {
            if let Err(e) = files_cmd::run_files(db_path, with_symbols, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Collisions {
            db_path,
            field,
            limit,
            output_format,
        }) => {
            if let Err(e) = collisions_cmd::run_collisions(db_path, field, limit, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Migrate {
            db_path,
            dry_run,
            no_backup,
            output_format,
        }) => {
            match migrate_cmd::run_migrate(db_path, dry_run, no_backup) {
                Ok(result) => match output_format {
                    OutputFormat::Json | OutputFormat::Pretty => {
                        let response = MigrateResponse {
                            success: result.success,
                            backup_path: result
                                .backup_path
                                .map(|p| p.to_string_lossy().to_string()),
                            old_version: result.old_version,
                            new_version: result.new_version,
                            message: result.message,
                        };
                        let exec_id = generate_execution_id();
                        let json_response = JsonResponse::new(response, &exec_id);
                        if let Err(e) = output_json(&json_response, output_format) {
                            eprintln!("Error: {}", e);
                            return ExitCode::from(1);
                        }
                    }
                    OutputFormat::Human => {
                        if result.success {
                            println!("{}", result.message);
                            if result.old_version != result.new_version {
                                println!(
                                    "Version: {} -> {}",
                                    result.old_version, result.new_version
                                );
                            }
                            if let Some(ref backup) = result.backup_path {
                                println!("Backup: {}", backup.display());
                            }
                        } else {
                            eprintln!("Migration failed: {}", result.message);
                            return ExitCode::from(1);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        Ok(Command::MigrateBackend {
            input_db,
            output_db,
            export_dir,
            dry_run,
            output_format,
        }) => {
            match magellan::migrate_backend_cmd::run_migrate_backend(
                input_db, output_db, export_dir, dry_run,
            ) {
                Ok(result) => {
                    match output_format {
                        OutputFormat::Json | OutputFormat::Pretty => {
                            let exec_id = generate_execution_id();
                            // Create a JSON response similar to MigrateResponse
                            let json_data = serde_json::json!({
                                "success": result.success,
                                "source_format": format!("{:?}", result.source_format),
                                "target_format": format!("{:?}", result.target_format),
                                "entities_migrated": result.entities_migrated,
                                "edges_migrated": result.edges_migrated,
                                "side_tables_migrated": result.side_tables_migrated,
                                "message": result.message,
                                "execution_id": exec_id,
                            });
                            if let Err(e) =
                                output_json(&JsonResponse::new(json_data, &exec_id), output_format)
                            {
                                eprintln!("Error: {}", e);
                                return ExitCode::from(1);
                            }
                        }
                        OutputFormat::Human => {
                            if result.success {
                                println!("{}", result.message);
                                println!(
                                    "Format: {:?} -> {:?}",
                                    result.source_format, result.target_format
                                );
                                println!("Entities: {}", result.entities_migrated);
                                println!("Edges: {}", result.edges_migrated);
                                if result.side_tables_migrated {
                                    println!("Side tables: migrated");
                                }
                            } else {
                                eprintln!("Migration failed: {}", result.message);
                                return ExitCode::from(1);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Get {
            db_path,
            file_path,
            symbol_name,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        }) => {
            if let Err(e) = get_cmd::run_get(
                db_path,
                file_path,
                symbol_name,
                output_format,
                with_context,
                with_semantics,
                with_checksums,
                context_lines,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::GetFile {
            db_path,
            file_path,
            output_format,
        }) => {
            if let Err(e) = get_cmd::run_get_file(db_path, file_path, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Chunks {
            db_path,
            output_format,
            limit,
            file_filter,
            kind_filter,
        }) => {
            if let Err(e) =
                get_cmd::run_chunks(db_path, output_format, limit, file_filter, kind_filter)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ChunkBySpan {
            db_path,
            file_path,
            byte_start,
            byte_end,
            output_format,
        }) => {
            if let Err(e) =
                get_cmd::run_chunk_by_span(db_path, file_path, byte_start, byte_end, output_format)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ChunkBySymbol {
            db_path,
            symbol_name,
            output_format,
            file_filter,
        }) => {
            if let Err(e) =
                get_cmd::run_chunk_by_symbol(db_path, symbol_name, output_format, file_filter)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Label {
            db_path,
            label,
            list,
            count,
            show_code,
            output_format,
        }) => {
            if let Err(e) =
                label_cmd::run_label(db_path, label, list, count, show_code, output_format)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Verify {
            root_path,
            db_path,
            output_format,
        }) => match verify_cmd::run_verify(root_path, db_path, output_format) {
            Ok(exit_code) => ExitCode::from(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                ExitCode::from(1)
            }
        },
        Ok(Command::Watch {
            root_path,
            db_path,
            config,
            scan_initial,
            validate,
            validate_only,
            output_format,
        }) => {
            if let Err(e) = watch_cmd::run_watch(
                root_path,
                db_path,
                config,
                scan_initial,
                validate,
                validate_only,
                output_format,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Ast {
            db_path,
            file_path,
            position,
            output_format,
        }) => {
            if let Err(e) = ast_cmd::run_ast_command(db_path, file_path, position, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::FindAst {
            db_path,
            kind,
            output_format,
        }) => {
            if let Err(e) = ast_cmd::run_find_ast_command(db_path, kind, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Reachable {
            db_path,
            symbol_id,
            reverse,
            output_format,
        }) => {
            if let Err(e) = reachable_cmd::run_reachable(db_path, symbol_id, reverse, output_format)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::DeadCode {
            db_path,
            entry_symbol_id,
            output_format,
        }) => {
            if let Err(e) = dead_code_cmd::run_dead_code(db_path, entry_symbol_id, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Paths {
            db_path,
            start_symbol_id,
            end_symbol_id,
            max_depth,
            max_paths,
            output_format,
        }) => {
            if let Err(e) = path_enumeration_cmd::run_paths(
                db_path,
                start_symbol_id,
                end_symbol_id,
                max_depth,
                max_paths,
                output_format,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Cycles {
            db_path,
            symbol_id,
            output_format,
        }) => {
            if let Err(e) = cycles_cmd::run_cycles(db_path, symbol_id, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Condense {
            db_path,
            show_members,
            output_format,
        }) => {
            if let Err(e) = condense_cmd::run_condense(db_path, show_members, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Slice {
            db_path,
            target,
            direction,
            verbose,
            output_format,
        }) => {
            let cli_direction = match slice_cmd::CliSliceDirection::from_str(&direction) {
                Some(d) => d,
                None => {
                    eprintln!("Error: Invalid direction: {}", direction);
                    return ExitCode::from(1);
                }
            };
            if let Err(e) =
                slice_cmd::run_slice(db_path, target, cli_direction, verbose, output_format)
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Refresh {
            db_path: raw_db_path,
            dry_run,
            include_untracked,
            staged,
            unstaged,
            force,
            output_format,
        }) => {
            // svc-9: registry lookup for default DB path when not explicitly provided
            let db_path = if raw_db_path.as_path() == std::path::Path::new(".magellan/magellan.db")
            {
                match refresh_cmd::resolve_db_path(None) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Warning: registry lookup failed ({}), using default", e);
                        raw_db_path
                    }
                }
            } else {
                raw_db_path
            };
            let args = refresh_cmd::RefreshArgs {
                db_path,
                dry_run,
                include_untracked,
                staged,
                unstaged,
                force,
                output_format,
            };
            match refresh_cmd::run_refresh(&args) {
                Ok(report) => {
                    // Output report based on output_format
                    match output_format {
                        OutputFormat::Json | OutputFormat::Pretty => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&report).unwrap_or_default()
                            );
                        }
                        OutputFormat::Human => {
                            println!("Refresh complete:");
                            println!("  Updated: {}", report.updated.len());
                            println!("  Deleted: {}", report.deleted.len());
                            println!("  Added: {}", report.added.len());
                            println!("  Unchanged: {}", report.unchanged);
                            if report.dry_run {
                                println!("  (dry run - no changes applied)");
                            }
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    ExitCode::from(1)
                }
            }
        }
        Ok(Command::SourceInventory {
            db_path,
            scan_dirs,
            list_kind,
            show_stale,
            output_format,
        }) => {
            if let Err(e) = source_inventory_cmd::run_source_inventory(
                db_path,
                scan_dirs,
                list_kind,
                show_stale,
                output_format,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::CandidateFact {
            db_path,
            action,
            output_format,
        }) => {
            if let Err(e) = candidate_fact_cmd::run_candidate_fact(db_path, action, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Service {
            action,
            output_format,
        }) => {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("Error: failed to create async runtime: {}", e);
                    return ExitCode::from(1);
                }
            };
            if let Err(e) =
                runtime.block_on(async { service_cmd::run(action, output_format).await })
            {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::ServiceDaemon) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(std::io::stderr)
                .init();
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("Error: failed to create async runtime: {}", e);
                    return ExitCode::from(1);
                }
            };
            if let Err(e) = runtime.block_on(async {
                let (svc, _shutdown_rx) = service::Service::new().await?;
                svc.run().await
            }) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Cypher {
            db_path,
            query,
            output_format,
        }) => {
            if let Err(e) = cypher_cmd::run_cypher(db_path, query, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::HnswCreate {
            db_path,
            name,
            dim,
            m,
            ef_construction,
            ef_search,
            output_format,
        }) => {
            if let Err(e) = hnsw_cmd::run_hnsw_create(
                db_path,
                name,
                dim,
                m,
                ef_construction,
                ef_search,
                output_format,
            ) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::HnswQuery {
            db_path,
            name,
            vector,
            k,
            output_format,
        }) => {
            if let Err(e) = hnsw_cmd::run_hnsw_query(db_path, name, vector, k, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Ask {
            question,
            db_path,
            output_format,
            all,
        }) => {
            if let Err(e) = ask_cmd::run_ask(question, db_path, all, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Navigate {
            task,
            db_path,
            depth,
            budget,
            limit,
            concise,
            with_llmgrep,
            with_mirage,
        }) => {
            let cfg = navigate_cmd::NavigateConfig {
                db_path,
                task,
                depth,
                budget,
                limit,
                concise,
                with_llmgrep,
                with_mirage,
            };
            if let Err(e) = navigate_cmd::run_navigate(cfg) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Features {
            db_path,
            output_format,
        }) => {
            if let Err(e) = features_cmd::run_features(db_path, output_format) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            print_short_usage();
            ExitCode::from(1)
        }
    }
}
