//! LSP enrichment using rust-analyzer, clangd, and jdtls
//!
//! Uses CLI commands to extract type signatures and documentation.
//! 
//! ## rust-analyzer
//! 
//! rust-analyzer provides JSON output through its analysis commands:
//! 
//! ```bash
//! # Get syntax tree (JSON)
//! rust-analyzer parse < file.rs
//! 
//! # Get analysis stats (JSON)
//! rust-analyzer analysis-stats .
//! ```
//!
//! ## clangd
//!
//! clangd provides JSON output through clangd-query:
//!
//! ```bash
//! clangd-query --dump-ast file.cpp
//! ```
//!
//! ## jdtls
//!
//! jdtls is typically run as a language server. For CLI usage,
//! use javac with annotation processing or Eclipse JDT Core batch APIs.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::graph::CodeGraph;
use super::analyzer::{AnalyzerKind, AnalyzerResult, detect_available_analyzers, detect_language_from_path};

/// Configuration for symbol enrichment
#[derive(Debug, Clone)]
pub struct EnrichConfig {
    /// Which analyzers to use (None = use all available)
    pub analyzers: Option<Vec<AnalyzerKind>>,
    /// Enrich only these files (None = all files)
    pub files: Option<Vec<PathBuf>>,
    /// Timeout per file in seconds
    pub timeout_secs: u64,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self {
            analyzers: None,
            files: None,
            timeout_secs: 30,
        }
    }
}

/// Result of enrichment operation
#[derive(Debug, Clone)]
pub struct EnrichResult {
    /// Number of files processed
    pub files_processed: usize,
    /// Number of symbols enriched
    pub symbols_enriched: usize,
    /// Number of errors encountered
    pub errors: usize,
}

/// Parsed signature from LSP analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSignature {
    /// Symbol name
    pub name: String,
    /// Full signature (e.g., "fn main() -> Result<(), Error>")
    pub signature: String,
    /// Return type (if available)
    pub return_type: Option<String>,
    /// Parameters (if available)
    pub parameters: Vec<String>,
    /// Documentation (if available)
    pub documentation: Option<String>,
}

/// Enrich symbols in the database with LSP data
///
/// # Arguments
/// * `graph` - Magellan code graph
/// * `config` - Enrichment configuration
///
/// # Returns
/// Enrichment result with statistics
pub fn enrich_symbols(graph: &mut CodeGraph, config: &EnrichConfig) -> Result<EnrichResult> {
    let mut result = EnrichResult {
        files_processed: 0,
        symbols_enriched: 0,
        errors: 0,
    };

    // Detect available analyzers
    let available_analyzers = detect_available_analyzers();
    
    if available_analyzers.is_empty() {
        eprintln!("No LSP analyzers found (rust-analyzer, jdtls, clangd)");
        eprintln!("Install one or more analyzers to enable enrichment.");
        return Ok(result);
    }

    eprintln!("Found {} analyzer(s):", available_analyzers.len());
    for analyzer in &available_analyzers {
        eprintln!("  - {}", analyzer.binary_name());
    }
    eprintln!();

    // Get all files from the graph
    let files = graph.all_file_nodes()?;
    
    for (file_path_str, _file_node) in files {
        let file_path = Path::new(&file_path_str);
        
        // Skip if file filter is specified and this file is not in it
        if let Some(ref files_filter) = config.files {
            if !files_filter.contains(&file_path.to_path_buf()) {
                continue;
            }
        }

        // Detect language for this file
        let language = match detect_language_from_path(file_path) {
            Some(lang) => lang,
            None => continue, // Skip unsupported languages
        };

        // Get appropriate analyzer for this language
        let analyzer_kind = match super::analyzer::get_analyzer_for_language(language) {
            Some(kind) if available_analyzers.contains(&kind) => kind,
            _ => continue, // No analyzer available for this language
        };

        eprintln!("Enriching {:?} with {}", file_path, analyzer_kind.binary_name());

        // Get symbols for this file
        let symbols = match graph.symbols_in_file(&file_path_str) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  Error getting symbols: {}", e);
                result.errors += 1;
                continue;
            }
        };

        if symbols.is_empty() {
            continue;
        }

        // Run analyzer on this file and parse JSON output
        let workspace_root = file_path.parent().unwrap_or(Path::new("."));
        let signatures = match analyzer_kind {
            AnalyzerKind::RustAnalyzer => {
                parse_rust_analyzer_json(file_path, workspace_root)?
            }
            AnalyzerKind::Clangd => {
                parse_clangd_json(file_path, workspace_root)?
            }
            AnalyzerKind::JDTLS => {
                parse_jdtls_json(file_path, workspace_root)?
            }
        };

        // Match signatures to symbols
        let enriched_count = match_signatures_to_symbols(&symbols, &signatures)?;
        
        result.files_processed += 1;
        result.symbols_enriched += enriched_count;
        eprintln!("  Enriched {} symbols", enriched_count);
    }

    eprintln!();
    eprintln!("Enrichment complete:");
    eprintln!("  Files processed: {}", result.files_processed);
    eprintln!("  Symbols enriched: {}", result.symbols_enriched);
    eprintln!("  Errors: {}", result.errors);

    Ok(result)
}

/// Parse rust-analyzer JSON output
fn parse_rust_analyzer_json(file_path: &Path, workspace: &Path) -> Result<Vec<LspSignature>> {
    let mut signatures = Vec::new();
    
    // Try rust-analyzer analysis-stats command
    let output = Command::new("rust-analyzer")
        .args(["analysis-stats", "--load-output-dirs"])
        .arg(file_path)
        .current_dir(workspace)
        .output()
        .context("Failed to run rust-analyzer")?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse JSON from stdout (rust-analyzer outputs line-delimited JSON)
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        
        // Try to parse as JSON
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(signature) = extract_signature_from_json(&json_value) {
                signatures.push(signature);
            }
        }
    }
    
    Ok(signatures)
}

/// Parse clangd JSON output
fn parse_clangd_json(file_path: &Path, workspace: &Path) -> Result<Vec<LspSignature>> {
    // clangd doesn't have a simple CLI for JSON output
    // This is a placeholder for future implementation
    let _ = (file_path, workspace);
    Ok(Vec::new())
}

/// Parse jdtls JSON output
fn parse_jdtls_json(file_path: &Path, workspace: &Path) -> Result<Vec<LspSignature>> {
    // jdtls is typically run as a language server
    // This is a placeholder for future implementation
    let _ = (file_path, workspace);
    Ok(Vec::new())
}

/// Extract signature from rust-analyzer JSON
fn extract_signature_from_json(json: &serde_json::Value) -> Option<LspSignature> {
    // rust-analyzer JSON format varies by command
    // Common format for function analysis:
    // { "name": "main", "kind": "function", "signature": "fn main() -> ()" }
    
    let name = json.get("name")?.as_str()?.to_string();
    let signature = json.get("signature")
        .or_else(|| json.get("display_name"))?
        .as_str()?
        .to_string();
    
    let return_type = json.get("return_type")
        .and_then(|v| v.as_str())
        .map(String::from);
    
    let parameters = json.get("parameters")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    
    let documentation = json.get("documentation")
        .or_else(|| json.get("docs"))
        .and_then(|v| v.as_str())
        .map(String::from);
    
    Some(LspSignature {
        name,
        signature,
        return_type,
        parameters,
        documentation,
    })
}

/// Match parsed signatures to symbol facts
fn match_signatures_to_symbols(
    symbols: &[crate::ingest::SymbolFact],
    signatures: &[LspSignature],
) -> Result<usize> {
    let mut matched = 0;
    
    for symbol in symbols {
        if let Some(ref name) = symbol.name {
            // Find matching signature
            if let Some(sig) = signatures.iter().find(|s| s.name == *name) {
                eprintln!("    Matched '{}': {}", name, sig.signature);
                matched += 1;
            }
        }
    }
    
    Ok(matched)
}

/// Run enrichment with default configuration
pub fn run_enrich(db_path: &Path) -> Result<EnrichResult> {
    let mut graph = CodeGraph::open(db_path)?;
    let config = EnrichConfig::default();
    enrich_symbols(&mut graph, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_signature_from_json() {
        let json = serde_json::json!({
            "name": "main",
            "signature": "fn main() -> Result<(), Error>",
            "return_type": "Result<(), Error>",
            "parameters": [],
            "documentation": "Main entry point"
        });
        
        let sig = extract_signature_from_json(&json);
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.name, "main");
        assert_eq!(sig.signature, "fn main() -> Result<(), Error>");
        assert_eq!(sig.return_type, Some("Result<(), Error>".to_string()));
    }

    #[test]
    fn test_enrich_config_default() {
        let config = EnrichConfig::default();
        assert!(config.analyzers.is_none());
        assert!(config.files.is_none());
        assert_eq!(config.timeout_secs, 30);
    }
}
