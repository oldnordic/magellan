use anyhow::Result;
use magellan::graph::CodeGraph;
use std::path::PathBuf;
use std::process::Command;

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "be", "been", "bug", "by", "fix", "for", "from", "how", "in", "into",
    "is", "of", "on", "that", "the", "these", "this", "those", "to", "was", "were", "what", "when",
    "where", "why", "with",
];

pub fn extract_terms(task: &str, max: usize) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut i = 0;
    let chars: Vec<char> = task.chars().collect();
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let term: String = chars[start..i].iter().collect();
            if term.len() >= 3 || term.contains('_') {
                let lc = term.to_lowercase();
                if !STOP_WORDS.contains(&lc.as_str()) && seen.insert(lc) {
                    terms.push(term);
                    if terms.len() >= max {
                        break;
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    terms
}

struct Section {
    title: String,
    body: String,
}

pub struct NavigateConfig {
    pub db_path: PathBuf,
    pub task: String,
    pub depth: usize,
    pub budget: usize,
    pub limit: usize,
    pub concise: bool,
    pub with_llmgrep: bool,
    pub with_mirage: bool,
    pub tokens: Option<usize>,
}

pub fn run_navigate(cfg: NavigateConfig) -> Result<()> {
    let NavigateConfig {
        db_path,
        task,
        depth,
        budget,
        limit,
        concise,
        with_llmgrep,
        with_mirage,
        tokens,
    } = cfg;
    let terms = extract_terms(&task, 8);
    let graph = CodeGraph::open(&db_path)?;
    let nav = graph.navigator();
    let mut sections: Vec<Section> = Vec::new();

    let mut resolved_ids: Vec<i64> = Vec::new();
    let mut resolved_names: Vec<String> = Vec::new();

    for term in &terms {
        match nav.resolve(term) {
            Ok(hits) if !hits.is_empty() => {
                let mut body = String::new();
                for hit in hits.iter().take(limit) {
                    body.push_str(&format!(
                        "- `{}` ({}) — {}:{}\n",
                        hit.name,
                        hit.kind_normalized.as_deref().unwrap_or(&hit.kind),
                        hit.file_path.as_deref().unwrap_or("?"),
                        hit.byte_start,
                    ));
                    if !resolved_names.contains(&hit.name) {
                        resolved_names.push(hit.name.clone());
                        resolved_ids.push(hit.id);
                    }
                }
                sections.push(Section {
                    title: format!("find: {}", term),
                    body,
                });
            }
            _ => {
                sections.push(Section {
                    title: format!("find: {}", term),
                    body: format!("- No symbols found for `{}`\n", term),
                });
            }
        }
    }

    if concise {
        if let Some((&first_id, first_name)) = resolved_ids.first().zip(resolved_names.first()) {
            let mut body = String::new();
            if let Some(info) = nav.info(first_id)? {
                body.push_str(&format!(
                    "- file: `{}` line: {}\n",
                    info.file_path.as_deref().unwrap_or("?"),
                    info.start_line,
                ));
                body.push_str(&format!(
                    "- kind: `{}`\n",
                    info.kind_normalized.as_deref().unwrap_or(&info.kind),
                ));
            }

            let callers = nav.k_hop_callers(first_id, 1)?;
            if !callers.is_empty() {
                body.push_str("\n**callers:**\n");
                for c in &callers {
                    body.push_str(&format!(
                        "  - `{}` — {}:{}\n",
                        c.info.name,
                        c.info.file_path.as_deref().unwrap_or("?"),
                        c.info.start_line,
                    ));
                }
            }
            let callees = nav.k_hop_callees(first_id, 1)?;
            if !callees.is_empty() {
                body.push_str("\n**callees:**\n");
                for c in &callees {
                    body.push_str(&format!(
                        "  - `{}` — {}:{}\n",
                        c.info.name,
                        c.info.file_path.as_deref().unwrap_or("?"),
                        c.info.start_line,
                    ));
                }
            }

            if let Some(info) = nav.info(first_id)? {
                let file = info.file_path.as_deref().unwrap_or("");
                let snippet = read_source_lines(file, info.start_line, info.end_line);
                if let Some(src) = snippet {
                    body.push_str("\n```\n");
                    body.push_str(&src);
                    body.push_str("\n```\n");
                }
            }

            let truncated = truncate_to_budget(&body, budget);
            sections.push(Section {
                title: format!("context: {}", first_name),
                body: truncated,
            });
        }
    } else {
        let top_n = resolved_ids.len().min(3);
        let top_ids: Vec<i64> = resolved_ids.iter().copied().take(top_n).collect();
        let top_names: Vec<&String> = resolved_names.iter().take(top_n).collect();

        for (idx, (&sym_id, sym_name)) in top_ids.iter().zip(top_names.iter()).enumerate() {
            let callers = nav.k_hop_callers(sym_id, 1)?;
            if !callers.is_empty() {
                let body = format_callers(&callers);
                sections.push(Section {
                    title: format!("callers: {}", sym_name),
                    body,
                });
            }

            let callees = nav.k_hop_callees(sym_id, 1)?;
            if !callees.is_empty() {
                let body = format_callees(&callees);
                sections.push(Section {
                    title: format!("callees: {}", sym_name),
                    body,
                });
            }

            let impact_depth = depth.max(1) as u32;
            let impact = nav.k_hop_callers(sym_id, impact_depth)?;
            if !impact.is_empty() {
                let body = format_impact(&impact, impact_depth);
                sections.push(Section {
                    title: format!("impact: {}", sym_name),
                    body,
                });
            }

            let affected = nav.k_hop_callees(sym_id, impact_depth)?;
            if !affected.is_empty() {
                let body = format_affected(&affected, impact_depth);
                sections.push(Section {
                    title: format!("affected: {}", sym_name),
                    body,
                });
            }

            if let Some(info) = nav.info(sym_id)? {
                let mut body = String::new();
                body.push_str(&format!(
                    "- file: `{}` line: {}\n",
                    info.file_path.as_deref().unwrap_or("?"),
                    info.start_line,
                ));
                body.push_str(&format!(
                    "- kind: `{}`\n",
                    info.kind_normalized.as_deref().unwrap_or(&info.kind),
                ));
                let file = info.file_path.as_deref().unwrap_or("");
                let snippet = read_source_lines(file, info.start_line, info.end_line);
                if let Some(src) = snippet {
                    body.push_str("\n```\n");
                    body.push_str(&src);
                    body.push_str("\n```\n");
                }
                sections.push(Section {
                    title: format!("context: {}", sym_name),
                    body,
                });
            }

            if with_mirage {
                let output = Command::new("mirage")
                    .args([
                        "cfg",
                        "--db",
                        &db_path.to_string_lossy(),
                        "--function",
                        sym_name,
                    ])
                    .output();
                if let Ok(out) = output {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    if !stdout.trim().is_empty() {
                        sections.push(Section {
                            title: format!("cfg: {}", sym_name),
                            body: format!("```\n{}\n```\n", stdout.trim()),
                        });
                    }
                }
            }

            let _ = idx;
        }
    }

    if with_llmgrep && !task.is_empty() {
        let output = Command::new("llmgrep")
            .args([
                "--db",
                &db_path.to_string_lossy(),
                "search",
                "--query",
                &task,
                "--output",
                "human",
            ])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if !stdout.trim().is_empty() {
                sections.push(Section {
                    title: "semantic search".to_string(),
                    body: stdout,
                });
            }
        }
    }

    let packet = render_packet(&task, &terms, &sections);
    let final_output = if let Some(token_limit) = tokens {
        if token_limit > 0 {
            let tokens_est = packet.len() / 4;
            if tokens_est > token_limit {
                let char_limit = token_limit * 4;
                let truncated = packet.chars().take(char_limit).collect::<String>();
                format!("{}\n\n*[~{} tokens, truncated]*", truncated, token_limit)
            } else {
                packet
            }
        } else {
            packet
        }
    } else {
        packet
    };
    print!("{}", final_output);
    Ok(())
}

fn format_callers(symbols: &[magellan::graph::navigator::DepthSymbol]) -> String {
    let mut body = String::new();
    for s in symbols {
        body.push_str(&format!(
            "- `{}` — {}:{}\n",
            s.info.name,
            s.info.file_path.as_deref().unwrap_or("?"),
            s.info.start_line,
        ));
    }
    body
}

fn format_callees(symbols: &[magellan::graph::navigator::DepthSymbol]) -> String {
    let mut body = String::new();
    for s in symbols {
        body.push_str(&format!(
            "- `{}` — {}:{}\n",
            s.info.name,
            s.info.file_path.as_deref().unwrap_or("?"),
            s.info.start_line,
        ));
    }
    body
}

fn format_impact(symbols: &[magellan::graph::navigator::DepthSymbol], _max_depth: u32) -> String {
    let mut body = String::new();
    for s in symbols {
        body.push_str(&format!(
            "- `{}` (depth {}) — {}:{}\n",
            s.info.name,
            s.depth,
            s.info.file_path.as_deref().unwrap_or("?"),
            s.info.start_line,
        ));
    }
    body
}

fn format_affected(symbols: &[magellan::graph::navigator::DepthSymbol], _max_depth: u32) -> String {
    let mut body = String::new();
    for s in symbols {
        body.push_str(&format!(
            "- `{}` (depth {}) — {}:{}\n",
            s.info.name,
            s.depth,
            s.info.file_path.as_deref().unwrap_or("?"),
            s.info.start_line,
        ));
    }
    body
}

fn truncate_to_budget(text: &str, budget: usize) -> String {
    let char_limit = budget * 4;
    if text.len() <= char_limit {
        return text.to_string();
    }
    let mut truncated = text.chars().take(char_limit).collect::<String>();
    truncated.push_str("\n\n*[truncated to budget]*\n");
    truncated
}

fn render_packet(task: &str, terms: &[String], sections: &[Section]) -> String {
    let mut out = String::new();
    out.push_str("# Grounded Investigation Packet\n\n");
    out.push_str(&format!("Task: {}\n\n", task));

    out.push_str("## Terms\n\n");
    if terms.is_empty() {
        out.push_str("- No search terms extracted\n");
    } else {
        for t in terms {
            out.push_str(&format!("- `{}`\n", t));
        }
    }

    out.push_str("\n## Results\n\n");
    for section in sections {
        out.push_str(&format!("### {}\n\n", section.title));
        out.push_str(&section.body);
        out.push('\n');
    }

    let token_est = out.len().div_ceil(4);
    out.push_str("## Token Summary\n\n");
    out.push_str(&format!("- **packet tokens**: {}\n", token_est));
    out.push_str("- **method**: chars/4\n");

    out
}

fn read_source_lines(file_path: &str, start_line: usize, end_line: usize) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let from = start_line.saturating_sub(1);
    let to = if end_line > 0 { end_line } else { from + 20 };
    let lines: Vec<&str> = content.lines().skip(from).take(to - from).collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_terms_basic() {
        let terms = extract_terms("who calls index_file in the indexer", 8);
        assert!(terms.contains(&"calls".to_string()) || terms.contains(&"index_file".to_string()));
        assert!(!terms.iter().any(|t| t == "the" || t == "in"));
    }

    #[test]
    fn test_extract_terms_max() {
        let terms = extract_terms(
            "alpha beta gamma delta epsilon zeta eta theta iota kappa",
            4,
        );
        assert_eq!(terms.len(), 4);
    }

    #[test]
    fn test_extract_terms_short_words_excluded() {
        let terms = extract_terms("fix the bug in db", 8);
        assert!(!terms.iter().any(|t| t == "fix" || t == "db"));
    }

    #[test]
    fn test_extract_terms_underscore_allowed() {
        let terms = extract_terms("look at my_fn for details", 8);
        assert!(terms.contains(&"my_fn".to_string()));
    }

    #[test]
    fn test_render_packet_contains_task() {
        let packet = render_packet("find parse_args", &["parse_args".to_string()], &[]);
        assert!(packet.contains("Task: find parse_args"));
        assert!(packet.contains("## Terms"));
        assert!(packet.contains("packet tokens"));
    }

    #[test]
    fn test_truncate_to_budget_no_truncation() {
        let text = "short text";
        assert_eq!(truncate_to_budget(text, 4000), text);
    }

    #[test]
    fn test_truncate_to_budget_truncates() {
        let text = "a".repeat(100);
        let result = truncate_to_budget(&text, 10);
        assert!(result.len() < text.len());
        assert!(result.contains("truncated to budget"));
    }
}
