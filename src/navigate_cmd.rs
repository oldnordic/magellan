use anyhow::Result;
use magellan::context::query::{
    affected_analysis, get_callees, get_callers, get_symbol_detail, impact_analysis,
};
use magellan::graph::CodeGraph;
use std::path::PathBuf;
use std::process::Command;

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "be", "been", "bug", "by", "fix", "for", "from", "how", "in", "into",
    "is", "of", "on", "that", "the", "these", "this", "those", "to", "was", "were", "what", "when",
    "where", "why", "with",
];

/// Extract identifier-like terms from a natural-language task string.
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
}

/// Run the navigate command: extract terms, query magellan graph, optionally invoke llmgrep/mirage.
///
/// Two modes (mirrors grounded-navigator):
/// - Normal: for top 3 symbols → callers + callees + impact + affected + context+source (7 queries each)
/// - Concise: for top 1 symbol → single bundled context call with callers+callees+source, truncated to budget
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
    } = cfg;
    let terms = extract_terms(&task, 8);
    let mut graph = CodeGraph::open(&db_path)?;
    let mut sections: Vec<Section> = Vec::new();

    // Step 1: find symbols for each term
    let mut resolved: Vec<String> = Vec::new();
    for term in &terms {
        match graph.search_symbols_by_name(term) {
            Ok(hits) if !hits.is_empty() => {
                let mut body = String::new();
                for hit in hits.iter().take(limit) {
                    body.push_str(&format!(
                        "- `{}` ({}) — {}:{}\n",
                        hit.name, hit.kind, hit.file_path, hit.byte_start
                    ));
                    if !resolved.contains(&hit.name) {
                        resolved.push(hit.name.clone());
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
        // Concise mode: single bundled context for top 1 symbol (callers + callees + source),
        // truncated to budget tokens. Equivalent to grounded-index context --include-callers
        // --include-callees --budget N.
        if let Some(sym) = resolved.first() {
            if let Ok(detail) = get_symbol_detail(&mut graph, sym, None) {
                let mut body = String::new();
                body.push_str(&format!(
                    "- file: `{}` line: {}\n",
                    detail.file, detail.line
                ));
                body.push_str(&format!("- kind: `{}`\n", detail.kind));

                if !detail.callers.is_empty() {
                    body.push_str("\n**callers:**\n");
                    for c in &detail.callers {
                        body.push_str(&format!("  - `{}` — {}:{}\n", c.name, c.file, c.line));
                    }
                }
                if !detail.callees.is_empty() {
                    body.push_str("\n**callees:**\n");
                    for c in &detail.callees {
                        body.push_str(&format!("  - `{}` — {}:{}\n", c.name, c.file, c.line));
                    }
                }

                let snippet = read_source_lines(&detail.file, detail.line, detail.end_line);
                if let Some(src) = snippet {
                    body.push_str("\n```\n");
                    body.push_str(&src);
                    body.push_str("\n```\n");
                }

                // Truncate to budget tokens
                let truncated = truncate_to_budget(&body, budget);
                sections.push(Section {
                    title: format!("context: {}", sym),
                    body: truncated,
                });
            }
        }
    } else {
        // Normal mode: for top 3 symbols run all individual queries
        let top_n = resolved.len().min(3);
        for sym in resolved.iter().take(top_n) {
            // callers
            if let Ok(callers) = get_callers(&mut graph, sym, None) {
                if !callers.is_empty() {
                    let mut body = String::new();
                    for c in &callers {
                        body.push_str(&format!("- `{}` — {}:{}\n", c.name, c.file, c.line));
                    }
                    sections.push(Section {
                        title: format!("callers: {}", sym),
                        body,
                    });
                }
            }

            // callees
            if let Ok(callees) = get_callees(&mut graph, sym, None) {
                if !callees.is_empty() {
                    let mut body = String::new();
                    for c in &callees {
                        body.push_str(&format!("- `{}` — {}:{}\n", c.name, c.file, c.line));
                    }
                    sections.push(Section {
                        title: format!("callees: {}", sym),
                        body,
                    });
                }
            }

            // impact (what calls this, transitively)
            if let Ok(impact) = impact_analysis(&mut graph, sym, None, depth) {
                if !impact.is_empty() {
                    let mut body = String::new();
                    for r in &impact {
                        body.push_str(&format!(
                            "- `{}` (depth {}) — {}:{}\n",
                            r.name,
                            r.depth.unwrap_or(0),
                            r.file,
                            r.line
                        ));
                    }
                    sections.push(Section {
                        title: format!("impact: {}", sym),
                        body,
                    });
                }
            }

            // affected (what this calls, transitively)
            if let Ok(affected) = affected_analysis(&mut graph, sym, None, depth) {
                if !affected.is_empty() {
                    let mut body = String::new();
                    for r in &affected {
                        body.push_str(&format!(
                            "- `{}` (depth {}) — {}:{}\n",
                            r.name,
                            r.depth.unwrap_or(0),
                            r.file,
                            r.line
                        ));
                    }
                    sections.push(Section {
                        title: format!("affected: {}", sym),
                        body,
                    });
                }
            }

            // context with source
            if let Ok(detail) = get_symbol_detail(&mut graph, sym, None) {
                let mut body = String::new();
                body.push_str(&format!(
                    "- file: `{}` line: {}\n",
                    detail.file, detail.line
                ));
                body.push_str(&format!("- kind: `{}`\n", detail.kind));
                let snippet = read_source_lines(&detail.file, detail.line, detail.end_line);
                if let Some(src) = snippet {
                    body.push_str("\n```\n");
                    body.push_str(&src);
                    body.push_str("\n```\n");
                }
                sections.push(Section {
                    title: format!("context: {}", sym),
                    body,
                });
            }

            // optional mirage CFG
            if with_mirage {
                let output = Command::new("mirage")
                    .args(["cfg", "--db", &db_path.to_string_lossy(), "--function", sym])
                    .output();
                if let Ok(out) = output {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    if !stdout.trim().is_empty() {
                        sections.push(Section {
                            title: format!("cfg: {}", sym),
                            body: format!("```\n{}\n```\n", stdout.trim()),
                        });
                    }
                }
            }
        }
    }

    // optional llmgrep semantic search (both modes)
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
    print!("{}", packet);
    Ok(())
}

/// Truncate body text to approximately `budget` tokens (1 token ≈ 4 chars).
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
        let result = truncate_to_budget(&text, 10); // budget=10 → limit=40 chars
        assert!(result.len() < text.len());
        assert!(result.contains("truncated to budget"));
    }
}
