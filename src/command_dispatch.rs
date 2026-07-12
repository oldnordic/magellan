use crate::cli::{Command, ContextSubcommand};
use crate::status_cmd::run_status;
use crate::{
    ask_cmd, ast_cmd, backfill_cmd, blast_score_cmd, candidate_fact_cmd, catalog_cmd,
    collisions_cmd, condense_cmd, config_cmd, context_cmd, cross_file_refs_cmd, cycles_cmd,
    cypher_cmd, dead_code_cmd, delete_cmd, doctor_cmd, embed_cmd, enrich_cmd, explore_cmd,
    export_cmd, features_cmd, files_cmd, find_cmd, get_cmd, hnsw_cmd, hook_cmd, hopgraph_cmd,
    import_lsif_cmd, index_cmd, ingest_coverage_cmd, init_cmd, label_cmd, navigate_cmd, orient_cmd,
    path_enumeration_cmd, project_metadata_cmd, query_cmd, reachable_cmd, refs_cmd, score_cmd,
    search_cmd, slice_cmd, source_inventory_cmd, telemetry_cmd, temporal_query_cmd,
    temporal_sweep_cmd, verify_cmd, watch_cmd,
};
use anyhow::Result;
use std::process::ExitCode;

fn exit_from_result(result: Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::Backfill { db_path } => exit_from_result(backfill_cmd::run_backfill(db_path)),
        Command::CrossFileRefs {
            db_path,
            fqn,
            output_format,
        } => exit_from_result(cross_file_refs_cmd::run_cross_file_refs(
            db_path,
            fqn,
            output_format,
        )),
        Command::Catalog { output_format } => {
            exit_from_result(catalog_cmd::run_catalog(output_format))
        }
        Command::CatalogDescribe {
            name,
            output_format,
        } => exit_from_result(catalog_cmd::run_catalog_describe(&name, output_format)),
        Command::Score {
            db,
            top,
            min_score,
            min_churn,
            min_complexity,
            min_lifetime,
            output,
        } => exit_from_result({
            let output = output.unwrap_or(magellan::OutputFormat::Human);
            score_cmd::run_score(
                &db,
                top,
                min_score,
                min_churn,
                min_complexity,
                min_lifetime,
                output,
            )
        }),
        Command::InstallHook { threshold, strict } => {
            exit_from_result(hook_cmd::run_install_hook(threshold, strict))
        }
        Command::ConfigShow { output_format } => {
            exit_from_result(config_cmd::run_config_show(output_format))
        }
        Command::ConfigInit { force } => exit_from_result(config_cmd::run_config_init(force)),
        Command::ProjectInit { path } => exit_from_result(init_cmd::run_project_init(path)),
        Command::Delete {
            db_path,
            file_path,
            root,
        } => exit_from_result(delete_cmd::run_delete(db_path, file_path, root)),
        Command::Index {
            db_path,
            file_path,
            root,
        } => exit_from_result(index_cmd::run_index(db_path, file_path, root)),
        Command::Status {
            output_format,
            db_path,
            all,
            ..
        } => exit_from_result(run_status(db_path, output_format, all)),
        Command::ProjectMetadata {
            db_path,
            query,
            output_format,
        } => exit_from_result(project_metadata_cmd::run_project_metadata(
            db_path,
            query,
            output_format,
        )),
        Command::Export {
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
            impact_symbol,
            impact_file,
            impact_depth,
        } => exit_from_result(export_cmd::run_export(
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
            impact_symbol,
            impact_file,
            impact_depth,
        )),
        Command::ImportLsif {
            db_path,
            lsif_paths,
        } => exit_from_result(import_lsif_cmd::run_import_lsif(db_path, lsif_paths)),
        Command::IngestCoverage { db_path, lcov_path } => {
            exit_from_result(ingest_coverage_cmd::run_ingest_coverage(db_path, lcov_path))
        }
        Command::Enrich {
            db_path,
            files,
            timeout_secs,
        } => exit_from_result(enrich_cmd::run_enrich(db_path, files, timeout_secs)),
        Command::Context {
            subcommand,
            db_paths,
        } => {
            let result = match subcommand {
                ContextSubcommand::Build => context_cmd::run_context_build(db_paths),
                ContextSubcommand::Summary { detail, concise } => {
                    context_cmd::run_context_summary(db_paths, None, detail, concise)
                }
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
                    detail,
                    concise,
                    tokens,
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
                    detail,
                    concise,
                    tokens,
                ),
                ContextSubcommand::File { path } => context_cmd::run_context_file(db_paths, path),
                ContextSubcommand::Impact {
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                    detail,
                    concise,
                    tokens,
                } => context_cmd::run_context_impact(
                    db_paths,
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                    detail,
                    concise,
                    tokens,
                ),
                ContextSubcommand::Affected {
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                    detail,
                    concise,
                    tokens,
                } => context_cmd::run_context_affected(
                    db_paths,
                    symbol,
                    file,
                    depth,
                    project,
                    output_format,
                    detail,
                    concise,
                    tokens,
                ),
            };
            if let Err(e) = result {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Command::Doctor {
            db_path,
            fix,
            output_format,
        } => exit_from_result(doctor_cmd::run_doctor(db_path, fix, output_format)),
        Command::Query {
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
        } => exit_from_result(query_cmd::run_query(
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
        )),
        Command::Search {
            db_path,
            pattern,
            limit,
            output_format,
        } => exit_from_result(search_cmd::run_search(
            db_path,
            pattern,
            limit,
            output_format,
        )),
        Command::Find {
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
        } => exit_from_result(find_cmd::run_find(
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
        )),
        Command::Refs {
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
            tokens,
        } => exit_from_result(refs_cmd::run_refs(
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
            tokens,
        )),
        Command::Files {
            db_path,
            output_format,
            with_symbols,
        } => exit_from_result(files_cmd::run_files(db_path, with_symbols, output_format)),
        Command::Collisions {
            db_path,
            field,
            limit,
            output_format,
        } => exit_from_result(collisions_cmd::run_collisions(
            db_path,
            field,
            limit,
            output_format,
        )),
        Command::Migrate {
            db_path,
            dry_run,
            no_backup,
            output_format,
        } => crate::command_special::handle_migrate(db_path, dry_run, no_backup, output_format),
        Command::MigrateBackend {
            input_db,
            output_db,
            export_dir,
            dry_run,
            output_format,
        } => crate::command_special::handle_migrate_backend(
            input_db,
            output_db,
            export_dir,
            dry_run,
            output_format,
        ),
        Command::Get {
            db_path,
            file_path,
            symbol_name,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        } => exit_from_result(get_cmd::run_get(
            db_path,
            file_path,
            symbol_name,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        )),
        Command::GetFile {
            db_path,
            file_path,
            output_format,
        } => exit_from_result(get_cmd::run_get_file(db_path, file_path, output_format)),
        Command::TemporalSweep {
            db_path,
            repo_path,
            every_n,
            tags_only,
            merge_commits_only,
            since_commit_time,
            until_commit_time,
            output_format,
        } => exit_from_result(temporal_sweep_cmd::run_temporal_sweep(
            db_path,
            repo_path,
            magellan::temporal::worktrees::TemporalSweepSelection {
                every_n,
                tags_only,
                merge_commits_only,
                since_commit_time,
                until_commit_time,
            },
            output_format,
        )),
        Command::TemporalStatus {
            db_path,
            output_format,
        } => exit_from_result(temporal_query_cmd::run_temporal_status(
            db_path,
            output_format,
        )),
        Command::TemporalBarcode {
            db_path,
            stable_id,
            edge_source,
            edge_target,
            edge_kind,
            scc,
            output_format,
        } => exit_from_result(temporal_query_cmd::run_temporal_barcode(
            db_path,
            stable_id,
            edge_source,
            edge_target,
            edge_kind,
            scc,
            output_format,
        )),
        Command::AsOf {
            db_path,
            commit_oid,
            symbol_name,
            output_format,
        } => exit_from_result(temporal_query_cmd::run_as_of(
            db_path,
            commit_oid,
            symbol_name,
            output_format,
        )),
        Command::Orient {
            db_path,
            repo_path,
            top_n,
            output_format,
        } => exit_from_result(orient_cmd::run_orient(
            db_path,
            repo_path,
            top_n,
            output_format,
        )),
        Command::Chunks {
            db_path,
            output_format,
            limit,
            file_filter,
            kind_filter,
        } => exit_from_result(get_cmd::run_chunks(
            db_path,
            output_format,
            limit,
            file_filter,
            kind_filter,
        )),
        Command::ChunkBySpan {
            db_path,
            file_path,
            byte_start,
            byte_end,
            output_format,
        } => exit_from_result(get_cmd::run_chunk_by_span(
            db_path,
            file_path,
            byte_start,
            byte_end,
            output_format,
        )),
        Command::ChunkBySymbol {
            db_path,
            symbol_name,
            output_format,
            file_filter,
        } => exit_from_result(get_cmd::run_chunk_by_symbol(
            db_path,
            symbol_name,
            output_format,
            file_filter,
        )),
        Command::Label {
            db_path,
            label,
            list,
            count,
            show_code,
            output_format,
        } => exit_from_result(label_cmd::run_label(
            db_path,
            label,
            list,
            count,
            show_code,
            output_format,
        )),
        Command::Verify {
            root_path,
            db_path,
            output_format,
        } => match verify_cmd::run_verify(root_path, db_path, output_format) {
            Ok(exit_code) => ExitCode::from(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                ExitCode::from(1)
            }
        },
        Command::Watch {
            root_path,
            db_path,
            config,
            scan_initial,
            validate,
            validate_only,
            compile_commands,
        } => exit_from_result(watch_cmd::run_watch(
            root_path,
            db_path,
            config,
            scan_initial,
            validate,
            validate_only,
            compile_commands,
        )),
        Command::Ast {
            db_path,
            file_path,
            position,
            output_format,
        } => exit_from_result(ast_cmd::run_ast_command(
            db_path,
            file_path,
            position,
            output_format,
        )),
        Command::FindAst {
            db_path,
            kind,
            output_format,
        } => exit_from_result(ast_cmd::run_find_ast_command(db_path, kind, output_format)),
        Command::Reachable {
            db_path,
            symbol_id,
            reverse,
            output_format,
        } => exit_from_result(reachable_cmd::run_reachable(
            db_path,
            symbol_id,
            reverse,
            output_format,
        )),
        Command::DeadCode {
            db_path,
            entry_symbol_id,
            output_format,
        } => exit_from_result(dead_code_cmd::run_dead_code(
            db_path,
            entry_symbol_id,
            output_format,
        )),
        Command::Paths {
            db_path,
            start_symbol_id,
            end_symbol_id,
            max_depth,
            max_paths,
            output_format,
        } => exit_from_result(path_enumeration_cmd::run_paths(
            db_path,
            start_symbol_id,
            end_symbol_id,
            max_depth,
            max_paths,
            output_format,
        )),
        Command::Cycles {
            db_path,
            symbol_id,
            output_format,
        } => exit_from_result(cycles_cmd::run_cycles(db_path, symbol_id, output_format)),
        Command::Condense {
            db_path,
            show_members,
            output_format,
        } => exit_from_result(condense_cmd::run_condense(
            db_path,
            show_members,
            output_format,
        )),
        Command::Slice {
            db_path,
            target,
            direction,
            verbose,
            output_format,
        } => {
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
        Command::Refresh {
            db_path: raw_db_path,
            dry_run,
            include_untracked,
            staged,
            unstaged,
            force,
            output_format,
        } => crate::command_special::handle_refresh(
            raw_db_path,
            dry_run,
            include_untracked,
            staged,
            unstaged,
            force,
            output_format,
        ),
        Command::SourceInventory {
            db_path,
            scan_dirs,
            list_kind,
            show_stale,
            output_format,
        } => exit_from_result(source_inventory_cmd::run_source_inventory(
            db_path,
            scan_dirs,
            list_kind,
            show_stale,
            output_format,
        )),
        Command::CandidateFact {
            db_path,
            action,
            output_format,
        } => exit_from_result(candidate_fact_cmd::run_candidate_fact(
            db_path,
            action,
            output_format,
        )),
        Command::Service {
            action,
            output_format,
        } => crate::command_special::handle_service(action, output_format),
        Command::ServiceDaemon => crate::command_special::handle_service_daemon(),
        Command::Cypher {
            db_path,
            query,
            output_format,
        } => exit_from_result(cypher_cmd::run_cypher(db_path, query, output_format)),
        Command::HnswCreate {
            db_path,
            name,
            dim,
            m,
            ef_construction,
            ef_search,
            output_format,
        } => exit_from_result(hnsw_cmd::run_hnsw_create(
            db_path,
            name,
            dim,
            m,
            ef_construction,
            ef_search,
            output_format,
        )),
        Command::HnswQuery {
            db_path,
            name,
            vector,
            k,
            output_format,
        } => exit_from_result(hnsw_cmd::run_hnsw_query(
            db_path,
            name,
            vector,
            k,
            output_format,
        )),
        Command::Ask {
            question,
            db_path,
            output_format,
            all,
        } => exit_from_result(ask_cmd::run_ask(question, db_path, all, output_format)),
        Command::BlastScore {
            db_path,
            symbol,
            file,
            depth,
            output_format,
        } => exit_from_result(blast_score_cmd::run_blast_score(
            db_path,
            symbol,
            file,
            depth,
            output_format,
        )),
        Command::Navigate {
            task,
            db_path,
            output_format,
            depth,
            budget,
            limit,
            concise,
            with_llmgrep,
            with_mirage,
            tokens,
        } => exit_from_result({
            let cfg = navigate_cmd::NavigateConfig {
                db_path,
                task,
                output_format,
                depth,
                budget,
                limit,
                concise,
                with_llmgrep,
                with_mirage,
                tokens,
            };
            navigate_cmd::run_navigate(cfg)
        }),
        Command::Explore {
            db_path,
            symbol,
            id,
            edges,
            callers,
            callees,
            chain,
            depth,
            json,
        } => exit_from_result({
            let cfg = explore_cmd::ExploreConfig {
                db_path,
                symbol,
                id,
                edges,
                callers,
                callees,
                chain,
                depth,
                format: if json {
                    explore_cmd::OutputFormat::Json
                } else {
                    explore_cmd::OutputFormat::Human
                },
            };
            explore_cmd::run_explore(cfg)
        }),
        Command::Telemetry {
            db_path,
            recent,
            phases,
            limit,
            output_format,
        } => exit_from_result(telemetry_cmd::run_telemetry(
            db_path,
            recent,
            phases,
            limit,
            output_format,
        )),
        Command::Features {
            db_path,
            output_format,
        } => exit_from_result(features_cmd::run_features(db_path, output_format)),
        Command::Hopgraph {
            db_path,
            query,
            k,
            hops,
            output_format,
        } => exit_from_result(hopgraph_cmd::run_hopgraph(
            db_path,
            query,
            k,
            hops,
            output_format,
        )),
        Command::Embed {
            db_path,
            force,
            batch_size,
            num_parallel,
            output_format,
        } => exit_from_result(embed_cmd::run_embed(
            db_path,
            force,
            batch_size,
            num_parallel,
            output_format,
        )),
    }
}
