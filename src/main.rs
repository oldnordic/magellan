//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod ast_cmd;
mod collisions_cmd;
mod dead_code_cmd;
mod export_cmd;
mod label_cmd;
mod migrate_cmd;
mod files_cmd;
mod find_cmd;
mod get_cmd;
mod path_enumeration_cmd;
mod query_cmd;
mod reachable_cmd;
mod refs_cmd;
mod condense_cmd;
mod cycles_cmd;
mod slice_cmd;
mod verify_cmd;
mod watch_cmd;
mod status_cmd;
mod version;
mod cli;

use magellan::output::{output_json, JsonResponse, MigrateResponse, OutputFormat};
use magellan::CodeGraph;
use std::process::ExitCode;

use cli::{Command, parse_args};
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
            _ => {}
        }
    }

    if args.len() < 2 {
        print_short_usage();
        return ExitCode::from(1);
    }

    match parse_args() {
        Ok(Command::Status {
            output_format,
            db_path,
        }) => {
            if let Err(e) = run_status(db_path, output_format) {
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
                Ok(result) => {
                    match output_format {
                        OutputFormat::Json | OutputFormat::Pretty => {
                            let response = MigrateResponse {
                                success: result.success,
                                backup_path: result.backup_path.map(|p| p.to_string_lossy().to_string()),
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
                                    println!("Version: {} -> {}", result.old_version, result.new_version);
                                }
                                if let Some(ref backup) = result.backup_path {
                                    println!("Backup: {}", backup.display());
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
        Ok(Command::MigrateBackend {
            input_db,
            output_db,
            export_dir,
            dry_run,
            output_format,
        }) => {
            match magellan::migrate_backend_cmd::run_migrate_backend(input_db, output_db, export_dir, dry_run) {
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
                            if let Err(e) = output_json(&JsonResponse::new(json_data, &exec_id), output_format) {
                                eprintln!("Error: {}", e);
                                return ExitCode::from(1);
                            }
                        }
                        OutputFormat::Human => {
                            if result.success {
                                println!("{}", result.message);
                                println!("Format: {:?} -> {:?}", result.source_format, result.target_format);
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
        Ok(Command::GetFile { db_path, file_path }) => {
            if let Err(e) = get_cmd::run_get_file(db_path, file_path) {
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
            if let Err(e) = get_cmd::run_chunks(db_path, output_format, limit, file_filter, kind_filter) {
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
            if let Err(e) = get_cmd::run_chunk_by_span(db_path, file_path, byte_start, byte_end, output_format) {
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
            if let Err(e) = get_cmd::run_chunk_by_symbol(db_path, symbol_name, output_format, file_filter) {
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
        }) => {
            if let Err(e) = label_cmd::run_label(db_path, label, list, count, show_code) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Verify { root_path, db_path }) => {
            match verify_cmd::run_verify(root_path, db_path) {
                Ok(exit_code) => ExitCode::from(exit_code),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    ExitCode::from(1)
                }
            }
        }
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
            if let Err(e) =
                reachable_cmd::run_reachable(db_path, symbol_id, reverse, output_format)
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
            if let Err(e) =
                dead_code_cmd::run_dead_code(db_path, entry_symbol_id, output_format)
            {
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
            if let Err(e) =
                cycles_cmd::run_cycles(db_path, symbol_id, output_format)
            {
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
            if let Err(e) =
                condense_cmd::run_condense(db_path, show_members, output_format)
            {
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
            if let Err(e) = slice_cmd::run_slice(
                db_path,
                target,
                cli_direction,
                verbose,
                output_format,
            ) {
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
