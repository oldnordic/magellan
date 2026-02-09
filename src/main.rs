//! Magellan CLI - Dumb, deterministic codebase mapping tool
//!
//! Usage: magellan <command> [arguments]

mod ast_cmd;
mod collisions_cmd;
mod dead_code_cmd;
mod export_cmd;
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
mod version;
mod cli;

use anyhow::Result;
use magellan::output::{generate_execution_id, output_json, JsonResponse, MigrateResponse, OutputFormat, StatusResponse};
use serde_json;
use magellan::CodeGraph;
use std::path::PathBuf;
use std::process::ExitCode;

use cli::{Command, parse_args};


fn print_usage() {
    cli::print_usage();
}
/// Handles both success and error outcomes.
struct ExecutionTracker {
    exec_id: String,
    tool_version: String,
    args: Vec<String>,
    root: Option<String>,
    db_path: String,
    outcome: String,
    error_message: Option<String>,
    files_indexed: usize,
    symbols_indexed: usize,
    references_indexed: usize,
}

impl ExecutionTracker {
    fn new(args: Vec<String>, root: Option<String>, db_path: String) -> Self {
        Self {
            exec_id: generate_execution_id(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            args,
            root,
            db_path,
            outcome: "success".to_string(),
            error_message: None,
            files_indexed: 0,
            symbols_indexed: 0,
            references_indexed: 0,
        }
    }

    fn start(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().start_execution(
            &self.exec_id,
            &self.tool_version,
            &self.args,
            self.root.as_deref(),
            &self.db_path,
        )?;
        Ok(())
    }

    fn finish(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().finish_execution(
            &self.exec_id,
            &self.outcome,
            self.error_message.as_deref(),
            self.files_indexed,
            self.symbols_indexed,
            self.references_indexed,
        )
    }

    /// Set execution outcome to error with message
    ///
    /// Currently unused but provided for API completeness and future error handling.
    #[expect(dead_code)] // API completeness for future error handling
    fn set_error(&mut self, msg: String) {
        self.outcome = "error".to_string();
        self.error_message = Some(msg);
    }

    /// Set indexing counts for execution tracking
    ///
    /// Currently unused but provided for API completeness and future tracking.
    #[expect(dead_code)] // API completeness for future tracking
    fn set_counts(&mut self, files: usize, symbols: usize, references: usize) {
        self.files_indexed = files;
        self.symbols_indexed = symbols;
        self.references_indexed = references;
    }

    fn exec_id(&self) -> &str {
        &self.exec_id
    }
}

fn run_status(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let tracker = ExecutionTracker::new(
        vec!["status".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;
    let call_count = graph.count_calls()?;
    let chunk_count = graph.count_chunks()?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = StatusResponse {
                files: file_count,
                symbols: symbol_count,
                references: reference_count,
                calls: call_count,
                code_chunks: chunk_count,
            };
            let exec_id = tracker.exec_id().to_string();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!("files: {}", file_count);
            println!("symbols: {}", symbol_count);
            println!("references: {}", reference_count);
            println!("calls: {}", call_count);
            println!("code_chunks: {}", chunk_count);
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

/// Run label query command
/// Usage: magellan label --db <FILE> --label <LABEL> [--list] [--count] [--show-code]
///
/// # Feature Availability
/// Label queries require SQLite backend (not available with native-v2)
#[cfg(not(feature = "native-v2"))]
fn run_label(
    db_path: PathBuf,
    labels: Vec<String>,
    list: bool,
    count: bool,
    show_code: bool,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let mut args = vec!["label".to_string()];
    for label in &labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }
    if list {
        args.push("--list".to_string());
    }
    if count {
        args.push("--count".to_string());
    }
    if show_code {
        args.push("--show-code".to_string());
    }

    let tracker = ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;

    // List all labels mode
    if list {
        let all_labels = graph.get_all_labels()?;
        println!("{} labels in use:", all_labels.len());
        for label in all_labels {
            let count = graph.count_entities_by_label(&label)?;
            println!("  {} ({})", label, count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Count mode
    if count {
        if labels.is_empty() {
            tracker.finish(&graph)?;
            return Err(anyhow::anyhow!("--count requires --label"));
        }
        for label in &labels {
            let entity_count = graph.count_entities_by_label(label)?;
            println!("{}: {} entities", label, entity_count);
        }
        tracker.finish(&graph)?;
        return Ok(());
    }

    // Query mode - get symbols by label(s)
    if labels.is_empty() {
        tracker.finish(&graph)?;
        return Err(anyhow::anyhow!(
            "No labels specified. Use --label <LABEL> or --list to see all labels"
        ));
    }

    let labels_ref: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let results = if labels.len() == 1 {
        graph.get_symbols_by_label(&labels[0])?
    } else {
        graph.get_symbols_by_labels(&labels_ref)?
    };

    if results.is_empty() {
        if labels.len() == 1 {
            println!("No symbols found with label '{}'", labels[0]);
        } else {
            println!("No symbols found with labels: {}", labels.join(", "));
        }
    } else {
        if labels.len() == 1 {
            println!("{} symbols with label '{}':", results.len(), labels[0]);
        } else {
            println!(
                "{} symbols with labels [{}]:",
                results.len(),
                labels.join(", ")
            );
        }

        for result in results {
            println!();
            println!(
                "  {} ({}) in {} [{}-{}]",
                result.name, result.kind, result.file_path, result.byte_start, result.byte_end
            );

            // Show code chunk if requested
            if show_code {
                // Get code chunk by exact byte span instead of by name
                // This avoids getting chunks for other symbols with the same name
                if let Ok(Some(chunk)) = graph.get_code_chunk_by_span(
                    &result.file_path,
                    result.byte_start,
                    result.byte_end,
                ) {
                    for line in chunk.content.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

/// Run label query command (native-v2 variant - not supported)
///
/// # Feature Availability
/// Label queries are not supported with native-v2 backend
#[cfg(feature = "native-v2")]
fn run_label(
    _db_path: PathBuf,
    _labels: Vec<String>,
    _list: bool,
    _count: bool,
    _show_code: bool,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Label queries are not supported with the native-v2 backend. \
         Label queries depend on SQLite's graph_labels table which doesn't exist in Native V2."
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
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
            if let Err(e) = run_label(db_path, label, list, count, show_code) {
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
                    return ExitCode::from(1);
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
            print_usage();
            ExitCode::from(1)
        }
    }
}
