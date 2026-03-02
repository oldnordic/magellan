//! LSP enrichment using rust-analyzer, clangd, and javac
//!
//! Uses CLI commands to extract type signatures and documentation.
//! 
//! ## rust-analyzer
//! 
//! rust-analyzer provides JSON output through its analysis commands:
//! 
//! ```bash
//! # Get analysis stats (JSON lines)
//! rust-analyzer analysis-stats .
//! ```
//!
//! ## clangd
//!
//! clangd provides JSON output through clangd-query or direct AST parsing:
//!
//! ```bash
//! # Get AST in JSON format
//! clangd-query --dump-ast file.cpp
//! ```
//!
//! ## javac (Java)
//!
//! For Java, we use javac with annotation processing or parse source directly:
//!
//! ```bash
//! # Compile and extract signatures
//! javac -parameters -Xprint file.java
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::{BufRead, BufReader};

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
        eprintln!("No LSP analyzers found (rust-analyzer, javac)");
        eprintln!("Install rust-analyzer: rustup component add rust-analyzer");
        eprintln!("Install Java JDK for Java projects");
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
                parse_javac_output(file_path, workspace_root)?
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

/// Parse clangd JSON output using clangd-query
fn parse_clangd_json(file_path: &Path, workspace: &Path) -> Result<Vec<LspSignature>> {
    let mut signatures = Vec::new();
    
    // Check if file is C/C++
    let extension = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    
    let is_cpp = matches!(extension, "cpp" | "cc" | "cxx" | "hpp" | "h");
    
    // Use clangd-query to get AST information
    let output = Command::new("clangd-query")
        .args([
            "--dump-ast",
            "--include-refs",
            file_path.to_str().unwrap()
        ])
        .current_dir(workspace)
        .output();
    
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Parse clangd AST output (not JSON, but structured text)
            // Format: kind name type location
            for line in stdout.lines() {
                if let Some(sig) = parse_clangd_line(line, is_cpp) {
                    signatures.push(sig);
                }
            }
        }
        Err(_) => {
            // Fallback: use clang with -Xclang -ast-dump=json
            let output = Command::new("clang")
                .args([
                    "-Xclang",
                    "-ast-dump=json",
                    "-fsyntax-only",
                    file_path.to_str().unwrap()
                ])
                .current_dir(workspace)
                .output()
                .context("Failed to run clang AST dump")?;
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Parse JSON AST
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&stdout) {
                extract_signatures_from_clang_ast(&json_value, &mut signatures);
            }
        }
    }
    
    Ok(signatures)
}

/// Parse a single line from clangd-query output
fn parse_clangd_line(line: &str, is_cpp: bool) -> Option<LspSignature> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    
    // clangd-query format: "kind name type"
    // Example: "Function main int()"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    
    let kind = parts[0];
    let name = parts[1];
    let type_info = parts[2..].join(" ");
    
    match kind {
        "Function" | "Method" | "Constructor" | "Destructor" => {
            Some(LspSignature {
                name: name.to_string(),
                signature: if is_cpp {
                    format!("{} {}", kind, type_info)
                } else {
                    format!("int {}({})", name, type_info)
                },
                return_type: Some(type_info.clone()),
                parameters: vec![],
                documentation: None,
            })
        }
        "Class" | "Struct" | "Enum" => {
            Some(LspSignature {
                name: name.to_string(),
                signature: format!("{} {}", kind, name),
                return_type: None,
                parameters: vec![],
                documentation: None,
            })
        }
        _ => None,
    }
}

/// Extract signatures from clang JSON AST
fn extract_signatures_from_clang_ast(json: &serde_json::Value, signatures: &mut Vec<LspSignature>) {
    if let Some(obj) = json.as_object() {
        // Check if this is a function declaration
        if let Some(kind) = obj.get("kind").and_then(|v| v.as_str()) {
            if matches!(kind, "FunctionDecl" | "CXXMethodDecl" | "Constructor" | "Destructor") {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    let return_type = obj.get("type")
                        .and_then(|t| t.get("qualType"))
                        .and_then(|t| t.as_str())
                        .map(String::from);
                    
                    let signature = format!("{} {}", kind, name);
                    
                    signatures.push(LspSignature {
                        name: name.to_string(),
                        signature,
                        return_type,
                        parameters: vec![],
                        documentation: None,
                    });
                }
            }
        }
        
        // Recurse into children
        if let Some(children) = obj.get("inner").and_then(|v| v.as_array()) {
            for child in children {
                extract_signatures_from_clang_ast(child, signatures);
            }
        }
    }
}

/// Parse javac output for Java signatures
fn parse_javac_output(file_path: &Path, workspace: &Path) -> Result<Vec<LspSignature>> {
    let mut signatures = Vec::new();
    
    // Use javac -Xprint to print declarations
    let output = Command::new("javac")
        .args([
            "-Xprint",
            "-parameters",  // Include parameter names
            file_path.to_str().unwrap()
        ])
        .current_dir(workspace)
        .output()
        .context("Failed to run javac -Xprint")?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse javac -Xprint output
    // Format: modifiers class ClassName { ... }
    //         modifiers returnType methodName(params) { ... }
    for line in stdout.lines() {
        if let Some(sig) = parse_javac_line(line) {
            signatures.push(sig);
        }
    }
    
    Ok(signatures)
}

/// Parse a single line from javac -Xprint output
fn parse_javac_line(line: &str) -> Option<LspSignature> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("//") {
        return None;
    }
    
    // Class/Interface/Enum declaration
    if let Some(sig) = parse_java_type_decl(line) {
        return Some(sig);
    }
    
    // Method declaration
    if let Some(sig) = parse_java_method_decl(line) {
        return Some(sig);
    }
    
    None
}

/// Parse Java type declaration (class, interface, enum)
fn parse_java_type_decl(line: &str) -> Option<LspSignature> {
    // Match: [modifiers] class ClassName [extends ...] [implements ...] {
    // or: [modifiers] interface InterfaceName [extends ...] {
    // or: [modifiers] enum EnumName {
    
    let kind = if line.contains(" class ") {
        "class"
    } else if line.contains(" interface ") {
        "interface"
    } else if line.contains(" enum ") {
        "enum"
    } else {
        return None;
    };
    
    // Extract name
    let parts: Vec<&str> = line.split_whitespace().collect();
    let name_idx = parts.iter().position(|&p| p == kind)? + 1;
    let name = parts.get(name_idx)?.split('<').next()?;
    
    Some(LspSignature {
        name: name.to_string(),
        signature: line.split('{').next()?.trim().to_string(),
        return_type: None,
        parameters: vec![],
        documentation: None,
    })
}

/// Parse Java method declaration
fn parse_java_method_decl(line: &str) -> Option<LspSignature> {
    // Match: [modifiers] ReturnType methodName(params) [throws ...] {
    if !line.contains('(') || !line.contains(')') {
        return None;
    }
    
    // Skip lines that are clearly not methods
    if line.contains(" class ") || line.contains(" interface ") || line.contains(" enum ") {
        return None;
    }
    
    // Extract method name and parameters
    let paren_start = line.find('(')?;
    let paren_end = line.find(')')?;
    
    let before_parens = &line[..paren_start];
    let params_str = &line[paren_start + 1..paren_end];
    
    // Extract method name (last word before parenthesis)
    let name = before_parens.split_whitespace().last()?;
    
    // Extract return type (second to last word before parenthesis)
    let return_type: Option<String> = before_parens
        .split_whitespace()
        .rev()
        .nth(1)
        .map(String::from);
    
    // Parse parameters
    let parameters = parse_java_parameters(params_str);
    
    // Build signature
    let signature = line.split('{').next()?.trim().to_string();
    
    Some(LspSignature {
        name: name.to_string(),
        signature,
        return_type,
        parameters,
        documentation: None,
    })
}

/// Parse Java method parameters
fn parse_java_parameters(params_str: &str) -> Vec<String> {
    if params_str.trim().is_empty() {
        return vec![];
    }
    
    params_str
        .split(',')
        .map(|p| {
            let p = p.trim();
            // Extract just the parameter name (last word)
            p.split_whitespace().last().unwrap_or(p).to_string()
        })
        .collect()
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
    fn test_parse_clangd_line() {
        let sig = parse_clangd_line("Function main int()", false);
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.name, "main");
        assert!(sig.signature.contains("main"));
    }

    #[test]
    fn test_parse_javac_class() {
        let sig = parse_java_type_decl("public class MyClass {");
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.name, "MyClass");
        assert_eq!(sig.signature, "public class MyClass");
    }

    #[test]
    fn test_parse_javac_method() {
        let sig = parse_java_method_decl("public void myMethod(int x, String y) {");
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.name, "myMethod");
        assert_eq!(sig.return_type, Some("void".to_string()));
        assert_eq!(sig.parameters, vec!["x", "y"]);
    }

    #[test]
    fn test_parse_java_parameters() {
        let params = parse_java_parameters("int x, String y, boolean z");
        assert_eq!(params, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_enrich_config_default() {
        let config = EnrichConfig::default();
        assert!(config.analyzers.is_none());
        assert!(config.files.is_none());
        assert_eq!(config.timeout_secs, 30);
    }
}
