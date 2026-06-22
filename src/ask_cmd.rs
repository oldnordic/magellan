use anyhow::{bail, Context, Result};
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::service::registry::Registry;

/// Detected query intent for routing
#[derive(Debug, PartialEq)]
pub enum Intent {
    Callers,
    Callees,
    Cfg,
    BlastZone,
    Cycles,
    Impact,
    Complex,
    Search,
    Find,
}

/// Classify a lowercased query string into a routing intent.
pub fn detect_intent(q: &str) -> Intent {
    if [
        "who calls",
        "who uses",
        "callers of",
        "who references",
        "who invokes",
        "dependencies of",
        "dependents of",
        "who depends on",
    ]
    .iter()
    .any(|p| q.contains(p))
    {
        return Intent::Callers;
    }
    if [
        "who is called by",
        "callees of",
        "calls from",
        "outgoing calls",
        "called by",
    ]
    .iter()
    .any(|p| q.contains(p))
    {
        return Intent::Callees;
    }
    if ["cfg for", "control flow", "cfg of", "show cfg"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Cfg;
    }
    if [
        "blast zone",
        "blast-zone",
        "hot paths",
        "hotpaths",
        "hot path",
    ]
    .iter()
    .any(|p| q.contains(p))
    {
        return Intent::BlastZone;
    }
    if ["cycles", "circular", "cyclic", "strongly connected"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Cycles;
    }
    if ["impact of", "affected by", "what breaks", "what changes"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Impact;
    }
    if ["complex", "high complexity", "most complex", "complicated"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Complex;
    }
    if ["search", "semantic", "find code", "look for"]
        .iter()
        .any(|p| q.contains(p))
    {
        return Intent::Search;
    }
    Intent::Find
}

/// Run the `magellan ask` intent-router command
pub fn run_ask(
    question: String,
    db_path: PathBuf,
    all: bool,
    output_format: OutputFormat,
) -> Result<()> {
    if all {
        return run_ask_all(question, output_format);
    }

    let q = question.to_lowercase();
    let intent = detect_intent(&q);

    // Intents that don't need a symbol name
    match intent {
        Intent::Cycles => return route_cycles(db_path, output_format),
        Intent::Search => return route_search(db_path, &question, output_format),
        Intent::Complex => return route_complex(db_path, &question, output_format),
        _ => {}
    }

    let name = extract_quoted_symbol(&q)
        .or_else(|| q.split_whitespace().last().map(str::to_string))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine symbol name from question. Try: \"who calls 'run_find'\""
            )
        })?;

    match intent {
        Intent::Callers => route_refs(db_path, name, "in".to_string(), output_format),
        Intent::Callees => route_refs(db_path, name, "out".to_string(), output_format),
        Intent::Cfg => route_cfg(db_path, name, output_format),
        Intent::BlastZone => route_blast_zone(db_path, name, output_format),
        Intent::Impact => route_impact(db_path, name, output_format),
        Intent::Find => route_find(db_path, name, output_format),
        Intent::Cycles | Intent::Search | Intent::Complex => unreachable!(),
    }
}

fn run_ask_all(question: String, output_format: OutputFormat) -> Result<()> {
    let registry = Registry::load()?;
    let enabled: Vec<_> = registry.projects.iter().filter(|p| p.enabled).collect();

    if enabled.is_empty() {
        println!("No enabled projects in registry.");
        println!("Hint: use `magellan catalog` to list registered projects, then `magellan watch` to index one.");
        return Ok(());
    }

    for entry in &enabled {
        println!("=== {} ===", entry.name);
        if entry.db.exists() {
            if let Err(e) = run_ask(question.clone(), entry.db.clone(), false, output_format) {
                println!("  error: {}", e);
            }
        } else {
            println!("  database not found: {}", entry.db.display());
        }
        println!();
    }

    Ok(())
}

/// Extract a single- or double-quoted symbol from a query string.
fn extract_quoted_symbol(q: &str) -> Option<String> {
    for (open, close) in [('\'', '\''), ('\"', '\"')] {
        if let Some(start) = q.find(open) {
            let rest = &q[start + 1..];
            if let Some(end) = rest.find(close) {
                let candidate = &rest[..end];
                if !candidate.is_empty() {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

fn route_find(db_path: PathBuf, name: String, output_format: OutputFormat) -> Result<()> {
    crate::find_cmd::run_find(
        db_path,
        Some(name),
        None,  // root
        None,  // path
        None,  // glob_pattern
        None,  // symbol_id
        None,  // ambiguous_name
        false, // first
        output_format,
        true,  // with_context
        true,  // with_callers
        true,  // with_callees
        true,  // with_semantics
        false, // with_checksums
        3,     // context_lines
        false, // all
    )
    .with_context(|| "Ask → find routing failed")
}

fn route_refs(
    db_path: PathBuf,
    name: String,
    direction: String,
    output_format: OutputFormat,
) -> Result<()> {
    crate::refs_cmd::run_refs(
        db_path,
        name,
        None, // root
        None, // path
        None, // symbol_id
        direction,
        output_format,
        true,  // with_context
        true,  // with_semantics
        false, // with_checksums
        3,     // context_lines
        false, // all
        None,  // tokens
    )
    .with_context(|| "Ask → refs routing failed")
}

fn route_cfg(db_path: PathBuf, name: String, _output_format: OutputFormat) -> Result<()> {
    let db = db_path.to_string_lossy();
    let status = std::process::Command::new("mirage")
        .args(["cfg", "--db", &db, "--function", &name])
        .status()
        .with_context(|| "Ask → cfg: failed to run mirage")?;
    if !status.success() {
        bail!("mirage exited with {}", status);
    }
    Ok(())
}

fn route_blast_zone(db_path: PathBuf, name: String, _output_format: OutputFormat) -> Result<()> {
    let db = db_path.to_string_lossy();
    let status = std::process::Command::new("mirage")
        .args(["blast-zone", "--db", &db, "--function", &name])
        .status()
        .with_context(|| "Ask → blast-zone: failed to run mirage")?;
    if !status.success() {
        bail!("mirage exited with {}", status);
    }
    Ok(())
}

fn route_cycles(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    crate::cycles_cmd::run_cycles(db_path, None, output_format)
        .with_context(|| "Ask → cycles routing failed")
}

fn route_impact(db_path: PathBuf, name: String, output_format: OutputFormat) -> Result<()> {
    crate::context_cmd::run_context_impact(
        vec![db_path],
        name,
        None, // file
        3,    // depth
        None, // project_filter
        output_format,
        None,  // detail
        false, // concise
        None,  // tokens
    )
    .with_context(|| "Ask → impact routing failed")
}

fn route_complex(db_path: PathBuf, question: &str, _output_format: OutputFormat) -> Result<()> {
    let db = db_path.to_string_lossy();
    let status = std::process::Command::new("llmgrep")
        .args([
            "--db",
            &db,
            "search",
            "--query",
            question,
            "--min-complexity",
            "10",
        ])
        .status()
        .with_context(|| "Ask → complex: failed to run llmgrep")?;
    if !status.success() {
        bail!("llmgrep exited with {}", status);
    }
    Ok(())
}

fn route_search(db_path: PathBuf, question: &str, _output_format: OutputFormat) -> Result<()> {
    let db = db_path.to_string_lossy();
    let status = std::process::Command::new("llmgrep")
        .args(["--db", &db, "search", "--query", question])
        .status()
        .with_context(|| "Ask → search: failed to run llmgrep")?;
    if !status.success() {
        bail!("llmgrep exited with {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_intent_callers() {
        assert_eq!(detect_intent("who calls run_find"), Intent::Callers);
        assert_eq!(detect_intent("callers of parse_args"), Intent::Callers);
        assert_eq!(detect_intent("who references foo"), Intent::Callers);
        assert_eq!(detect_intent("who depends on bar"), Intent::Callers);
    }

    #[test]
    fn test_detect_intent_callees() {
        assert_eq!(detect_intent("callees of main"), Intent::Callees);
        assert_eq!(
            detect_intent("outgoing calls from run_watch"),
            Intent::Callees
        );
    }

    #[test]
    fn test_detect_intent_cfg() {
        assert_eq!(detect_intent("cfg for run_status"), Intent::Cfg);
        assert_eq!(
            detect_intent("control flow of parse_watch_args"),
            Intent::Cfg
        );
        assert_eq!(detect_intent("show cfg of main"), Intent::Cfg);
    }

    #[test]
    fn test_detect_intent_blast_zone() {
        assert_eq!(
            detect_intent("blast zone of handle_request"),
            Intent::BlastZone
        );
        assert_eq!(detect_intent("hot paths in run_find"), Intent::BlastZone);
        assert_eq!(detect_intent("blast-zone of foo"), Intent::BlastZone);
    }

    #[test]
    fn test_detect_intent_cycles() {
        assert_eq!(detect_intent("cycles in the call graph"), Intent::Cycles);
        assert_eq!(detect_intent("circular dependencies"), Intent::Cycles);
        assert_eq!(
            detect_intent("strongly connected components"),
            Intent::Cycles
        );
    }

    #[test]
    fn test_detect_intent_impact() {
        assert_eq!(detect_intent("impact of resolve_db_path"), Intent::Impact);
        assert_eq!(detect_intent("affected by run_status"), Intent::Impact);
        assert_eq!(detect_intent("what breaks if i change foo"), Intent::Impact);
    }

    #[test]
    fn test_detect_intent_complex() {
        assert_eq!(detect_intent("complex functions in src"), Intent::Complex);
        assert_eq!(detect_intent("high complexity code"), Intent::Complex);
        assert_eq!(detect_intent("most complex function"), Intent::Complex);
    }

    #[test]
    fn test_detect_intent_search() {
        assert_eq!(detect_intent("search for error handling"), Intent::Search);
        assert_eq!(
            detect_intent("semantic query about retry logic"),
            Intent::Search
        );
        assert_eq!(detect_intent("find code that parses toml"), Intent::Search);
    }

    #[test]
    fn test_detect_intent_find_fallback() {
        assert_eq!(detect_intent("run_status"), Intent::Find);
        assert_eq!(detect_intent("where is parse_watch_args"), Intent::Find);
    }

    #[test]
    fn test_extract_quoted_symbol_single_quotes() {
        assert_eq!(
            extract_quoted_symbol("who calls 'run_find'"),
            Some("run_find".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_symbol_double_quotes() {
        assert_eq!(
            extract_quoted_symbol("cfg for \"parse_status_args\""),
            Some("parse_status_args".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_symbol_none() {
        assert_eq!(extract_quoted_symbol("who calls run_find"), None);
    }
}
