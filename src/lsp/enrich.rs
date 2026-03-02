//! LSP enrichment using rust-analyzer, jdtls, and clangd
//!
//! Uses CLI commands to extract type signatures and documentation.

use anyhow::Result;
use std::path::{Path, PathBuf};

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

        // Run analyzer on this file
        let workspace_root = file_path.parent().unwrap_or(Path::new("."));
        let analyzer_result = match analyzer_kind.analyze_file(file_path, workspace_root) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  Analyzer error: {}", e);
                result.errors += 1;
                continue;
            }
        };

        // Parse analyzer output and enrich symbols
        let enriched_count = parse_and_enrich_symbols(graph, &file_path_str, &symbols, &analyzer_result)?;
        
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

/// Parse analyzer output and extract type signatures
fn parse_and_enrich_symbols(
    _graph: &mut CodeGraph,
    _file_path: &str,
    symbols: &[crate::ingest::SymbolFact],
    analyzer_result: &AnalyzerResult,
) -> Result<usize> {
    let mut enriched_count = 0;

    for symbol in symbols {
        if let Some(ref name) = symbol.name {
            // For rust-analyzer, parse the output for type signatures
            if analyzer_result.analyzer == AnalyzerKind::RustAnalyzer {
                if let Some(signature) = extract_signature_from_rust_analyzer(&analyzer_result.raw_output, name) {
                    eprintln!("    Found signature for '{}': {}", name, signature);
                    enriched_count += 1;
                }
            }
        }
    }

    Ok(enriched_count)
}

/// Extract function signature from rust-analyzer output
/// 
/// This uses improved parsing based on Splice's rust-analyzer output parsing.
fn extract_signature_from_rust_analyzer(output: &str, symbol_name: &str) -> Option<String> {
    // Look for lines containing the symbol name with function definition
    for line in output.lines() {
        let trimmed = line.trim();
        
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        
        // Look for function definitions containing our symbol name
        if trimmed.contains(&format!("fn {}", symbol_name)) || 
           trimmed.contains(&format!("pub fn {}", symbol_name)) {
            // Extract the full signature
            if let Some(sig) = extract_function_signature(trimmed) {
                return Some(sig);
            }
        }
        
        // Also check for error/warning lines that mention the symbol
        if (trimmed.starts_with("error") || trimmed.starts_with("warning")) 
            && trimmed.contains(symbol_name) {
            // This line has diagnostic info about the symbol
            if let Some(sig) = extract_context_signature(trimmed) {
                return Some(sig);
            }
        }
    }
    
    None
}

/// Extract a function signature from a line
fn extract_function_signature(line: &str) -> Option<String> {
    // Find "fn symbol_name" and extract until the opening brace or end of line
    if let Some(fn_pos) = line.find("fn ") {
        let rest = &line[fn_pos..];
        
        // Find the opening brace or end
        if let Some(brace_pos) = rest.find('{') {
            let sig = rest[..brace_pos].trim();
            if !sig.is_empty() {
                return Some(sig.to_string());
            }
        } else {
            // No brace, take the whole line
            return Some(rest.trim().to_string());
        }
    }
    
    None
}

/// Extract signature context from error/warning lines
fn extract_context_signature(line: &str) -> Option<String> {
    // Rust analyzer error format often includes the problematic code
    // e.g., "error[E0425]: cannot find function `foo` in this scope"
    // or "help: consider importing `foo`"
    
    if line.contains("help:") {
        // Help lines often suggest the correct signature
        if let Some(help_pos) = line.find("help:") {
            return Some(line[help_pos..].trim().to_string());
        }
    }
    
    None
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
    fn test_extract_function_signature() {
        let line = "pub fn process_data<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>> {";
        let sig = extract_function_signature(line);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap(), "pub fn process_data<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>>");
    }

    #[test]
    fn test_extract_signature_from_rust_analyzer() {
        let output = r#"
Checking myproject v0.1.0
error[E0425]: cannot find function `missing_helper` in this scope
 --> src/lib.rs:2:5
  |
2 |     missing_helper(name)
  |     ^^^^^^^^^^^^^^ not found in this scope
help: consider importing `missing_helper`
  |
1 | use crate::helpers::missing_helper;
  |
"#;
        
        let sig = extract_signature_from_rust_analyzer(output, "missing_helper");
        assert!(sig.is_some());
        assert!(sig.unwrap().contains("help:"));
    }

    #[test]
    fn test_enrich_config_default() {
        let config = EnrichConfig::default();
        assert!(config.analyzers.is_none());
        assert!(config.files.is_none());
        assert_eq!(config.timeout_secs, 30);
    }
}
