//! Context command implementation
//!
//! Provides summarized, paginated context queries for LLMs.

use anyhow::Result;
use magellan::context::{build_context_index, ListQuery};
use magellan::graph::multi_db::MultiDbContext;
use magellan::output::{generate_execution_id, ContextResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the context build command (multi-DB)
pub fn run_context_build(db_paths: Vec<PathBuf>) -> Result<()> {
    let exec_id = generate_execution_id();

    for db_path in &db_paths {
        match CodeGraph::open(db_path) {
            Ok(mut graph) => {
                graph
                    .telemetry()
                    .record_phase_start(&exec_id, "build_context")?;
                if let Err(e) = build_context_index(&mut graph, db_path) {
                    eprintln!(
                        "Warning: failed to build index for {}: {}",
                        db_path.display(),
                        e
                    );
                }
                let _ = graph
                    .telemetry()
                    .record_phase_end(&exec_id, "build_context");
            }
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", db_path.display(), e);
            }
        }
    }
    Ok(())
}

/// Run the context summary command (multi-DB)
pub fn run_context_summary(db_paths: Vec<PathBuf>) -> Result<()> {
    let exec_id = generate_execution_id();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: query_summary
    mdb.telemetry()
        .record_phase_start(&exec_id, "query_summary")?;
    let summaries = mdb.summaries();
    mdb.telemetry()
        .record_phase_end(&exec_id, "query_summary")?;

    // Phase: output
    mdb.telemetry().record_phase_start(&exec_id, "output")?;

    for (project, summary) in &summaries {
        println!("Project: {} {}", summary.name, summary.version);
        println!("Language: {}", summary.language);
        println!("Files: {}", summary.total_files);
        println!("Symbols: {}", summary.total_symbols);
        println!();
        println!("Symbol Breakdown:");
        println!("  Functions: {}", summary.symbol_counts.functions);
        println!("  Methods: {}", summary.symbol_counts.methods);
        println!("  Structs: {}", summary.symbol_counts.structs);
        println!("  Traits: {}", summary.symbol_counts.traits);
        println!("  Enums: {}", summary.symbol_counts.enums);
        println!("  Modules: {}", summary.symbol_counts.modules);

        if !summary.entry_points.is_empty() {
            println!();
            println!("Entry Points:");
            for entry in &summary.entry_points {
                println!("  - {}", entry);
            }
        }

        println!("Project ID: {}", project);
        println!("---");
    }

    // End output phase
    mdb.telemetry().record_phase_end(&exec_id, "output")?;

    Ok(())
}

/// Run the context list command (multi-DB)
pub fn run_context_list(
    db_paths: Vec<PathBuf>,
    kind: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
    _cursor: Option<String>,
    project_filter: Option<String>,
    output_format: OutputFormat,
) -> Result<()> {
    if db_paths.is_empty() {
        anyhow::bail!("No database paths provided. Use --db <path> to specify.");
    }

    let exec_id = generate_execution_id();
    let query = ListQuery {
        kind,
        page: None,
        page_size: None,
        cursor: None,
        file_pattern: None,
    };

    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: query_list
    mdb.telemetry().record_phase_start(&exec_id, "query_list")?;
    let mut all_items = mdb.list_symbols(&query);
    mdb.telemetry().record_phase_end(&exec_id, "query_list")?;

    // Post-filter by --project
    if let Some(ref filter) = project_filter {
        all_items.retain(|(proj, _)| proj == filter);
    }

    let total_items = all_items.len();
    let page_num = page.unwrap_or(1);
    let size = page_size.unwrap_or(50);

    // Sort by project then name for consistent output
    all_items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));

    let total_pages = total_items.div_ceil(size);
    let start = ((page_num.saturating_sub(1)) * size).min(total_items);
    let end = (start + size).min(total_items);
    let page_items: Vec<_> = all_items[start..end].to_vec();

    // Output
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            // Phase: build_response
            mdb.telemetry()
                .record_phase_start(&exec_id, "build_response")?;

            let items_json: Vec<serde_json::Value> = page_items
                .iter()
                .map(|(proj, item)| {
                    serde_json::json!({
                        "project": proj,
                        "name": item.name,
                        "kind": item.kind,
                        "file": item.file,
                        "line": item.line,
                    })
                })
                .collect();
            let data = serde_json::json!({
                "page": page_num,
                "total_pages": total_pages,
                "total_items": total_items,
                "matches": items_json,
            });
            let response = serde_json::json!({
                "schema_version": "1.0",
                "execution_id": generate_execution_id(),
                "command": "context list",
                "data": data,
            });
            let formatted = if matches!(output_format, OutputFormat::Pretty) {
                serde_json::to_string_pretty(&response)?
            } else {
                serde_json::to_string(&response)?
            };
            println!("{}", formatted);

            mdb.telemetry()
                .record_phase_end(&exec_id, "build_response")?;
        }
        OutputFormat::Human => {
            // Phase: output
            mdb.telemetry().record_phase_start(&exec_id, "output")?;

            println!(
                "Page {} of {} ({} total symbols across {} projects)",
                page_num,
                total_pages,
                total_items,
                db_paths.len()
            );
            println!();

            let mut last_project = String::new();
            for (proj, item) in &page_items {
                if *proj != last_project {
                    if !last_project.is_empty() {
                        println!();
                    }
                    println!("Project: {}", proj);
                    last_project = proj.clone();
                }
                println!(
                    "  {}:{}  {}  ({})",
                    item.file, item.line, item.name, item.kind
                );
            }

            if page_num < total_pages {
                println!();
                println!("Next page: --page {}", page_num + 1);
            }

            // End output phase
            mdb.telemetry().record_phase_end(&exec_id, "output")?;
        }
    }

    Ok(())
}

/// Run the context symbol command
#[allow(
    clippy::too_many_arguments,
    reason = "CLI command surface: each arg maps to a flag"
)]
pub fn run_context_symbol(
    db_paths: Vec<PathBuf>,
    name: String,
    file: Option<String>,
    include_callers: bool,
    include_callees: bool,
    output_format: OutputFormat,
    with_source: bool,
    depth: Option<usize>,
    project_filter: Option<String>,
) -> Result<()> {
    if db_paths.is_empty() {
        anyhow::bail!("No database paths provided. Use --db <path> to specify.");
    }

    let exec_id = generate_execution_id();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: search_symbol
    mdb.telemetry()
        .record_phase_start(&exec_id, "search_symbol")?;
    let mut all_matches = mdb.search_symbol(
        &name,
        file.as_deref(),
        depth,
        include_callers,
        include_callees,
    );
    mdb.telemetry()
        .record_phase_end(&exec_id, "search_symbol")?;

    // Post-filter by --project
    if let Some(ref filter) = project_filter {
        all_matches.retain(|m| m.project == *filter);
    }

    // Add source snippets if requested
    if with_source {
        for m in &mut all_matches {
            m.source = read_source_lines(&m.span.file_path, m.span.start_line, m.span.end_line);
        }
    }

    let project_names: Vec<String> = all_matches.iter().map(|m| m.project.clone()).collect();

    if all_matches.is_empty() {
        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = ContextResponse {
                    query: name.clone(),
                    projects: vec![],
                    matches: vec![],
                };
                let exec_id = generate_execution_id();
                let json_response = magellan::output::JsonResponse::new(response, &exec_id);
                magellan::output::output_json(&json_response, output_format)?;
            }
            OutputFormat::Human => {
                eprintln!("Error: Symbol '{}' not found", name);
                eprintln!();
                eprintln!("No exact matches.");
            }
        }
        return Ok(());
    }
    // Output results
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            // Phase: build_response
            mdb.telemetry()
                .record_phase_start(&exec_id, "build_response")?;

            let response = ContextResponse {
                query: name.clone(),
                projects: project_names,
                matches: all_matches,
            };
            let exec_id = generate_execution_id();
            let json_response = magellan::output::JsonResponse::new(response, &exec_id);
            magellan::output::output_json(&json_response, output_format)?;

            mdb.telemetry()
                .record_phase_end(&exec_id, "build_response")?;
        }
        OutputFormat::Human => {
            // Phase: output
            mdb.telemetry().record_phase_start(&exec_id, "output")?;

            for (i, m) in all_matches.iter().enumerate() {
                if i > 0 {
                    println!();
                    println!("---");
                }
                println!("Project: {}", m.project);
                println!("Symbol: {}", m.name);
                println!("Kind: {}", m.kind);
                println!("File: {}:{}", m.span.file_path, m.span.start_line);

                if let Some(ref callers) = m.callers {
                    if !callers.is_empty() {
                        println!();
                        println!("Callers ({}):", callers.len());
                        for c in callers {
                            let depth_str =
                                c.depth.map_or(String::new(), |d| format!("[depth={}]", d));
                            println!("  - {} ({}:{}) {}", c.name, c.file_path, c.line, depth_str);
                        }
                    }
                }

                if let Some(ref callees) = m.callees {
                    if !callees.is_empty() {
                        println!();
                        println!("Callees ({}):", callees.len());
                        for c in callees {
                            let depth_str =
                                c.depth.map_or(String::new(), |d| format!("[depth={}]", d));
                            println!("  - {} ({}:{}) {}", c.name, c.file_path, c.line, depth_str);
                        }
                    }
                }

                if let Some(ref source) = m.source {
                    println!();
                    println!(
                        "Source ({}:{}-{}):",
                        m.span.file_path, m.span.start_line, m.span.end_line
                    );
                    for line in source.lines() {
                        println!("  {}", line);
                    }
                }
            }

            // End output phase
            mdb.telemetry().record_phase_end(&exec_id, "output")?;
        }
    }

    Ok(())
}

/// Run the context file command
pub fn run_context_file(db_paths: Vec<PathBuf>, path: String) -> Result<()> {
    let exec_id = generate_execution_id();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: query_file
    mdb.telemetry().record_phase_start(&exec_id, "query_file")?;
    let results = mdb.file_context(&path);
    mdb.telemetry().record_phase_end(&exec_id, "query_file")?;

    // Phase: output
    mdb.telemetry().record_phase_start(&exec_id, "output")?;

    if results.is_empty() {
        println!("File '{}' not found in any project.", path);
        return Ok(());
    }

    for (project, context) in &results {
        println!("Project: {}", project);
        println!("File: {}", context.path);
        println!("Language: {}", context.language);
        println!("Symbols: {}", context.symbol_count);
        println!();
        println!("Symbol Breakdown:");
        println!("  Functions: {}", context.symbol_counts.functions);
        println!("  Methods: {}", context.symbol_counts.methods);
        println!("  Structs: {}", context.symbol_counts.structs);
        println!("  Traits: {}", context.symbol_counts.traits);
        println!("  Enums: {}", context.symbol_counts.enums);
        println!();
        println!("Public Symbols:");
        for symbol in &context.public_symbols {
            println!("  - {}", symbol);
        }

        if !context.imports.is_empty() {
            println!();
            println!("Imports:");
            for import in &context.imports {
                println!("  - {}", import);
            }
        }
        println!("---");
    }

    // End output phase
    mdb.telemetry().record_phase_end(&exec_id, "output")?;

    Ok(())
}

/// Read source lines from a file, returning them as a single string.
///
/// Uses 1-indexed line numbers. If the file can't be read or lines are
/// out of range, returns None.
fn read_source_lines(file_path: &str, start_line: usize, end_line: usize) -> Option<String> {
    use std::fs;
    use std::io::BufRead;

    let file = fs::File::open(file_path).ok()?;
    let reader = std::io::BufReader::new(file);

    let lines: Vec<String> = reader
        .lines()
        .skip(start_line.saturating_sub(1))
        .take(end_line.saturating_sub(start_line) + 1)
        .filter_map(|l| l.ok())
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(lines.join("\n"))
}

/// Run the context impact command (multi-DB)
pub fn run_context_impact(
    db_paths: Vec<PathBuf>,
    symbol_name: String,
    file: Option<String>,
    depth: usize,
    project_filter: Option<String>,
    output_format: OutputFormat,
) -> Result<()> {
    if db_paths.is_empty() {
        anyhow::bail!("No database paths provided. Use --db <path> to specify.");
    }

    let exec_id = generate_execution_id();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: query_impact
    mdb.telemetry()
        .record_phase_start(&exec_id, "query_impact")?;
    let mut all_impacted = mdb.impact(&symbol_name, file.as_deref(), depth);
    mdb.telemetry().record_phase_end(&exec_id, "query_impact")?;

    if let Some(ref filter) = project_filter {
        all_impacted.retain(|(proj, _)| proj == filter);
    }

    if all_impacted.is_empty() {
        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let target = format!(
                    "{}{}",
                    symbol_name,
                    file.as_ref()
                        .map(|f| format!(" (in {})", f))
                        .unwrap_or_default()
                );
                let response = serde_json::json!({
                    "schema_version": "1.0",
                    "execution_id": generate_execution_id(),
                    "command": "context impact",
                    "data": {
                        "target": target,
                        "depth_limit": depth,
                        "total_impacted": 0,
                        "impacted": [],
                    },
                });
                let formatted = if matches!(output_format, OutputFormat::Pretty) {
                    serde_json::to_string_pretty(&response)?
                } else {
                    serde_json::to_string(&response)?
                };
                println!("{}", formatted);
            }
            OutputFormat::Human => {
                println!("No impact found for symbol '{}'.", symbol_name);
                if file.is_some() {
                    println!("(Try without --file, or check the symbol exists in the index.)");
                }
            }
        }
        return Ok(());
    }

    let target = format!(
        "{}{}",
        symbol_name,
        file.as_ref()
            .map(|f| format!(" (in {})", f))
            .unwrap_or_default()
    );

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            // Phase: build_response
            mdb.telemetry()
                .record_phase_start(&exec_id, "build_response")?;

            let impacted_json: Vec<serde_json::Value> = all_impacted
                .iter()
                .map(|(proj, r)| {
                    serde_json::json!({
                        "project": proj,
                        "name": r.name,
                        "file": r.file,
                        "line": r.line,
                        "depth": r.depth,
                    })
                })
                .collect();
            let response = serde_json::json!({
                "schema_version": "1.0",
                "execution_id": generate_execution_id(),
                "command": "context impact",
                "data": {
                    "target": target,
                    "depth_limit": depth,
                    "total_impacted": all_impacted.len(),
                    "impacted": impacted_json,
                },
            });
            let formatted = if matches!(output_format, OutputFormat::Pretty) {
                serde_json::to_string_pretty(&response)?
            } else {
                serde_json::to_string(&response)?
            };
            println!("{}", formatted);

            mdb.telemetry()
                .record_phase_end(&exec_id, "build_response")?;
        }
        OutputFormat::Human => {
            // Phase: output
            mdb.telemetry().record_phase_start(&exec_id, "output")?;

            println!("Impact analysis: {} (depth limit: {})", target, depth);
            println!(
                "{} symbol(s) affected across {} DB(s)\n",
                all_impacted.len(),
                db_paths.len()
            );

            let mut last_project = String::new();
            for (proj, r) in &all_impacted {
                if *proj != last_project {
                    if !last_project.is_empty() {
                        println!();
                    }
                    println!("Project: {}", proj);
                    last_project = proj.clone();
                }
                let depth_str = r.depth.map_or(String::new(), |d| format!(" [depth={}]", d));
                println!("  {} ({}:{}){}", r.name, r.file, r.line, depth_str);
            }

            // End output phase
            mdb.telemetry().record_phase_end(&exec_id, "output")?;
        }
    }

    Ok(())
}

/// Run the context affected command (multi-DB)
/// Forward reachability: find all symbols the target transitively calls
pub fn run_context_affected(
    db_paths: Vec<PathBuf>,
    symbol_name: String,
    file: Option<String>,
    depth: usize,
    project_filter: Option<String>,
    output_format: OutputFormat,
) -> Result<()> {
    if db_paths.is_empty() {
        anyhow::bail!("No database paths provided. Use --db <path> to specify.");
    }

    let exec_id = generate_execution_id();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    // Phase: query_affected
    mdb.telemetry()
        .record_phase_start(&exec_id, "query_affected")?;
    let mut all_affected = mdb.affected(&symbol_name, file.as_deref(), depth);
    mdb.telemetry()
        .record_phase_end(&exec_id, "query_affected")?;

    if let Some(ref filter) = project_filter {
        all_affected.retain(|(proj, _)| proj == filter);
    }

    let target = format!(
        "{}{}",
        symbol_name,
        file.as_ref()
            .map(|f| format!(" (in {})", f))
            .unwrap_or_default()
    );

    if all_affected.is_empty() {
        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = serde_json::json!({
                    "schema_version": "1.0",
                    "execution_id": generate_execution_id(),
                    "command": "context affected",
                    "data": {
                        "target": target,
                        "depth_limit": depth,
                        "total_affected": 0,
                        "affected": [],
                    },
                });
                let formatted = if matches!(output_format, OutputFormat::Pretty) {
                    serde_json::to_string_pretty(&response)?
                } else {
                    serde_json::to_string(&response)?
                };
                println!("{}", formatted);
            }
            OutputFormat::Human => {
                println!("No dependencies found for symbol '{}'.", symbol_name);
            }
        }
        return Ok(());
    }

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            // Phase: build_response
            mdb.telemetry()
                .record_phase_start(&exec_id, "build_response")?;

            let affected_json: Vec<serde_json::Value> = all_affected
                .iter()
                .map(|(proj, r)| {
                    serde_json::json!({
                        "project": proj,
                        "name": r.name,
                        "file": r.file,
                        "line": r.line,
                        "depth": r.depth,
                    })
                })
                .collect();
            let response = serde_json::json!({
                "schema_version": "1.0",
                "execution_id": generate_execution_id(),
                "command": "context affected",
                "data": {
                    "target": target,
                    "depth_limit": depth,
                    "total_affected": all_affected.len(),
                    "affected": affected_json,
                },
            });
            let formatted = if matches!(output_format, OutputFormat::Pretty) {
                serde_json::to_string_pretty(&response)?
            } else {
                serde_json::to_string(&response)?
            };
            println!("{}", formatted);

            mdb.telemetry()
                .record_phase_end(&exec_id, "build_response")?;
        }
        OutputFormat::Human => {
            // Phase: output
            mdb.telemetry().record_phase_start(&exec_id, "output")?;

            println!("Affected analysis: {} (depth limit: {})", target, depth);
            println!(
                "{} symbol(s) reached across {} DB(s)\n",
                all_affected.len(),
                db_paths.len()
            );

            let mut last_project = String::new();
            for (proj, r) in &all_affected {
                if *proj != last_project {
                    if !last_project.is_empty() {
                        println!();
                    }
                    println!("Project: {}", proj);
                    last_project = proj.clone();
                }
                let depth_str = r.depth.map_or(String::new(), |d| format!(" [depth={}]", d));
                println!("  {} ({}:{}){}", r.name, r.file, r.line, depth_str);
            }

            // End output phase
            mdb.telemetry().record_phase_end(&exec_id, "output")?;
        }
    }

    Ok(())
}
