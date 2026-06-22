//! Context command implementation
//!
//! Provides summarized, paginated context queries for LLMs.

use anyhow::Result;
use magellan::context::{build_context_index, ListQuery, SymbolRelation};
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
pub fn run_context_summary(
    db_paths: Vec<PathBuf>,
    tokens: Option<usize>,
    detail: Option<String>,
    concise: bool,
) -> Result<()> {
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

    let limits = OutputLimits::new(&detail, concise);
    let output = prune_and_format_summary_response(summaries, &limits, tokens)?;
    print!("{}", output);

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
    detail: Option<String>,
    concise: bool,
    tokens: Option<usize>,
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
    let phase = match output_format {
        OutputFormat::Json | OutputFormat::Pretty => "build_response",
        OutputFormat::Human => "output",
    };
    mdb.telemetry().record_phase_start(&exec_id, phase)?;

    let limits = OutputLimits::new(&detail, concise);
    let output = prune_and_format_symbol_response(
        ContextResponse {
            query: name.clone(),
            projects: project_names,
            matches: all_matches,
        },
        &exec_id,
        output_format,
        &limits,
        tokens,
    )?;
    println!("{}", output);

    mdb.telemetry().record_phase_end(&exec_id, phase)?;

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
#[allow(
    clippy::too_many_arguments,
    reason = "CLI command surface: each arg maps to a flag"
)]
pub fn run_context_impact(
    db_paths: Vec<PathBuf>,
    symbol_name: String,
    file: Option<String>,
    depth: usize,
    project_filter: Option<String>,
    output_format: OutputFormat,
    detail: Option<String>,
    concise: bool,
    tokens: Option<usize>,
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

    let target = format!(
        "{}{}",
        symbol_name,
        file.as_ref()
            .map(|f| format!(" (in {})", f))
            .unwrap_or_default()
    );

    if all_impacted.is_empty() {
        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = serde_json::json!({
                    "schema_version": "1.0",
                    "execution_id": exec_id,
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

    let phase = match output_format {
        OutputFormat::Json | OutputFormat::Pretty => "build_response",
        OutputFormat::Human => "output",
    };
    mdb.telemetry().record_phase_start(&exec_id, phase)?;

    let limits = OutputLimits::new(&detail, concise);
    let output = prune_and_format_relation_response(
        "context impact",
        &target,
        depth,
        all_impacted,
        &exec_id,
        output_format,
        &limits,
        tokens,
    )?;
    println!("{}", output);

    mdb.telemetry().record_phase_end(&exec_id, phase)?;

    Ok(())
}

/// Run the context affected command (multi-DB)
/// Forward reachability: find all symbols the target transitively calls
#[allow(
    clippy::too_many_arguments,
    reason = "CLI command surface: each arg maps to a flag"
)]
pub fn run_context_affected(
    db_paths: Vec<PathBuf>,
    symbol_name: String,
    file: Option<String>,
    depth: usize,
    project_filter: Option<String>,
    output_format: OutputFormat,
    detail: Option<String>,
    concise: bool,
    tokens: Option<usize>,
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
                    "execution_id": exec_id,
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

    let phase = match output_format {
        OutputFormat::Json | OutputFormat::Pretty => "build_response",
        OutputFormat::Human => "output",
    };
    mdb.telemetry().record_phase_start(&exec_id, phase)?;

    let limits = OutputLimits::new(&detail, concise);
    let output = prune_and_format_relation_response(
        "context affected",
        &target,
        depth,
        all_affected,
        &exec_id,
        output_format,
        &limits,
        tokens,
    )?;
    println!("{}", output);

    mdb.telemetry().record_phase_end(&exec_id, phase)?;

    Ok(())
}

// Bounded Output Controls Helper Structures and Functions

struct OutputLimits {
    max_callers: usize,
    max_callees: usize,
    max_source_lines: usize,
    max_items: usize,
}

impl OutputLimits {
    fn new(detail: &Option<String>, concise: bool) -> Self {
        let is_concise = concise || detail.as_deref() == Some("concise");
        let is_deep = detail.as_deref() == Some("deep");
        if is_concise {
            Self {
                max_callers: 5,
                max_callees: 5,
                max_source_lines: 15,
                max_items: 5,
            }
        } else if is_deep {
            Self {
                max_callers: 50,
                max_callees: 50,
                max_source_lines: 100,
                max_items: 50,
            }
        } else {
            Self {
                max_callers: 15,
                max_callees: 15,
                max_source_lines: 40,
                max_items: 20,
            }
        }
    }
}

fn prune_and_format_summary_response(
    mut summaries: Vec<(String, magellan::context::ProjectSummary)>,
    limits: &OutputLimits,
    tokens: Option<usize>,
) -> Result<String> {
    let mut is_partial = false;

    // 1. Initial structural pruning based on detail level
    for (_, summary) in &mut summaries {
        if summary.entry_points.len() > limits.max_items {
            summary.entry_points.truncate(limits.max_items);
            is_partial = true;
        }
    }

    // 2. Token budget check and iterative pruning if needed
    if let Some(token_limit) = tokens {
        if token_limit > 0 {
            let char_limit = token_limit * 4;
            let mut entry_points_limit = limits.max_items;

            loop {
                let formatted = format_summary_response(&summaries, is_partial)?;
                if formatted.len() <= char_limit {
                    return Ok(formatted);
                }

                is_partial = true;

                if entry_points_limit > 0 {
                    entry_points_limit = if entry_points_limit > 2 {
                        entry_points_limit / 2
                    } else {
                        0
                    };
                    for (_, summary) in &mut summaries {
                        if summary.entry_points.len() > entry_points_limit {
                            summary.entry_points.truncate(entry_points_limit);
                        }
                    }
                } else {
                    let mut truncated = formatted;
                    truncated.truncate(char_limit.saturating_sub(60));
                    truncated.push_str("\n... [Output truncated due to --token-budget]");
                    return Ok(truncated);
                }
            }
        }
    } else {
        // token_limit is 0, no limit
        return format_summary_response(&summaries, is_partial);
    }

    format_summary_response(&summaries, is_partial)
}

fn format_summary_response(
    summaries: &[(String, magellan::context::ProjectSummary)],
    is_partial: bool,
) -> Result<String> {
    let mut out = String::new();
    for (project, summary) in summaries {
        out.push_str(&format!("Project: {} {}\n", summary.name, summary.version));
        out.push_str(&format!("Language: {}\n", summary.language));
        out.push_str(&format!("Files: {}\n", summary.total_files));
        out.push_str(&format!("Symbols: {}\n", summary.total_symbols));
        out.push('\n');
        out.push_str("Symbol Breakdown:\n");
        out.push_str(&format!(
            "  Functions: {}\n",
            summary.symbol_counts.functions
        ));
        out.push_str(&format!("  Methods: {}\n", summary.symbol_counts.methods));
        out.push_str(&format!("  Structs: {}\n", summary.symbol_counts.structs));
        out.push_str(&format!("  Traits: {}\n", summary.symbol_counts.traits));
        out.push_str(&format!("  Enums: {}\n", summary.symbol_counts.enums));
        out.push_str(&format!("  Modules: {}\n\n", summary.symbol_counts.modules));

        if !summary.entry_points.is_empty() {
            out.push_str("Entry Points:\n");
            for entry in &summary.entry_points {
                out.push_str(&format!("  - {}\n", entry));
            }
            out.push('\n');
        }

        out.push_str(&format!("Project ID: {}\n", project));
        out.push_str("---\n");
    }
    if is_partial {
        out.push_str("\n... [Output truncated due to token budget]\n");
    }
    Ok(out)
}

fn prune_and_format_symbol_response(
    mut response: ContextResponse,
    exec_id: &str,
    output_format: OutputFormat,
    limits: &OutputLimits,
    tokens: Option<usize>,
) -> Result<String> {
    let mut is_partial = false;

    // 1. Initial structural pruning based on detail level
    for m in &mut response.matches {
        if let Some(ref mut callers) = m.callers {
            if callers.len() > limits.max_callers {
                callers.truncate(limits.max_callers);
                is_partial = true;
            }
        }
        if let Some(ref mut callees) = m.callees {
            if callees.len() > limits.max_callees {
                callees.truncate(limits.max_callees);
                is_partial = true;
            }
        }
        if let Some(ref mut source) = m.source {
            let lines: Vec<&str> = source.lines().collect();
            if lines.len() > limits.max_source_lines {
                let pruned = lines[..limits.max_source_lines].join("\n");
                m.source = Some(pruned);
                is_partial = true;
            }
        }
    }

    if response.matches.len() > limits.max_items {
        response.matches.truncate(limits.max_items);
        is_partial = true;
    }

    // 2. Token budget check and iterative pruning if needed
    if let Some(token_limit) = tokens {
        if token_limit > 0 {
            let char_limit = token_limit * 4;
            let mut source_limit = limits.max_source_lines;
            let mut callers_limit = limits.max_callers;
            let mut matches_limit = response.matches.len();

            loop {
                let formatted =
                    format_symbol_response(&response, exec_id, output_format, is_partial)?;
                if formatted.len() <= char_limit {
                    return Ok(formatted);
                }

                is_partial = true;

                if source_limit > 0 {
                    source_limit = if source_limit > 5 {
                        source_limit / 2
                    } else {
                        0
                    };
                    for m in &mut response.matches {
                        if let Some(ref mut source) = m.source {
                            let lines: Vec<&str> = source.lines().collect();
                            if lines.len() > source_limit {
                                if source_limit == 0 {
                                    m.source = None;
                                } else {
                                    m.source = Some(lines[..source_limit].join("\n"));
                                }
                            }
                        }
                    }
                } else if callers_limit > 0 {
                    callers_limit = if callers_limit > 2 {
                        callers_limit / 2
                    } else {
                        0
                    };
                    for m in &mut response.matches {
                        if let Some(ref mut callers) = m.callers {
                            if callers.len() > callers_limit {
                                if callers_limit == 0 {
                                    m.callers = None;
                                } else {
                                    callers.truncate(callers_limit);
                                }
                            }
                        }
                        if let Some(ref mut callees) = m.callees {
                            if callees.len() > callers_limit {
                                if callers_limit == 0 {
                                    m.callees = None;
                                } else {
                                    callees.truncate(callers_limit);
                                }
                            }
                        }
                    }
                } else if matches_limit > 1 {
                    matches_limit -= 1;
                    response.matches.truncate(matches_limit);
                } else {
                    let mut truncated = formatted;
                    truncated.truncate(char_limit.saturating_sub(60));
                    truncated.push_str("\n... [Output truncated due to --token-budget]");
                    return Ok(truncated);
                }
            }
        }
    } else {
        // token_limit is 0, no limit
        return format_symbol_response(&response, exec_id, output_format, is_partial);
    }

    format_symbol_response(&response, exec_id, output_format, is_partial)
}

fn format_symbol_response(
    response: &ContextResponse,
    exec_id: &str,
    output_format: OutputFormat,
    is_partial: bool,
) -> Result<String> {
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let mut json_response = magellan::output::JsonResponse::new(response.clone(), exec_id);
            if is_partial {
                json_response.partial = Some(true);
            }
            let s = match output_format {
                OutputFormat::Json => serde_json::to_string(&json_response)?,
                OutputFormat::Pretty => serde_json::to_string_pretty(&json_response)?,
                _ => unreachable!(),
            };
            Ok(s)
        }
        OutputFormat::Human => {
            let mut out = String::new();
            for (i, m) in response.matches.iter().enumerate() {
                if i > 0 {
                    out.push_str("\n---\n");
                }
                out.push_str(&format!("Project: {}\n", m.project));
                out.push_str(&format!("Symbol: {}\n", m.name));
                out.push_str(&format!("Kind: {}\n", m.kind));
                out.push_str(&format!(
                    "File: {}:{}\n",
                    m.span.file_path, m.span.start_line
                ));

                if let Some(ref callers) = m.callers {
                    if !callers.is_empty() {
                        out.push_str(&format!("\nCallers ({}):\n", callers.len()));
                        for c in callers {
                            let depth_str =
                                c.depth.map_or(String::new(), |d| format!("[depth={}]", d));
                            out.push_str(&format!(
                                "  - {} ({}:{}) {}\n",
                                c.name, c.file_path, c.line, depth_str
                            ));
                        }
                    }
                }

                if let Some(ref callees) = m.callees {
                    if !callees.is_empty() {
                        out.push_str(&format!("\nCallees ({}):\n", callees.len()));
                        for c in callees {
                            let depth_str =
                                c.depth.map_or(String::new(), |d| format!("[depth={}]", d));
                            out.push_str(&format!(
                                "  - {} ({}:{}) {}\n",
                                c.name, c.file_path, c.line, depth_str
                            ));
                        }
                    }
                }

                if let Some(ref source) = m.source {
                    out.push_str(&format!(
                        "\nSource ({}:{}-{}):\n",
                        m.span.file_path, m.span.start_line, m.span.end_line
                    ));
                    for line in source.lines() {
                        out.push_str(&format!("  {}\n", line));
                    }
                }
            }
            if is_partial {
                out.push_str("\n... [Output truncated due to token budget]\n");
            }
            Ok(out)
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Formatting helper taking output configuration parameters"
)]
fn prune_and_format_relation_response(
    command_name: &str,
    target: &str,
    depth_limit: usize,
    mut all_relations: Vec<(String, SymbolRelation)>,
    exec_id: &str,
    output_format: OutputFormat,
    limits: &OutputLimits,
    tokens: Option<usize>,
) -> Result<String> {
    let mut is_partial = false;

    // 1. Initial structural pruning based on detail level
    if all_relations.len() > limits.max_items {
        all_relations.truncate(limits.max_items);
        is_partial = true;
    }

    // 2. Token budget check and iterative pruning if needed
    if let Some(token_limit) = tokens {
        if token_limit > 0 {
            let char_limit = token_limit * 4;
            let mut relations_limit = all_relations.len();

            loop {
                let formatted = format_relation_response(
                    command_name,
                    target,
                    depth_limit,
                    &all_relations,
                    exec_id,
                    output_format,
                    is_partial,
                )?;
                if formatted.len() <= char_limit {
                    return Ok(formatted);
                }

                is_partial = true;

                if relations_limit > 1 {
                    relations_limit -= 1;
                    all_relations.truncate(relations_limit);
                } else {
                    let mut truncated = formatted;
                    truncated.truncate(char_limit.saturating_sub(60));
                    truncated.push_str("\n... [Output truncated due to --token-budget]");
                    return Ok(truncated);
                }
            }
        }
    } else {
        // token_limit is 0, no limit
        return format_relation_response(
            command_name,
            target,
            depth_limit,
            &all_relations,
            exec_id,
            output_format,
            is_partial,
        );
    }

    format_relation_response(
        command_name,
        target,
        depth_limit,
        &all_relations,
        exec_id,
        output_format,
        is_partial,
    )
}

fn format_relation_response(
    command_name: &str,
    target: &str,
    depth_limit: usize,
    records: &[(String, SymbolRelation)],
    exec_id: &str,
    output_format: OutputFormat,
    is_partial: bool,
) -> Result<String> {
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let impacted_json: Vec<serde_json::Value> = records
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
            let mut response = serde_json::json!({
                "schema_version": "1.0",
                "execution_id": exec_id,
                "command": command_name,
                "data": {
                    "target": target,
                    "depth_limit": depth_limit,
                    "total_records": records.len(),
                    "records": impacted_json,
                },
            });
            if is_partial {
                response["partial"] = serde_json::json!(true);
            }
            let s = match output_format {
                OutputFormat::Json => serde_json::to_string(&response)?,
                OutputFormat::Pretty => serde_json::to_string_pretty(&response)?,
                _ => unreachable!(),
            };
            Ok(s)
        }
        OutputFormat::Human => {
            let mut out = String::new();
            out.push_str(&format!(
                "{}: {} (depth limit: {})\n",
                if command_name.contains("impact") {
                    "Impact analysis"
                } else {
                    "Affected analysis"
                },
                target,
                depth_limit
            ));
            out.push_str(&format!("{} symbol(s) reached\n\n", records.len(),));

            let mut last_project = String::new();
            for (proj, r) in records {
                if proj != &last_project {
                    if !last_project.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&format!("Project: {}\n", proj));
                    last_project = proj.clone();
                }
                let depth_str = r.depth.map_or(String::new(), |d| format!(" [depth={}]", d));
                out.push_str(&format!(
                    "  {} ({}:{}){}\n",
                    r.name, r.file, r.line, depth_str
                ));
            }
            if is_partial {
                out.push_str("\n... [Output truncated due to token budget]\n");
            }
            Ok(out)
        }
    }
}
