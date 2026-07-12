use anyhow::Result;
use magellan::context::{ProjectSummary, SymbolRelation};
use magellan::output::{ContextResponse, JsonResponse, OutputFormat};

pub(crate) struct OutputLimits {
    max_callers: usize,
    max_callees: usize,
    max_source_lines: usize,
    max_items: usize,
}

impl OutputLimits {
    pub(crate) fn new(detail: &Option<String>, concise: bool) -> Self {
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

pub(crate) fn prune_and_format_summary_response(
    mut summaries: Vec<(String, ProjectSummary)>,
    limits: &OutputLimits,
    tokens: Option<usize>,
) -> Result<String> {
    let mut is_partial = false;

    for (_, summary) in &mut summaries {
        if summary.entry_points.len() > limits.max_items {
            summary.entry_points.truncate(limits.max_items);
            is_partial = true;
        }
    }

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
        return format_summary_response(&summaries, is_partial);
    }

    format_summary_response(&summaries, is_partial)
}

fn format_summary_response(
    summaries: &[(String, ProjectSummary)],
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

pub(crate) fn prune_and_format_symbol_response(
    mut response: ContextResponse,
    exec_id: &str,
    output_format: OutputFormat,
    limits: &OutputLimits,
    tokens: Option<usize>,
) -> Result<String> {
    let mut is_partial = false;

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
            let mut json_response = JsonResponse::new(response.clone(), exec_id);
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
pub(crate) fn prune_and_format_relation_response(
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

    if all_relations.len() > limits.max_items {
        all_relations.truncate(limits.max_items);
        is_partial = true;
    }

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
            out.push_str(&format!("{} symbol(s) reached\n\n", records.len()));

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
